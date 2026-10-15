use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const APP_STATE_DIR: &str = ".chatgptx";
// 兼容历史状态目录名，用于首次启动一次性平滑迁移。
const LEGACY_APP_STATE_DIR: &str = concat!(".codex-", "session-delete");
const SETTINGS_FILE: &str = "settings.json";
const LATEST_STATUS_FILE: &str = "latest-status.json";
const DIAGNOSTIC_LOG_FILE: &str = "chatgptx.log";
const LEGACY_DIAGNOSTIC_LOG_FILE: &str = concat!("codex-", "plus.log");
const PENDING_PROVIDER_IMPORT_FILE: &str = "pending-provider-import.json";
const PENDING_SESSION_SHARE_FILE: &str = "pending-session-share.txt";
const PENDING_REMOTE_CONTROL_RECOVERY_FILE: &str = "pending-remote-control-recovery.json";
const SKILLS_STATE_FILE: &str = "skills.json";
const SKILLS_DIR: &str = "skills";
const SKILL_BACKUPS_DIR: &str = "skill-backups";
const PENDING_MANAGER_NAVIGATION_FILE: &str = "pending-manager-navigation.json";
const OFFICIAL_ACCOUNTS_FILE: &str = "official-accounts.json";

/// 显式指定状态目录的环境变量（Windows 多开隔离的必需项）。
pub const STATE_DIR_ENV: &str = "CHATGPTX_STATE_DIR";

/// 解析显式状态目录覆盖；未设置或设置为空时返回 `None`。
pub fn state_dir_override_from(value: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let value = value?;
    let text = value.to_string_lossy();
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

pub fn state_dir_override() -> Option<PathBuf> {
    state_dir_override_from(std::env::var_os(STATE_DIR_ENV).as_deref())
}

pub fn default_app_state_dir() -> PathBuf {
    if let Some(state_dir) = state_dir_override() {
        return state_dir;
    }

    if let Some(home_dir) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        let target_dir = home_dir.join(APP_STATE_DIR);
        migrate_legacy_state_dir_once(&home_dir, &target_dir);
        return target_dir;
    }

    PathBuf::from(APP_STATE_DIR)
}

static LEGACY_STATE_MIGRATED: OnceLock<()> = OnceLock::new();

fn migrate_legacy_state_dir_once(home_dir: &Path, target_dir: &Path) {
    LEGACY_STATE_MIGRATED.get_or_init(|| {
        let legacy_dir = home_dir.join(LEGACY_APP_STATE_DIR);
        let _ = migrate_legacy_state_dir_between(&legacy_dir, target_dir);
    });
}

/// 将旧状态目录中的数据一次性迁移到新的 `~/.chatgptx` 目录（若目标文件尚不存在）。
pub fn migrate_legacy_state_dir_between(
    legacy_dir: &Path,
    target_dir: &Path,
) -> std::io::Result<bool> {
    if !legacy_dir.is_dir() {
        return Ok(false);
    }
    let marker = target_dir.join(".legacy-migrated");
    if marker.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(target_dir)?;
    copy_dir_missing_entries(legacy_dir, target_dir)?;

    let legacy_log = legacy_dir.join(LEGACY_DIAGNOSTIC_LOG_FILE);
    let target_log = target_dir.join(DIAGNOSTIC_LOG_FILE);
    if legacy_log.is_file() && !target_log.exists() {
        let _ = std::fs::copy(&legacy_log, &target_log);
    }
    let _ = std::fs::write(&marker, b"1");
    Ok(true)
}

fn copy_dir_missing_entries(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        if file_type.is_dir() {
            copy_dir_missing_entries(&src_path, &dst_path)?;
        } else if file_type.is_file() && !dst_path.exists() {
            let _ = std::fs::copy(&src_path, &dst_path);
        }
    }
    Ok(())
}

pub fn default_settings_path() -> PathBuf {
    if let Some(path) = settings_path_for_tests() {
        return path;
    }
    default_app_state_dir().join(SETTINGS_FILE)
}

pub fn default_latest_status_path() -> PathBuf {
    default_app_state_dir().join(LATEST_STATUS_FILE)
}

pub fn default_diagnostic_log_path() -> PathBuf {
    default_app_state_dir().join(DIAGNOSTIC_LOG_FILE)
}

pub fn default_pending_provider_import_path() -> PathBuf {
    default_app_state_dir().join(PENDING_PROVIDER_IMPORT_FILE)
}

pub fn default_pending_session_share_path() -> PathBuf {
    default_app_state_dir().join(PENDING_SESSION_SHARE_FILE)
}

pub fn default_pending_remote_control_recovery_path() -> PathBuf {
    default_app_state_dir().join(PENDING_REMOTE_CONTROL_RECOVERY_FILE)
}

/// Skills 的「单一事实来源」目录。已安装的 skill 目录都放这里，
/// 启用时再软链到 `$CODEX_HOME/skills/<id>`，停用只删链接、源目录留着。
pub fn default_skills_source_dir() -> PathBuf {
    default_app_state_dir().join(SKILLS_DIR)
}

pub fn default_skills_state_path() -> PathBuf {
    default_app_state_dir().join(SKILLS_STATE_FILE)
}

/// 卸载 skill 时把源目录整体移到这里，方便反悔。不自动轮转删除。
pub fn default_skill_backups_dir() -> PathBuf {
    default_app_state_dir().join(SKILL_BACKUPS_DIR)
}

pub fn default_pending_manager_navigation_path() -> PathBuf {
    default_app_state_dir().join(PENDING_MANAGER_NAVIGATION_FILE)
}

pub fn default_official_accounts_path() -> PathBuf {
    default_app_state_dir().join(OFFICIAL_ACCOUNTS_FILE)
}

fn settings_path_for_tests() -> Option<PathBuf> {
    SETTINGS_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|path| path.clone())
}

static SETTINGS_PATH_FOR_TESTS: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[cfg(test)]
static SETTINGS_PATH_TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn settings_path_test_guard() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_PATH_TEST_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap()
}

pub fn set_settings_path_for_tests(path: Option<PathBuf>) -> Option<PathBuf> {
    SETTINGS_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut current| std::mem::replace(&mut *current, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_settings_path();

        assert!(path.ends_with(".chatgptx/settings.json"));
    }

    #[test]
    fn default_latest_status_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_latest_status_path();

        assert!(path.ends_with(".chatgptx/latest-status.json"));
    }

    #[test]
    fn default_diagnostic_log_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_diagnostic_log_path();

        assert!(path.ends_with(".chatgptx/chatgptx.log"));
    }

    #[test]
    fn default_pending_provider_import_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_provider_import_path();

        assert!(path.ends_with(".chatgptx/pending-provider-import.json"));
    }

    #[test]
    fn default_pending_session_share_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_session_share_path();

        assert!(path.ends_with(".chatgptx/pending-session-share.txt"));
    }

    #[test]
    fn default_pending_remote_control_recovery_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_remote_control_recovery_path();

        assert!(path.ends_with(".chatgptx/pending-remote-control-recovery.json"));
    }

    #[test]
    fn default_pending_manager_navigation_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_manager_navigation_path();

        assert!(path.ends_with(".chatgptx/pending-manager-navigation.json"));
    }

    #[test]
    fn state_dir_override_accepts_non_empty_paths_only() {
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new("/tmp/instance-state"))),
            Some(PathBuf::from("/tmp/instance-state"))
        );
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new("  /tmp/padded  "))),
            Some(PathBuf::from("/tmp/padded"))
        );
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new(""))),
            None
        );
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new("   "))),
            None
        );
        assert_eq!(state_dir_override_from(None), None);
    }

    #[test]
    fn default_app_state_dir_honours_explicit_override() {
        let _guard = settings_path_test_guard();
        let scope = StateDirEnvScope::set("/tmp/chatgptx-instance-state");

        assert_eq!(
            default_app_state_dir(),
            PathBuf::from("/tmp/chatgptx-instance-state")
        );

        drop(scope);
        assert!(default_app_state_dir().ends_with(".chatgptx"));
    }

    #[test]
    fn migrates_legacy_state_directory_once_without_overwriting_new_files() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join(LEGACY_APP_STATE_DIR);
        let target = temp.path().join(APP_STATE_DIR);
        std::fs::create_dir_all(legacy.join("skills/demo")).unwrap();
        std::fs::write(legacy.join("settings.json"), br#"{"activeRelayId":"legacy"}"#).unwrap();
        std::fs::write(legacy.join(LEGACY_DIAGNOSTIC_LOG_FILE), b"legacy-log").unwrap();
        std::fs::write(legacy.join("skills/demo/SKILL.md"), b"# Demo").unwrap();

        assert!(migrate_legacy_state_dir_between(&legacy, &target).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).unwrap(),
            r#"{"activeRelayId":"legacy"}"#
        );
        assert_eq!(
            std::fs::read_to_string(target.join("chatgptx.log")).unwrap(),
            "legacy-log"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("skills/demo/SKILL.md")).unwrap(),
            "# Demo"
        );

        // 第二次调用幂等跳过，不会覆盖新目录中的修改
        std::fs::write(target.join("settings.json"), br#"{"activeRelayId":"new"}"#).unwrap();
        assert!(!migrate_legacy_state_dir_between(&legacy, &target).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).unwrap(),
            r#"{"activeRelayId":"new"}"#
        );
    }

    struct StateDirEnvScope {
        previous: Option<std::ffi::OsString>,
    }

    impl StateDirEnvScope {
        fn set(value: &str) -> Self {
            let previous = std::env::var_os(STATE_DIR_ENV);
            unsafe { std::env::set_var(STATE_DIR_ENV, value) };
            Self { previous }
        }
    }

    impl Drop for StateDirEnvScope {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(previous) => unsafe { std::env::set_var(STATE_DIR_ENV, previous) },
                None => unsafe { std::env::remove_var(STATE_DIR_ENV) },
            }
        }
    }
}
