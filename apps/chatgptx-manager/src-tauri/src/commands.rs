use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use chatgptx_core::models::{DeleteResult, SessionRef};
use chatgptx_core::relay_environment::RelayEnvironmentReport;
use chatgptx_core::script_market::{self, MarketScript, ScriptMarketManifest};
use chatgptx_core::settings::{
    BackendSettings, RelayMode, RelayProfile, RelaySessionProvider, SettingsStore,
};
use chatgptx_core::status::{LaunchStatus, StatusStore};
use chatgptx_core::user_scripts::UserScriptManager;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::Emitter;

use crate::install::{self, InstallActionResult};

// 扫描快照仅驻留后端内存，不接受前端指定磁盘路径，不开放给扩展路由。
static AGENT_CACHE_PLAN: OnceLock<Mutex<Option<chatgptx_core::agent_cache::CachePlan>>> =
    OnceLock::new();

#[tauri::command]
pub fn dictation_local_model_status() -> Value {
    chatgptx_core::dictation::local_model_status()
}

/// 只注册为管理器命令，不开放给用户脚本或公开 HTTP 路由。
#[tauri::command]
pub async fn download_dictation_local_model() -> CommandResult<Value> {
    match chatgptx_core::dictation::prepare_local_model().await {
        Ok(state) => ok("本地 SenseVoice 模型下载并校验完成。", state),
        Err(error) => failed(&error.message, json!({})),
    }
}


#[tauri::command]
pub async fn scan_agent_cache() -> CommandResult<Value> {
    let result = tauri::async_runtime::spawn_blocking(|| -> anyhow::Result<Value> {
        let mut slot = AGENT_CACHE_PLAN
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| anyhow::anyhow!("缓存扫描状态不可用"))?;
        *slot = None;
        let plan = chatgptx_core::agent_cache::scan_default()?;
        let report = serde_json::to_value(&plan.report)?;
        *slot = Some(plan);
        Ok(json!({ "report": report }))
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok("缓存扫描完成。", payload),
        Ok(Err(error)) => failed(&error.to_string(), json!({})),
        Err(error) => failed(&format!("缓存扫描失败：{error}"), json!({})),
    }
}

#[tauri::command]
pub async fn clean_agent_cache(
    scan_id: String,
    group_ids: Vec<String>,
    confirmed: bool,
) -> CommandResult<Value> {
    let result = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Value> {
        anyhow::ensure!(confirmed, "尚未确认清理。");
        let mut slot = AGENT_CACHE_PLAN
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| anyhow::anyhow!("缓存扫描状态不可用"))?;
        let plan = slot
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("请先扫描缓存。"))?;
        plan.validate_selection(&scan_id, &group_ids, SystemTime::now())?;
        // 持锁消费快照，阻止并行扫描/清理和重复执行。
        let plan = slot.take().unwrap();
        let result = plan.clean(&scan_id, &group_ids, confirmed)?;
        Ok(json!({ "result": result }))
    })
    .await;
    match result {
        Ok(Ok(payload)) => ok("缓存清理完成。", payload),
        Ok(Err(error)) => failed(&error.to_string(), json!({})),
        Err(error) => failed(&format!("缓存清理失败：{error}"), json!({})),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandResult<T>
where
    T: Serialize,
{
    pub status: String,
    pub message: String,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionPayload {
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathState {
    pub status: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewPayload {
    pub codex_app: PathState,
    pub codex_version: Option<String>,
    pub silent_shortcut: PathState,
    pub management_shortcut: PathState,
    pub latest_launch: Option<LaunchStatus>,
    pub runtime_health: crate::runtime_health::RuntimeHealth,
    pub current_version: String,
    pub update_status: String,
    pub settings_path: String,
    pub logs_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsPayload {
    pub settings: BackendSettings,
    pub settings_path: String,
    pub user_scripts: Value,
}


#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcsProvidersPayload {
    pub db_path: String,
    pub configured_db_path: String,
    pub fallback_reason: Option<String>,
    pub providers: Vec<chatgptx_core::ccs_import::CcsProviderImport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProviderImportPayload {
    pub pending: Option<chatgptx_core::provider_import::ProviderImportRequest>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSessionsPayload {
    pub db_path: String,
    pub db_paths: Vec<String>,
    pub sessions: Vec<chatgptx_data::LocalSession>,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub total_count: usize,
}

const DEFAULT_LOCAL_SESSIONS_PAGE_SIZE: usize = 50;
const MAX_LOCAL_SESSIONS_PAGE_SIZE: usize = 100;

fn default_local_sessions_page_size() -> usize {
    DEFAULT_LOCAL_SESSIONS_PAGE_SIZE
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalSessionsRequest {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_local_sessions_page_size")]
    pub limit: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteLocalSessionRequest {
    pub session_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub db_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayPayload {
    pub authenticated: bool,
    pub auth_source: String,
    pub account_label: Option<String>,
    pub config_path: String,
    pub configured: bool,
    pub requires_openai_auth: bool,
    pub has_bearer_token: bool,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayFilesPayload {
    pub config_path: String,
    pub auth_path: String,
    pub config_contents: String,
    pub auth_contents: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySwitchPayload {
    pub settings: BackendSettings,
    pub relay: RelayPayload,
    pub settings_path: String,
    pub user_scripts: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBackfillPayload {
    pub settings: BackendSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntriesPayload {
    pub settings: BackendSettings,
    pub entries: chatgptx_core::relay_config::CodexContextEntries,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContextEntriesPayload {
    pub entries: chatgptx_core::relay_config::CodexContextEntries,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRelayCommonConfigPayload {
    pub common_config_contents: String,
    pub profile_config_contents: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProfileTestPayload {
    pub http_status: u16,
    pub endpoint: String,
    pub response_preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepwiseTestPayload {
    pub item_count: usize,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProfileModelsPayload {
    pub models: Vec<String>,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sub2ApiBillingPayload {
    pub endpoint: String,
    pub group_rate_multiplier: f64,
    pub user_rate_multiplier: Option<f64>,
    pub resolved_rate_multiplier: f64,
    pub peak_rate_enabled: bool,
    pub peak_rate_multiplier: Option<f64>,
    pub applied_peak_multiplier: Option<f64>,
    pub effective_rate_multiplier: f64,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDoctorCheck {
    pub id: String,
    pub title: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDoctorPayload {
    pub profile_name: String,
    pub model: String,
    pub summary: String,
    pub recommendation: String,
    pub checks: Vec<ProviderDoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvConflictsPayload {
    pub conflicts: Vec<chatgptx_core::env_conflicts::EnvConflict>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveEnvConflictsRequest {
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveEnvConflictsPayload {
    pub removed: Vec<chatgptx_core::env_conflicts::EnvConflictRemoval>,
    pub backup_path: Option<String>,
    pub remaining: Vec<chatgptx_core::env_conflicts::EnvConflict>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRelayFileRequest {
    pub kind: String,
    pub contents: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackfillRelayProfileRequest {
    pub settings: BackendSettings,
    pub profile_id: String,
    #[serde(default)]
    pub adopt_provider_identity: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSettingsRequest {
    pub settings: BackendSettings,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntryRequest {
    pub settings: BackendSettings,
    pub kind: String,
    pub id: String,
    pub toml_body: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDeleteRequest {
    pub settings: BackendSettings,
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRelayCommonConfigRequest {
    pub config_contents: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    #[serde(default)]
    pub app_path: String,
    #[serde(default = "default_debug_port")]
    pub debug_port: u16,
    #[serde(default = "default_helper_port")]
    pub helper_port: u16,
    #[serde(default)]
    pub sync_active_relay: bool,
}

/// 启动一个与管理器当前实例完全隔离的 Codex Desktop 实例。
///
/// 这个入口故意不复用 `launch_chatgptx`：后者只管理默认 CODEX_HOME，且
/// launcher/helper 的生命周期是单例。多开实例通过隐藏 worker 进程复用完整的
/// ChatGPTX launcher/injection 链，并给每个供应商分配独立的 CODEX_HOME、SQLite、
/// Electron user-data-dir 和 CDP/helper 端口。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultiLaunchRequest {
    /// 前端当前设置快照。省略时读取磁盘上的设置；API key 等不可序列化字段
    /// 会从磁盘中同 id 的 profile 补回。
    #[serde(default)]
    pub settings: Option<BackendSettings>,
    #[serde(default)]
    pub profile_id: String,
    /// 同一身份再次点击会复用这个目录，默认使用 profile id。
    #[serde(default)]
    pub instance_id: String,
    #[serde(default)]
    pub app_path: String,
    /// 为 0 时按 instance id 选择一个可用端口。
    #[serde(default)]
    pub debug_port: u16,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultiWorkerOptions {
    pub app_path: String,
    pub debug_port: u16,
    pub helper_port: u16,
    /// 实例自己的 ChatGPTX 状态目录。
    ///
    /// 多开隔离原先靠给 worker 换 `HOME`，但这只在 Unix 成立：`directories` 在
    /// Windows 上用 `SHGetKnownFolderPath(FOLDERID_Profile)`，完全不读 `HOME`，
    /// 于是 worker 会读写主实例的 `settings.json` / `latest-status.json`，
    /// 多开专用设置（含 `--user-data-dir`）从未被读取。这里改成 worker 自己以
    /// 显式路径为准（同时仍设置 `CHATGPTX_STATE_DIR` 覆盖子进程）。
    #[serde(default)]
    pub state_dir: String,
}

/// 一个已经由「Codex 多开」创建过的实例。记录文件放在实例自己的隔离目录，
/// 因此管理器重启后仍然可以找回它，不依赖当前进程内存。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultiInstance {
    pub instance_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub pid: Option<u32>,
    pub debug_port: u16,
    pub helper_port: u16,
    pub status: String,
    pub running: bool,
    pub started_at_ms: u64,
    pub is_main: bool,
    pub app_path: Option<String>,
    pub codex_home: Option<String>,
    pub sqlite_home: Option<String>,
    pub user_data_dir: Option<String>,
    /// worker 写入的启动结果，用于把「起不来」如实反映到界面。
    ///
    /// 旧版 manifest 没有这个字段，`serde(default)` 保证升级后仍能读出来。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_result: Option<CodexMultiLaunchResult>,
}

/// 多开 worker 的启动结果（落盘在实例目录，父进程按需读取）。
///
/// 多开的 worker 是独立进程，stdout/stderr 被丢弃，父进程只能靠这个文件知道
/// 「实例到底起没起来」，否则界面会在窗口根本没出现时显示「运行中」。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultiLaunchResult {
    /// 成功或失败都写一条；`failed` 表示激活/就绪校验没过。
    pub status: String,
    pub message: String,
    #[serde(default)]
    pub finished_at_ms: u64,
    #[serde(default)]
    pub debug_port: Option<u16>,
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultiInstancesPayload {
    pub instances: Vec<CodexMultiInstance>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultiInstanceRequest {
    pub instance_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRequest {
    #[serde(default = "default_log_lines")]
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogsPayload {
    pub path: String,
    pub text: String,
    pub lines: usize,
    pub truncated: bool,
    pub file_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsPayload {
    pub report: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WatcherPayload {
    pub enabled: bool,
    pub disabled_flag: String,
}


#[derive(Debug, Clone, Serialize)]
pub struct ScriptMarketPayload {
    pub market: Value,
    pub user_scripts: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsPayload {
    pub skills: Vec<chatgptx_core::skills::SkillEntry>,
    pub repos: Vec<chatgptx_core::skills::SkillRepo>,
    pub backups: Vec<chatgptx_core::skills::SkillBackup>,
    /// 单个仓库拉取失败不该让整块面板空掉，把错误单独带回前端提示。
    pub repo_errors: Vec<String>,
    pub skills_dir: String,
    pub codex_skills_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupPayload {
    pub show_update: bool,
}


#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEntry {
    /// 工具的稳定标识，如 `codex` / `grok`。
    pub id: String,
    pub name: String,
    /// 该工具的配置根目录，UI 上作为副标题展示。
    pub home_dir: String,
    /// 供应商 profile 的写盘能力是否已接好；未接好的工具在 UI 上显示为不可切。
    pub switchable: bool,
    pub active: bool,
    /// 该工具当前选中的供应商名，没配置时为 None。
    pub active_relay_name: Option<String>,
    /// 该工具名下已保存的供应商数量。
    pub relay_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsPayload {
    pub tools: Vec<ToolEntry>,
    pub active_tool: String,
}

/// 列出所有已注册的工具及其当前状态，供顶栏切换条渲染。
///
/// 注意：这里只读，不写盘。切换「当前聚焦的工具」由 save_settings 负责，
/// 因为那只是 UI 状态，不该触发任何配置文件写入。
#[tauri::command]
pub fn list_tools() -> CommandResult<ToolsPayload> {
    let settings = SettingsStore::default().load().unwrap_or_default();
    let tools = chatgptx_core::tools::tool_specs()
        .into_iter()
        .map(|spec| {
            let config = settings.tool_config(&spec.id);
            let active_relay_name = config
                .relay_profiles
                .iter()
                .find(|profile| profile.id == config.active_relay_id)
                .map(|profile| profile.name.clone());
            ToolEntry {
                id: spec.id.as_str().to_string(),
                name: spec.name,
                home_dir: spec.home_dir,
                switchable: spec.switchable,
                active: settings.active_tool == spec.id,
                active_relay_name,
                relay_count: config.relay_profiles.len(),
            }
        })
        .collect();

    let payload = ToolsPayload {
        tools,
        active_tool: settings.active_tool.as_str().to_string(),
    };
    ok("工具列表已加载。", payload)
}

#[tauri::command]
pub fn backend_version() -> CommandResult<VersionPayload> {
    ok(
        "后端版本已读取。",
        VersionPayload {
            version: chatgptx_core::version::VERSION.to_string(),
        },
    )
}

#[tauri::command]
pub fn startup_options() -> CommandResult<StartupPayload> {
    ok(
        "启动参数已读取。",
        StartupPayload {
            show_update: startup_should_show_update(),
        },
    )
}

#[tauri::command]
pub fn consume_pending_manager_navigation()
-> Result<Option<chatgptx_core::manager_navigation::ManagerNavigationIntent>, String> {
    chatgptx_core::manager_navigation::consume_pending_manager_navigation()
        .map_err(|error| error.to_string())
}

pub fn startup_should_show_update() -> bool {
    should_show_update(
        std::env::args(),
        std::env::var("CHATGPTX_SHOW_UPDATE").ok().as_deref(),
    )
}

fn should_show_update<I, S>(args: I, env_value: Option<&str>) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--show-update") || env_value == Some("1")
}

#[tauri::command]
pub async fn load_overview() -> CommandResult<OverviewPayload> {
    let payload = tauri::async_runtime::spawn_blocking(load_overview_payload).await;
    let Ok((codex_app_path, entrypoints, latest_launch, runtime_health)) = payload else {
        return failed(
            "概览后台任务失败。",
            OverviewPayload {
                codex_app: path_state(None),
                codex_version: None,
                silent_shortcut: path_state(None),
                management_shortcut: path_state(None),
                latest_launch: None,
                runtime_health: crate::runtime_health::RuntimeHealth::unavailable(),
                current_version: chatgptx_core::version::VERSION.to_string(),
                update_status: "not_checked".to_string(),
                settings_path: chatgptx_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                logs_path: chatgptx_core::paths::default_diagnostic_log_path()
                    .to_string_lossy()
                    .to_string(),
            },
        );
    };
    ok(
        "概览已加载。",
        OverviewPayload {
            codex_version: codex_app_path
                .as_deref()
                .and_then(chatgptx_core::app_paths::codex_app_version),
            codex_app: path_state(codex_app_path),
            silent_shortcut: shortcut_state(entrypoints.silent_shortcut),
            management_shortcut: shortcut_state(entrypoints.management_shortcut),
            latest_launch,
            runtime_health,
            current_version: chatgptx_core::version::VERSION.to_string(),
            update_status: "not_checked".to_string(),
            settings_path: chatgptx_core::paths::default_settings_path()
                .to_string_lossy()
                .to_string(),
            logs_path: chatgptx_core::paths::default_diagnostic_log_path()
                .to_string_lossy()
                .to_string(),
        },
    )
}

#[tauri::command]
pub async fn launch_chatgptx(request: LaunchRequest) -> CommandResult<Value> {
    match tauri::async_runtime::spawn_blocking(move || {
        let _guard = match try_acquire_launch_guard() {
            Ok(guard) => guard,
            Err(message) => return failed(message, json!({})),
        };
        spawn_chatgptx_launch(request, "启动任务已在后台开始，可稍后查看概览状态。")
    })
    .await
    {
        Ok(result) => result,
        Err(error) => failed(&format!("启动后台任务失败：{error}"), json!({})),
    }
}

#[tauri::command]
pub async fn launch_codex_multi_instance(request: CodexMultiLaunchRequest) -> CommandResult<Value> {
    let maybe_acct_id = request
        .profile_id
        .trim()
        .strip_prefix("official:")
        .unwrap_or_else(|| request.profile_id.trim())
        .to_string();
    if !maybe_acct_id.is_empty() {
        let _ = chatgptx_core::official_accounts::prepare_official_account_for_switch(&maybe_acct_id).await;
    }
    match tauri::async_runtime::spawn_blocking(move || {
        launch_codex_multi_instance_blocking(request)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => failed(&format!("启动 Codex 多开实例失败：{error}"), json!({})),
    }
}

fn launch_codex_multi_instance_blocking(request: CodexMultiLaunchRequest) -> CommandResult<Value> {
    let stored_settings = match SettingsStore::default().load() {
        Ok(settings) => settings,
        Err(error) => return failed(&format!("读取供应商设置失败：{error}"), json!({})),
    };
    let mut settings = request
        .settings
        .clone()
        .unwrap_or_else(|| stored_settings.clone());
    let profile_id = if request.profile_id.trim().is_empty() {
        settings.active_relay_id.clone()
    } else {
        request.profile_id.trim().to_string()
    };

    let official_acct_id = profile_id
        .strip_prefix("official:")
        .unwrap_or(profile_id.as_str());
    let official_account = chatgptx_core::official_accounts::load_official_accounts_state()
        .accounts
        .into_iter()
        .find(|a| a.id == official_acct_id);

    let (profile, explicit_official_auth_json, is_pure_official_instance) =
        if let Some(account) = official_account {
            let bound_relay = account
                .bound_relay_profile_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .and_then(|bound_id| {
                    stored_settings
                        .relay_profiles
                        .iter()
                        .find(|p| p.id == bound_id)
                        .cloned()
                });
            if let Some(mut bound) = bound_relay {
                if bound.relay_mode == chatgptx_core::settings::RelayMode::Aggregate
                    || bound.has_model_routes()
                {
                    return failed(
                        "该官方账号绑定的供应商使用了聚合或模型路由，暂不支持独立多开。",
                        json!({ "profileId": profile_id }),
                    );
                }
                let auth_json = match chatgptx_core::official_accounts::build_auth_json_for_account(
                    &account,
                    Some(&bound.auth_contents),
                    true,
                ) {
                    Ok(v) => v,
                    Err(err) => {
                        return failed(
                            &format!("生成官方账号多开凭据失败：{err}"),
                            json!({ "profileId": profile_id }),
                        );
                    }
                };
                bound.id = format!("official:{}", account.id);
                bound.name = format!("{} · {}", account.email, bound.name);
                bound.auth_contents = auth_json.clone();
                (bound, Some(auth_json), false)
            } else {
                let auth_json = match chatgptx_core::official_accounts::build_auth_json_for_account(
                    &account,
                    None,
                    false,
                ) {
                    Ok(v) => v,
                    Err(err) => {
                        return failed(
                            &format!("生成官方账号多开凭据失败：{err}"),
                            json!({ "profileId": profile_id }),
                        );
                    }
                };
                let display_name = account
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|lbl| format!("{} ({lbl})", account.email))
                    .unwrap_or_else(|| account.email.clone());
                let pure_official = chatgptx_core::settings::RelayProfile {
                    id: format!("official:{}", account.id),
                    name: display_name,
                    relay_mode: chatgptx_core::settings::RelayMode::Official,
                    official_mix_api_key: false,
                    auth_contents: auth_json.clone(),
                    use_common_config: true,
                    ..chatgptx_core::settings::RelayProfile::default()
                };
                (pure_official, Some(auth_json), true)
            }
        } else {
            let Some(profile_index) = settings
                .relay_profiles
                .iter()
                .position(|profile| profile.id == profile_id)
            else {
                return failed(
                    &format!("供应商身份不存在：{profile_id}"),
                    json!({ "profileId": profile_id }),
                );
            };
            if let Some(saved_profile) = stored_settings
                .relay_profiles
                .iter()
                .find(|profile| profile.id == profile_id)
            {
                let profile = &mut settings.relay_profiles[profile_index];
                if profile.api_key.trim().is_empty() {
                    profile.api_key = saved_profile.api_key.clone();
                }
                if profile.auth_contents.trim().is_empty() {
                    profile.auth_contents = saved_profile.auth_contents.clone();
                }
            }
            let profile = settings.relay_profiles[profile_index].clone();
            if profile.relay_mode == chatgptx_core::settings::RelayMode::Aggregate {
                return failed(
                    "Codex 多开暂不支持需要共享本地聚合代理的供应商，请选择纯 API 或官方身份。",
                    json!({ "profileId": profile_id }),
                );
            }
            if profile.has_model_routes() {
                return failed(
                    "Codex 多开暂不支持模型路由，请选择单一上游供应商。",
                    json!({ "profileId": profile_id }),
                );
            }
            (profile, None, false)
        };
    let instance_id = if request.instance_id.trim().is_empty() {
        profile_id.clone()
    } else {
        request.instance_id.trim().to_string()
    };
    let slug = multi_instance_slug(&instance_id);
    let root = multi_instance_root(&slug);
    if let Ok(existing) = load_multi_instance_manifest(&root)
        && multi_instance_is_running(&existing)
    {
        return ok(
            "Codex 多开实例已经在运行。",
            json!({
                "instanceId": existing.instance_id,
                "profileId": existing.profile_id,
                "pid": existing.pid,
                "debugPort": existing.debug_port,
                "helperPort": existing.helper_port,
                "codexHome": existing.codex_home,
                "sqliteHome": existing.sqlite_home,
                "userDataDir": existing.user_data_dir,
                "appPath": existing.app_path
            }),
        );
    }
    // 先分配端口，再生成 config.toml。Chat Completions 通过实例专属协议代理
    // 转换，配置里的本地 base_url 必须与这个 helper 端口一致。
    let debug_port = select_multi_instance_port(request.debug_port, &instance_id);
    let helper_port = select_multi_instance_port(
        debug_port.saturating_add(1),
        &format!("{instance_id}-helper"),
    );
    let codex_home = root.join("codex-home");
    let sqlite_home = root.join("sqlite-home");
    let user_data_dir = multi_instance_user_data_dir(&slug);
    if let Err(error) = std::fs::create_dir_all(&codex_home)
        .and_then(|_| std::fs::create_dir_all(&sqlite_home))
        .and_then(|_| std::fs::create_dir_all(&user_data_dir))
    {
        return failed(
            &format!("创建 Codex 多开隔离目录失败：{error}"),
            json!({ "instanceId": instance_id }),
        );
    }

    // 官方 profile 通常没有 authContents；复制一次当前默认 CODEX_HOME 的登录态，
    // 用户也可以在新窗口中重新登录。纯 API 实例不复制官方 auth，避免两个身份
    // 重新混在一起。永远不覆盖实例已有 auth.json。
    let default_home = chatgptx_core::relay_config::default_codex_home_dir();
    let instance_auth = codex_home.join("auth.json");
    if let Some(auth_json) = explicit_official_auth_json.as_deref() {
        let _ = chatgptx_core::settings::atomic_write(&instance_auth, auth_json.as_bytes());
    } else if profile.relay_mode == chatgptx_core::settings::RelayMode::Official
        && !instance_auth.exists()
    {
        let default_auth = default_home.join("auth.json");
        if let Ok(contents) = std::fs::read(&default_auth) {
            let _ = chatgptx_core::settings::atomic_write(&instance_auth, &contents);
        }
    }
    let common_config = if settings.relay_profiles_enabled {
        settings.relay_common_config_contents.as_str()
    } else {
        ""
    };
    if let Err(error) =
        chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules_and_proxy_port(
            &codex_home,
            &profile,
            common_config,
            helper_port,
        )
    {
        return failed(
            &format!("应用配置到多开实例失败：{error}"),
            json!({
                "profileId": profile_id,
                "codexHome": codex_home.to_string_lossy()
            }),
        );
    }
    if let Some(auth_json) = explicit_official_auth_json.as_deref() {
        let _ = chatgptx_core::official_accounts::write_auth_json_and_sync_store(&codex_home, auth_json);
        let _ = chatgptx_core::official_accounts::align_forced_login_method_in_config(&codex_home, "chatgpt");
    }
    if is_pure_official_instance {
        let cfg_path = codex_home.join("config.toml");
        if let Ok(existing_cfg) = std::fs::read_to_string(&cfg_path)
            && let Ok(mut doc) = existing_cfg.parse::<toml_edit::DocumentMut>()
        {
            for key in [
                "model",
                "model_provider",
                "model_catalog_json",
                "openai_base_url",
                "base_url",
                "experimental_bearer_token",
                "requires_openai_auth",
            ] {
                doc.remove(key);
            }
            if let Some(providers) = doc
                .get_mut("model_providers")
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                providers.remove("custom");
            }
            let _ = chatgptx_core::settings::atomic_write(&cfg_path, doc.to_string().as_bytes());
        }
    } else if explicit_official_auth_json.is_some() {
        let cfg_path = codex_home.join("config.toml");
        if let Ok(existing_cfg) = std::fs::read_to_string(&cfg_path)
            && let Ok(mut doc) = existing_cfg.parse::<toml_edit::DocumentMut>()
        {
            let provider_key = doc
                .get("model_provider")
                .and_then(toml_edit::Item::as_str)
                .map(str::trim)
                .unwrap_or("custom")
                .to_string();
            if let Some(providers) = doc
                .get_mut("model_providers")
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                if let Some(p) = providers.get_mut(&provider_key).and_then(toml_edit::Item::as_table_like_mut) {
                    p.insert("requires_openai_auth", toml_edit::value(true));
                }
            }
            let _ = chatgptx_core::settings::atomic_write(&cfg_path, doc.to_string().as_bytes());
        }
    } else {
        let _ = chatgptx_core::official_accounts::align_forced_login_method_in_config(&codex_home, "api");
        chatgptx_core::official_accounts::delete_codex_keychain_if_present(&codex_home);
    }
    if let Err(error) = absolutize_multi_instance_model_catalog(&codex_home) {
        return failed(
            &format!("修正多开实例 model_catalog_json 路径失败：{error}"),
            json!({ "profileId": profile_id, "codexHome": codex_home.to_string_lossy() }),
        );
    }
    if let Err(error) = remove_invalid_multi_instance_mcp_servers(&codex_home) {
        return failed(
            &format!("清理多开实例无效 MCP 配置失败：{error}"),
            json!({ "profileId": profile_id, "codexHome": codex_home.to_string_lossy() }),
        );
    }
    if let Err(error) = seed_multi_instance_onboarding_state(&codex_home) {
        return failed(
            &format!("写入多开实例首次启动状态失败：{error}"),
            json!({ "profileId": profile_id, "codexHome": codex_home.to_string_lossy() }),
        );
    }

    let app_dir = chatgptx_core::app_paths::resolve_codex_app_dir_with_saved(
        (!request.app_path.trim().is_empty()).then(|| Path::new(request.app_path.trim())),
        Some(settings.codex_app_path.as_str()),
    );
    let Some(app_dir) = app_dir else {
        return failed("未找到 Codex Desktop，请先设置应用路径。", json!({}));
    };
    let executable = chatgptx_core::app_paths::build_codex_executable(&app_dir);
    // Windows 上包内 exe 的 `is_file()` 可能因 WindowsApps 的 ACL 返回假
    // （仓库在 app_paths.rs 里记录过「常因 ACL 无法枚举」）。目录在就不要误报
    // 「可执行文件不存在」——真正的结论交给启动后的就绪校验。
    if !executable.is_file() && !app_dir.is_dir() {
        return failed(
            &format!("Codex 可执行文件不存在：{}", executable.display()),
            json!({ "appPath": app_dir.to_string_lossy() }),
        );
    }
    let launcher_guard_port = select_multi_instance_port(0, &format!("{instance_id}-guard"));
    let worker_home = root.join("worker-home");
    let worker_state_dir = worker_home.join(".chatgptx");
    if let Err(error) = std::fs::create_dir_all(&worker_state_dir) {
        return failed(
            &format!("创建多开 launcher 状态目录失败：{error}"),
            json!({ "instanceId": instance_id }),
        );
    }
    let worker_settings_path = worker_state_dir.join("settings.json");
    let mut worker_settings = settings.clone();
    worker_settings.codex_app_path = app_dir.to_string_lossy().to_string();
    worker_settings.active_relay_id = profile.id.clone();
    worker_settings.relay_profiles = vec![profile.clone()];
    worker_settings.aggregate_relay_profiles.clear();
    worker_settings.active_aggregate_relay_id.clear();
    worker_settings.provider_sync_enabled = false;
    // Chat Completions（例如 Zhipu GLM）需要 launcher 启动实例自己的协议代理。
    // 保持 relay_profiles_enabled，让 `ensure_active_protocol_proxy_config_in_home`
    // 用多开进程的 CHATGPTX_PROTOCOL_PROXY_PORT 修正 config.toml 中的本地代理地址；
    // 否则父进程写入的默认 57321 会让多个实例互相抢同一个代理端口。
    worker_settings.relay_profiles_enabled =
        profile.protocol == chatgptx_core::settings::RelayProtocol::ChatCompletions;
    worker_settings.codex_extra_args = multi_instance_extra_args(&settings.codex_extra_args);
    worker_settings
        .codex_extra_args
        .push(format!("--user-data-dir={}", user_data_dir.display()));
    if let Err(error) = SettingsStore::new(worker_settings_path).save(&worker_settings) {
        return failed(
            &format!("保存多开 launcher 设置失败：{error}"),
            json!({ "instanceId": instance_id }),
        );
    }
    let worker_options_path = root.join("worker-options.json");
    let worker_options = CodexMultiWorkerOptions {
        app_path: app_dir.to_string_lossy().to_string(),
        debug_port,
        helper_port,
        state_dir: worker_state_dir.to_string_lossy().to_string(),
    };
    let worker_options_bytes = match serde_json::to_vec_pretty(&worker_options) {
        Ok(bytes) => bytes,
        Err(error) => {
            return failed(
                &format!("序列化多开 worker 参数失败：{error}"),
                json!({ "instanceId": instance_id }),
            );
        }
    };
    if let Err(error) =
        chatgptx_core::settings::atomic_write(&worker_options_path, &worker_options_bytes)
    {
        return failed(
            &format!("保存多开 worker 参数失败：{error}"),
            json!({ "instanceId": instance_id }),
        );
    }
    let manager_executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return failed(
                &format!("定位多开 launcher 可执行文件失败：{error}"),
                json!({ "instanceId": instance_id }),
            );
        }
    };
    let shared_config_home = directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".config"))
        .unwrap_or_else(|| PathBuf::from(".config"));
    let mut command = std::process::Command::new(manager_executable);
    command
        .arg("--chatgptx-multi-worker")
        .arg(&worker_options_path)
        .env("HOME", &worker_home)
        .env("CODEX_HOME", &codex_home)
        .env("CODEX_SQLITE_HOME", &sqlite_home)
        .env("CHATGPTX_MULTI_INSTANCE", "1")
        .env("CHATGPTX_INSTANCE_ID", &instance_id)
        .env("CHATGPTX_INSTANCE_LABEL", &profile.name)
        .env(
            "CHATGPTX_LAUNCHER_GUARD_PORT",
            launcher_guard_port.to_string(),
        )
        .env("CHATGPTX_PROTOCOL_PROXY_PORT", helper_port.to_string())
        .env("XDG_CONFIG_HOME", &shared_config_home)
        .env(
            chatgptx_core::paths::STATE_DIR_ENV,
            worker_state_dir.as_os_str(),
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(chatgptx_core::windows_create_no_window());
    }
    // 每次启动前清掉上一轮的启动结果，避免旧结果被当成这一次的结论。
    let _ = std::fs::remove_file(root.join(MULTI_LAUNCH_RESULT_FILE));
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return failed(
                &format!("启动 Codex 多开实例失败：{error}"),
                json!({ "executable": executable.to_string_lossy() }),
            );
        }
    };
    let pid = child.id();
    let instance = CodexMultiInstance {
        instance_id: instance_id.clone(),
        profile_id: profile_id.clone(),
        profile_name: profile.name.clone(),
        pid: Some(pid),
        debug_port,
        helper_port,
        status: "starting".to_string(),
        // 这里只说「worker 已拉起」。真正的可用性由调试端口与启动结果文件决定，
        // 否则 Windows 上起不来时界面会一直显示「运行中」。
        running: false,
        started_at_ms: current_timestamp_ms(),
        is_main: false,
        app_path: Some(app_dir.to_string_lossy().to_string()),
        codex_home: Some(codex_home.to_string_lossy().to_string()),
        sqlite_home: Some(sqlite_home.to_string_lossy().to_string()),
        user_data_dir: Some(user_data_dir.to_string_lossy().to_string()),
        launch_result: None,
    };
    if let Err(error) = save_multi_instance_manifest(&root, &instance) {
        let _ = terminate_multi_worker(pid, &worker_options_path);
        return failed(
            &format!("保存 Codex 多开实例信息失败：{error}"),
            json!({ "instanceId": instance_id }),
        );
    }
    // Child drop 不会终止 Unix 子进程；Codex Desktop 自己持有生命周期。
    drop(child);
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
        "manager.multi_instance_started",
        json!({
            "instance_id": instance_id,
            "profile_id": profile_id,
            "debug_port": debug_port,
            "helper_port": helper_port,
            "codex_home": codex_home,
            "sqlite_home": sqlite_home,
            "user_data_dir": user_data_dir,
            "pid": pid
        }),
    );
    ok(
        "Codex 多开实例已启动。",
        json!({
            "instanceId": instance_id,
            "profileId": profile_id,
            "pid": pid,
            "debugPort": debug_port,
            "homeDir": codex_home.to_string_lossy(),
            "codexHome": codex_home.to_string_lossy(),
            "sqliteHome": sqlite_home.to_string_lossy(),
            "userDataDir": user_data_dir.to_string_lossy(),
            "appPath": app_dir.to_string_lossy()
        }),
    )
}

#[tauri::command]
pub async fn list_codex_multi_instances() -> CommandResult<CodexMultiInstancesPayload> {
    match tauri::async_runtime::spawn_blocking(list_codex_multi_instances_blocking).await {
        Ok(result) => result,
        Err(error) => failed(
            &format!("读取 Codex 运行实例失败：{error}"),
            CodexMultiInstancesPayload {
                instances: Vec::new(),
            },
        ),
    }
}

fn list_codex_multi_instances_blocking() -> CommandResult<CodexMultiInstancesPayload> {
    let mut instances = Vec::new();
    if let Ok(settings) = SettingsStore::default().load()
        && let Ok(Some(latest)) = StatusStore::default().load_latest()
    {
        let debug_port = latest.debug_port.unwrap_or(default_debug_port());
        // 主实例同样以端口为准：`find_codex_processes_for_debug_port` 在 Windows 上
        // 不区分端口，用它会让主实例行在任何 Codex 进程存在时都显示运行中。
        let running = chatgptx_core::cdp::endpoint_available(debug_port);
        if running {
            let profile = settings
                .relay_profiles
                .iter()
                .find(|profile| profile.id == settings.active_relay_id);
            instances.push(CodexMultiInstance {
                instance_id: "main".to_string(),
                profile_id: settings.active_relay_id.clone(),
                profile_name: profile
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "主进程".to_string()),
                pid: None,
                debug_port,
                helper_port: latest.helper_port.unwrap_or(default_helper_port()),
                status: latest.status,
                running: true,
                started_at_ms: latest.started_at_ms,
                is_main: true,
                app_path: latest.codex_app,
                codex_home: Some(
                    chatgptx_core::relay_config::default_codex_home_dir()
                        .to_string_lossy()
                        .to_string(),
                ),
                sqlite_home: None,
                user_data_dir: None,
                launch_result: None,
            });
        }
    }

    let root = chatgptx_core::paths::default_app_state_dir().join("codex-multi");
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(instance) = load_multi_instance_manifest(&path) else {
                continue;
            };
            let running = multi_instance_is_running(&instance);
            let mut current = instance;
            current.launch_result = load_multi_launch_result(&path);
            current.running = running;
            current.status =
                multi_instance_status(running, current.launch_result.as_ref()).to_string();
            if !running {
                current.pid = None;
            }
            instances.push(current);
        }
    }
    instances.sort_by(|left, right| {
        left.is_main
            .cmp(&right.is_main)
            .reverse()
            .then_with(|| left.profile_name.cmp(&right.profile_name))
    });
    ok(
        "Codex 运行实例已加载。",
        CodexMultiInstancesPayload { instances },
    )
}

#[tauri::command]
pub async fn stop_codex_multi_instance(
    request: CodexMultiInstanceRequest,
) -> CommandResult<CodexMultiInstancesPayload> {
    match tauri::async_runtime::spawn_blocking(move || {
        stop_codex_multi_instance_blocking(&request.instance_id)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => failed(
            &format!("结束 Codex 多开实例失败：{error}"),
            CodexMultiInstancesPayload {
                instances: Vec::new(),
            },
        ),
    }
}

fn stop_codex_multi_instance_blocking(
    instance_id: &str,
) -> CommandResult<CodexMultiInstancesPayload> {
    if instance_id.trim() == "main" {
        let debug_port = StatusStore::default()
            .load_latest()
            .ok()
            .flatten()
            .and_then(|s| s.debug_port)
            .filter(|p| *p > 0)
            .unwrap_or(9229);
        if let Err(err) = stop_main_codex_blocking(debug_port) {
            return failed(
                &err,
                CodexMultiInstancesPayload {
                    instances: Vec::new(),
                },
            );
        }
        return list_codex_multi_instances_blocking();
    }
    let path = multi_instance_root(&multi_instance_slug(instance_id));
    let Some(instance) = load_multi_instance_manifest(&path).ok() else {
        return failed(
            &format!("运行实例不存在：{instance_id}"),
            CodexMultiInstancesPayload {
                instances: Vec::new(),
            },
        );
    };
    // 先确认这个实例真的在跑，再按它自己的端口结束它。
    //
    // 早先是「无条件全局结束 → 再看端口是否还在」，在 Windows 上会一次杀掉
    // 主 Codex 与其它多开实例（`stop_codex_processes_for_debug_port_and_wait`
    // 在非 macOS 上退化为全局结束）。现在只在这个实例的调试端口确实有服务时才动手。
    if !chatgptx_core::cdp::endpoint_available(instance.debug_port) {
        if let Some(pid) = instance.pid {
            let options_path = path.join("worker-options.json");
            let _ = terminate_multi_worker(pid, &options_path);
        }
        return ok(
            "该实例当前未在运行，未结束任何 Codex 进程。",
            CodexMultiInstancesPayload {
                instances: Vec::new(),
            },
        );
    }
    // Windows 的判据是「命令行里带这个调试端口」，进程扫描读不到命令行时宁可漏判；
    // 这里再确认一次该端口确实还有本实例的进程，避免误碰别的 Codex。
    #[cfg(windows)]
    if chatgptx_core::watcher::codex_instance_processes_for_debug_port(instance.debug_port)
        .is_empty()
    {
        return ok(
            "未发现属于该实例的 Codex 进程，未结束任何 Codex。",
            CodexMultiInstancesPayload {
                instances: Vec::new(),
            },
        );
    }
    chatgptx_core::watcher::stop_codex_processes_for_debug_port_and_wait(instance.debug_port);
    if chatgptx_core::cdp::endpoint_available(instance.debug_port) {
        return failed(
            "该实例的调试端口仍可访问，Codex 进程尚未退出，未结束多开实例。",
            CodexMultiInstancesPayload {
                instances: Vec::new(),
            },
        );
    }
    // 实例已结束，清掉上一轮的启动结果，避免下次启动前显示旧的「启动失败」。
    let _ = std::fs::remove_file(path.join(MULTI_LAUNCH_RESULT_FILE));
    if let Some(pid) = instance.pid {
        let options_path = path.join("worker-options.json");
        let _ = terminate_multi_worker(pid, &options_path);
    }
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
        "manager.multi_instance_stopped",
        json!({ "instance_id": instance.instance_id, "debug_port": instance.debug_port }),
    );
    list_codex_multi_instances_blocking()
}

#[tauri::command]
pub async fn restart_codex_multi_instance(
    request: CodexMultiInstanceRequest,
) -> CommandResult<Value> {
    match tauri::async_runtime::spawn_blocking(move || {
        restart_codex_multi_instance_blocking(&request.instance_id)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => failed(&format!("重启 Codex 多开实例失败：{error}"), json!({})),
    }
}

fn restart_codex_multi_instance_blocking(instance_id: &str) -> CommandResult<Value> {
    if instance_id.trim() == "main" {
        return failed(
            "主进程请使用顶部的重启 Codex。",
            json!({ "instanceId": "main" }),
        );
    }
    let path = multi_instance_root(&multi_instance_slug(instance_id));
    let Some(instance) = load_multi_instance_manifest(&path).ok() else {
        return failed(
            &format!("运行实例不存在：{instance_id}"),
            json!({ "instanceId": instance_id }),
        );
    };
    let stop = stop_codex_multi_instance_blocking(instance_id);
    if stop.status == "failed" {
        return failed(&stop.message, json!({ "instanceId": instance_id }));
    }
    launch_codex_multi_instance_blocking(CodexMultiLaunchRequest {
        settings: None,
        profile_id: instance.profile_id,
        instance_id: instance.instance_id,
        app_path: instance.app_path.unwrap_or_default(),
        debug_port: instance.debug_port,
    })
}

fn multi_instance_root(slug: &str) -> PathBuf {
    chatgptx_core::paths::default_app_state_dir()
        .join("codex-multi")
        .join(slug)
}

fn multi_instance_manifest_path(root: &Path) -> PathBuf {
    root.join("instance.json")
}

fn save_multi_instance_manifest(root: &Path, instance: &CodexMultiInstance) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(instance)?;
    chatgptx_core::settings::atomic_write(&multi_instance_manifest_path(root), &bytes)
}

fn load_multi_instance_manifest(root: &Path) -> anyhow::Result<CodexMultiInstance> {
    Ok(serde_json::from_slice(&std::fs::read(
        multi_instance_manifest_path(root),
    )?)?)
}

fn multi_instance_is_running(instance: &CodexMultiInstance) -> bool {
    // 判定只看调试端口是否真的在服务。
    //
    // 早先是「端口可用 OR 能找到 Codex 进程」，但后者在 Windows 上会退化成
    // 「机器上有没有任意一个 Codex 在跑」——只要主实例开着，每个多开清单都会
    // 被判成运行中；反过来「结束实例」又会杀掉全部 Codex。端口是实例自己的，
    // 是这里唯一可靠的判据。
    chatgptx_core::cdp::endpoint_available(instance.debug_port)
}

/// 列表里显示的状态：能跑就是运行中；跑不起来且 worker 明确报了失败才是失败，
/// 否则只是没在运行（例如用户自己关掉了窗口）。
fn multi_instance_status(running: bool, result: Option<&CodexMultiLaunchResult>) -> &'static str {
    if running {
        return "running";
    }
    match result.map(|result| result.status.as_str()) {
        Some("failed") => "failed",
        Some("ready_degraded") => "running_degraded",
        _ => "stopped",
    }
}

fn terminate_multi_worker(pid: u32, options_path: &Path) -> anyhow::Result<()> {
    if pid == std::process::id() || !multi_worker_process_matches(pid, options_path) {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let status = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()?;
        anyhow::ensure!(status.success(), "结束多开 worker 失败（pid {pid}）");
    }
    #[cfg(windows)]
    {
        let pid_arg = pid.to_string();
        // 不带 `/T`：这里只该结束 worker 自己。
        // `/T` 会连带杀掉整棵子进程树，而 worker 下面挂着 Codex Desktop；
        // 一旦 manifest 里的 pid 已被系统回收，就会误杀无关进程树
        // （Windows 分支没有 unix 那样的命令行校验，见 multi_worker_process_matches）。
        let status = std::process::Command::new("taskkill")
            .args(["/PID", pid_arg.as_str(), "/F"])
            .status()?;
        anyhow::ensure!(status.success(), "结束多开 worker 失败（pid {pid}）");
    }
    Ok(())
}

fn multi_worker_process_matches(pid: u32, options_path: &Path) -> bool {
    #[cfg(unix)]
    {
        let Ok(output) = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
        else {
            return false;
        };
        let command = String::from_utf8_lossy(&output.stdout);
        command.contains("--chatgptx-multi-worker")
            && command.contains(&options_path.to_string_lossy().to_string())
    }
    #[cfg(windows)]
    {
        let _ = options_path;
        true
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, options_path);
        false
    }
}

/// Hidden child process used by Codex multi-launch. Its environment is isolated
/// before this function is called, so the normal launcher can safely use its
/// existing default-home/settings/helper/bridge code without touching the main
/// ChatGPTX instance.
///
/// 就绪校验（issue：多开在 Windows 上静默失败）：
/// launcher 的 `start` 只保证「激活流程走完」，不代表 Codex 真的起来了。
/// 在 Windows 上按路径启动 MSIX 包内 exe 会丢包身份，进程秒退、CDP 端口永不监听，
/// 而父进程看不到任何失败信号。这里显式确认调试端口可用，并把结论落盘给父进程，
/// 让界面显示「启动失败」而不是「运行中」。
pub fn run_codex_multi_worker(options_path: &Path) -> anyhow::Result<()> {
    let options: CodexMultiWorkerOptions = serde_json::from_slice(&std::fs::read(options_path)?)?;

    // 状态目录以 worker 自己的 options 为准，并且**不用** `set_var` 改进程环境：
    // 多开 worker 复用管理器进程，启动时已是多线程，edition 2024 下 `set_var` 属未定义
    // 行为。早先只靠父进程设置 HOME 来隔离也不成立——Windows 上 `directories` 走
    // SHGetKnownFolderPath、不读 HOME，worker 会读写主实例的状态文件。
    let instance_state_dir = PathBuf::from(options.state_dir.trim());
    // 真正生效的等待端口：父进程注入的 `CHATGPTX_PROTOCOL_PROXY_PORT` 优先于 options。
    let effective_helper_port = std::env::var("CHATGPTX_PROTOCOL_PROXY_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .unwrap_or(options.helper_port);

    let result_path = options_path
        .parent()
        .map(|root| root.join(MULTI_LAUNCH_RESULT_FILE))
        .unwrap_or_else(|| PathBuf::from(MULTI_LAUNCH_RESULT_FILE));
    // 状态文件也显式指向实例目录：`StatusStore::default()` 会解析主实例路径，
    // worker 只应写自己那份 latest-status.json。
    let instance_status_store = StatusStore::new(instance_state_dir.join("latest-status.json"));
    let launch_options = chatgptx_launcher::LaunchOptions {
        app_dir: Some(PathBuf::from(&options.app_path)),
        debug_port: options.debug_port,
        helper_port: options.helper_port,
        status_store: instance_status_store.clone(),
        state_dir: Some(instance_state_dir.clone()),
        codex_home: std::env::var_os("CODEX_HOME").map(PathBuf::from),
        sqlite_home: std::env::var_os("CODEX_SQLITE_HOME").map(PathBuf::from),
    };
    if let Err(error) = chatgptx_launcher::start(launch_options) {
        let message = format!("{error:#}");
        let _ = write_multi_launch_result(
            &result_path,
            &CodexMultiLaunchResult {
                status: "failed".to_string(),
                message: format!("启动 Codex 失败：{message}"),
                finished_at_ms: current_timestamp_ms(),
                debug_port: Some(options.debug_port),
                pid: None,
            },
        );
        // 父进程按启动结果文件与端口探测判断成败，这里不再向上返回错误：
        // 让 worker 正常退出，避免主进程日志里出现无主的 panic 噪音。
        return Ok(());
    }

    let ready = wait_for_multi_instance_ready(options.debug_port, MULTI_LAUNCH_READY_TIMEOUT);
    let latest = instance_status_store.load_latest().ok().flatten();
    // 「端口通了」还要看注入结果：launcher 写 `running_degraded` 表示增强脚本没注入成功，
    // 这时算「实例可用但增强未就绪」，不能谎报成完全就绪。
    let injection_degraded = latest
        .as_ref()
        .is_some_and(|status| status.status == "running_degraded");
    let message = multi_launch_result_message(ready, options.debug_port, latest.as_ref());
    let status = if !ready {
        "failed"
    } else if injection_degraded {
        "ready_degraded"
    } else {
        "ready"
    };
    let _ = write_multi_launch_result(
        &result_path,
        &CodexMultiLaunchResult {
            status: status.to_string(),
            message,
            finished_at_ms: current_timestamp_ms(),
            debug_port: Some(options.debug_port),
            pid: multi_instance_codex_pid(options.debug_port),
        },
    );
    if !ready {
        let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
            "manager.multi_instance_not_ready",
            json!({
                "instanceId": std::env::var("CHATGPTX_INSTANCE_ID").unwrap_or_default(),
                "debug_port": options.debug_port,
                "helper_port": effective_helper_port,
                "app_path": options.app_path,
                "codex_home": std::env::var("CODEX_HOME").unwrap_or_default(),
                "waited_ms": MULTI_LAUNCH_READY_TIMEOUT.as_millis() as u64,
                "status": latest.as_ref().map(|status| status.status.clone()),
                "phase": latest.as_ref().and_then(|status| status.phase.clone()),
                "message": latest.as_ref().map(|status| status.message.clone()),
            }),
        );
        // 起不来时不要留下 helper/协议代理占着端口。
        // 只关掉服务，不 exit 整个进程：worker 就是管理器自己的进程（多开用
        // `--chatgptx-multi-worker` 复用同一个 exe 并隐藏窗口），exit 会把管理界面一起带走。
        let _ = chatgptx_launcher::shutdown();
        return Ok(());
    }

    while chatgptx_core::cdp::endpoint_available(options.debug_port) {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    chatgptx_launcher::shutdown()
}

/// 多开实例启动结果文件名（与 `worker-options.json` 同目录）。
pub const MULTI_LAUNCH_RESULT_FILE: &str = "launch-result.json";

/// 等待多开实例的 CDP 端口就绪；窗口已出现但注入失败同样算未就绪。
pub fn wait_for_multi_instance_ready(debug_port: u16, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if chatgptx_core::cdp::endpoint_available(debug_port) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

pub const MULTI_LAUNCH_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

fn multi_instance_codex_pid(debug_port: u16) -> Option<u32> {
    chatgptx_core::watcher::find_codex_processes_for_debug_port(debug_port)
        .first()
        .copied()
}

/// 就绪失败时的用户可读文案：区分「端口没起来」与「launcher 已记下失败原因」。
fn multi_launch_result_message(
    ready: bool,
    debug_port: u16,
    latest: Option<&chatgptx_core::status::LaunchStatus>,
) -> String {
    if ready {
        return "Codex 多开实例已就绪。".to_string();
    }
    let detail = latest
        .map(|status| status.message.trim())
        .filter(|message| !message.is_empty());
    match detail {
        Some(detail) => format!(
            "Codex 多开实例启动后调试端口 {debug_port} 未就绪（{detail}）。\
             若 Codex 是 Microsoft Store 安装版，请改用包上下文启动或改用非打包版本。"
        ),
        None => format!("Codex 多开实例启动后调试端口 {debug_port} 未就绪，Codex 可能已退出。"),
    }
}

fn write_multi_launch_result(path: &Path, result: &CodexMultiLaunchResult) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(result)?;
    chatgptx_core::settings::atomic_write(path, &bytes)
}

/// 读取实例的启动结果；文件不存在或不是合法 JSON 时返回 `None`。
fn load_multi_launch_result(root: &Path) -> Option<CodexMultiLaunchResult> {
    let contents = std::fs::read_to_string(root.join(MULTI_LAUNCH_RESULT_FILE)).ok()?;
    serde_json::from_str(&contents).ok()
}

fn multi_instance_slug(value: &str) -> String {
    let mut slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "instance".to_string()
    } else {
        slug.chars().take(64).collect()
    }
}

fn multi_instance_user_data_dir(slug: &str) -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| {
            dirs.data_local_dir()
                .join("ChatGPTX")
                .join("multi")
                .join(slug)
        })
        .unwrap_or_else(|| {
            chatgptx_core::paths::default_app_state_dir()
                .join("user-data")
                .join(slug)
        })
}

fn multi_instance_extra_args(args: &[String]) -> Vec<String> {
    chatgptx_core::settings::normalize_codex_extra_args(args)
        .into_iter()
        .filter(|arg| {
            !arg.starts_with("--remote-debugging-port")
                && !arg.starts_with("--remote-allow-origins")
                && !arg.starts_with("--user-data-dir")
        })
        .collect()
}

fn absolutize_multi_instance_model_catalog(home: &Path) -> anyhow::Result<()> {
    let config_path = home.join("config.toml");
    let contents = std::fs::read_to_string(&config_path)?;
    let mut doc = contents.parse::<toml_edit::DocumentMut>()?;
    let Some(raw_path) = doc
        .get("model_catalog_json")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        return Ok(());
    }
    let absolute = home.join(path);
    doc["model_catalog_json"] = toml_edit::value(absolute.to_string_lossy().to_string());
    chatgptx_core::settings::atomic_write(&config_path, doc.to_string().as_bytes())?;
    Ok(())
}

fn seed_multi_instance_onboarding_state(home: &Path) -> anyhow::Result<()> {
    let path = home.join(".codex-global-state.json");
    let mut state = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| json!({})),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error.into()),
    };
    let root = state
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(".codex-global-state.json 不是 JSON 对象"))?;
    let atoms = root
        .entry("electron-persisted-atom-state")
        .or_insert_with(|| json!({}));
    let atoms = atoms
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("electron-persisted-atom-state 不是 JSON 对象"))?;
    atoms.insert(
        "electron:onboarding-projectless-completed".to_string(),
        Value::Bool(true),
    );
    atoms
        .entry("last_completed_onboarding".to_string())
        .or_insert_with(|| Value::from(current_timestamp_ms() / 1000));
    let bytes = serde_json::to_vec_pretty(&state)?;
    chatgptx_core::settings::atomic_write(&path, &bytes)?;
    Ok(())
}

fn remove_invalid_multi_instance_mcp_servers(home: &Path) -> anyhow::Result<()> {
    let config_path = home.join("config.toml");
    let contents = std::fs::read_to_string(&config_path)?;
    let mut doc = contents.parse::<toml_edit::DocumentMut>()?;
    let Some(servers) = doc
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_like_mut)
    else {
        return Ok(());
    };
    let invalid = servers
        .iter()
        .filter_map(|(id, item)| {
            let table = item.as_table_like()?;
            let valid = ["command", "url"].iter().any(|key| {
                table
                    .get(*key)
                    .and_then(toml_edit::Item::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            });
            (!valid).then_some(id.to_string())
        })
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        return Ok(());
    }
    for id in invalid {
        servers.remove(&id);
    }
    if servers.is_empty() {
        doc.as_table_mut().remove("mcp_servers");
    }
    chatgptx_core::settings::atomic_write(&config_path, doc.to_string().as_bytes())?;
    Ok(())
}

fn select_multi_instance_port(requested: u16, instance_id: &str) -> u16 {
    let base = if requested > 0 {
        requested
    } else {
        let sum = instance_id
            .bytes()
            .fold(0u16, |sum, byte| sum.wrapping_add(byte as u16));
        9300u16.saturating_add(sum % 500)
    };
    (0..1000)
        .map(|offset| base.saturating_add(offset))
        .find(|port| *port > 0 && std::net::TcpListener::bind(("127.0.0.1", *port)).is_ok())
        .unwrap_or(base)
}

#[tauri::command]
pub async fn restart_chatgptx(request: LaunchRequest) -> CommandResult<Value> {
    match tauri::async_runtime::spawn_blocking(move || restart_chatgptx_blocking(request)).await {
        Ok(result) => result,
        Err(error) => failed(&format!("重启后台任务失败：{error}"), json!({})),
    }
}

static LAUNCH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct LaunchGuard;

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        LAUNCH_IN_PROGRESS.store(false, Ordering::Release);
    }
}

fn try_acquire_launch_guard() -> Result<LaunchGuard, &'static str> {
    LAUNCH_IN_PROGRESS
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .map(|_| LaunchGuard)
        .map_err(|_| "启动或重启正在进行，请等待本次操作完成。")
}

/// 供自动接管守护线程复用同一把串行锁：它与用户点「启动/重启」必须互斥，
/// 否则会出现两套 launcher 会话抢同一个官方 App。
pub fn try_acquire_launch_guard_for_takeover() -> Result<impl Drop, &'static str> {
    try_acquire_launch_guard()
}

/// 是否有启动/重启在途（判据用，只读）。
pub fn launch_in_progress() -> bool {
    LAUNCH_IN_PROGRESS.load(Ordering::Acquire)
}

/// 自动接管发起的启动：走与「点启动」完全相同的 launcher 链路。
///
/// 与 `start_embedded_launcher` 的唯一区别是入参由守护线程解析，不接受前端传来的
/// 临时状态。
pub fn run_auto_takeover_launch(request: LaunchRequest) -> Result<(), String> {
    if crate::app_is_exiting() {
        return Err("ChatGPTX 正在退出，未启动 Codex".to_string());
    }
    let launch_options = chatgptx_launcher::LaunchOptions {
        app_dir: (!request.app_path.trim().is_empty())
            .then(|| PathBuf::from(request.app_path.trim())),
        debug_port: request.debug_port,
        helper_port: request.helper_port,
        status_store: StatusStore::default(),
        ..Default::default()
    };
    chatgptx_launcher::start(launch_options).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_auto_takeover_state() -> CommandResult<crate::auto_takeover::AutoTakeoverPayload> {
    ok("自动接管状态已加载。", crate::auto_takeover::payload())
}

#[tauri::command]
pub fn take_over_codex_now() -> CommandResult<crate::auto_takeover::AutoTakeoverPayload> {
    match crate::auto_takeover::take_over_now() {
        Ok(()) => ok("已发起接管。", crate::auto_takeover::payload()),
        Err(message) => failed(&message, crate::auto_takeover::payload()),
    }
}

#[tauri::command]
pub fn pause_auto_takeover_for_instance() -> CommandResult<crate::auto_takeover::AutoTakeoverPayload>
{
    match crate::auto_takeover::pause_until_next_instance() {
        Ok(()) => ok(
            "本次不再自动接管，重启官方 App 后恢复。",
            crate::auto_takeover::payload(),
        ),
        Err(message) => failed(&message, crate::auto_takeover::payload()),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RestartDisposition {
    LaunchOnly,
    StopAndRestart,
}

fn restart_disposition(sync_active_relay: bool, app_running: bool) -> RestartDisposition {
    if !sync_active_relay && !app_running {
        RestartDisposition::LaunchOnly
    } else {
        RestartDisposition::StopAndRestart
    }
}

fn restart_chatgptx_blocking(request: LaunchRequest) -> CommandResult<Value> {
    let _launch_guard = match try_acquire_launch_guard() {
        Ok(guard) => guard,
        Err(message) => return failed(message, json!({})),
    };
    let request = normalize_launch_request(request);
    // 仅按主实例端口及非多开主进程判定主 Codex 是否在运行，排除所有多开实例。
    let app_running = chatgptx_core::cdp::endpoint_available(request.debug_port)
        || !chatgptx_core::watcher::find_main_codex_processes(request.debug_port).is_empty();
    if restart_disposition(request.sync_active_relay, app_running) == RestartDisposition::LaunchOnly
    {
        return spawn_chatgptx_launch(request, "Codex 未运行，已按直接启动方式在后台唤起。");
    }
    let Ok(_guard) = relay_switch_mutex().try_lock() else {
        return failed(
            "供应商切换正在进行或锁不可用，未执行重启；请稍后重试。",
            json!({}),
        );
    };
    let settings = if request.sync_active_relay {
        match SettingsStore::default().load() {
            Ok(settings) => Some(settings),
            Err(error) => {
                return failed(
                    &format!("读取已保存供应商设置失败，未执行重启：{error}"),
                    json!({
                        "debugPort": request.debug_port,
                        "helperPort": request.helper_port
                    }),
                );
            }
        }
    } else {
        None
    };
    let provider_guard = match ensure_provider_sync_is_idle_before_stop() {
        Ok(guard) => guard,
        Err(message) => return failed(&message, json!({})),
    };
    // 停止 Codex 后清理同进程后台服务，再确认浏览器恢复结果。
    // 先记录停止阶段，让界面在等待期间显示进度（issue #2403）。
    // 该状态带的是当前时间戳，早于后面返回给前端的 launch_started_at_ms，
    // 因此等待完成度判定时会被视作 stale，不会误报成最终结果。
    let _ = StatusStore::default().save_latest(&requested_launch_status_with_phase(
        &request,
        "stopping",
        "正在停止 Codex 并清理后台服务…",
        current_timestamp_ms(),
        "restart_stopping",
        10,
    ));
    let native_browser_shutdown = match stop_chatgptx_for_restart(
        || {
            // 仅停止主 Codex 实例（带主调试端口或裸启动的主实例），绝不触碰多开实例。
            chatgptx_core::watcher::stop_main_codex_processes_and_wait(request.debug_port);
            anyhow::ensure!(
                !chatgptx_core::cdp::endpoint_available(request.debug_port)
                    && chatgptx_core::watcher::find_main_codex_processes(request.debug_port)
                        .is_empty(),
                "Codex 进程尚未退出；未启动新实例"
            );
            Ok(())
        },
        chatgptx_launcher::stop,
        || {
            chatgptx_core::native_browser::wait_for_monitor_shutdown(
                std::time::Duration::from_secs(10),
            )
        },
    ) {
        Ok(shutdown) => shutdown,
        Err(error) => {
            return failed(
                &restart_stop_failure_message(&error),
                json!({"debugPort": request.debug_port, "helperPort": request.helper_port}),
            );
        }
    };
    let native_browser_restore_failed = matches!(
        native_browser_shutdown,
        chatgptx_core::native_browser::NativeBrowserShutdown::RestoreFailed
    );
    if native_browser_restore_failed {
        let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
            "native_browser.cleanup_not_restored",
            json!({"state": "blocked"}),
        );
    }
    if let Err(error) = prepare_fixed_helper_port_for_restart(settings.as_ref()) {
        return failed(
            &format!("重启 Codex 失败：{error:#}"),
            json!({"debugPort": request.debug_port, "helperPort": request.helper_port}),
        );
    }
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
        "manager.restart_requested",
        json!({
            "debug_port": request.debug_port,
            "helper_port": request.helper_port,
            "app_path": request.app_path.trim(),
            "sync_active_relay": request.sync_active_relay
        }),
    );
    let launch_started_at_ms = current_timestamp_ms();
    if let Err(error) = save_requested_launch_status(
        &request,
        "starting",
        "ChatGPTX launcher is starting",
        launch_started_at_ms,
    ) {
        return failed(
            &format!("记录重启状态失败，未执行重启：{error}"),
            json!({
                "debugPort": request.debug_port,
                "helperPort": request.helper_port,
                "syncActiveRelay": request.sync_active_relay
            }),
        );
    }
    let mut restart_message = "Codex 已请求重启，启动任务正在后台运行。".to_string();
    match restart_chatgptx_after_stop(&request, &home, settings.as_ref(), |request| {
        spawn_after_provider_guard_release(provider_guard, request, start_embedded_launcher)
    }) {
        Ok(warnings) => {
            for warning in warnings {
                restart_message.push_str("\n\n");
                restart_message.push_str(&warning);
            }
            let (debug_port, helper_port, started_at_ms) =
                accepted_launch_identity(&request, launch_started_at_ms);
            CommandResult {
                status: "accepted".to_string(),
                message: restart_message,
                payload: json!({
                    "debugPort": debug_port,
                    "helperPort": helper_port,
                    "syncActiveRelay": request.sync_active_relay,
                    "launchStartedAtMs": started_at_ms,
                    "nativeBrowserRestoreFailed": native_browser_restore_failed,
                    "forcedStoppedLauncherCount": 0
                }),
            }
        }
        Err(error) => {
            let message = format!("重启 Codex 失败：{error}");
            let _ =
                save_requested_launch_status(&request, "failed", &message, launch_started_at_ms);
            failed(
                &message,
                json!({
                    "debugPort": request.debug_port,
                    "helperPort": request.helper_port,
                    "syncActiveRelay": request.sync_active_relay,
                    "launchStartedAtMs": launch_started_at_ms,
                    "nativeBrowserRestoreFailed": native_browser_restore_failed
                }),
            )
        }
    }
}

fn stop_main_codex_blocking(debug_port: u16) -> Result<(), String> {
    let port = if debug_port == 0 { 9229 } else { debug_port };
    let _ = stop_chatgptx_for_restart(
        || {
            chatgptx_core::watcher::stop_main_codex_processes_and_wait(port);
            anyhow::ensure!(
                !chatgptx_core::cdp::endpoint_available(port)
                    && chatgptx_core::watcher::find_main_codex_processes(port).is_empty(),
                "Codex 进程尚未退出"
            );
            Ok(())
        },
        chatgptx_launcher::stop,
        || {
            chatgptx_core::native_browser::wait_for_monitor_shutdown(
                std::time::Duration::from_secs(10),
            )
        },
    )
    .map_err(|error| restart_stop_failure_message(&error))?;

    let stop_req = LaunchRequest {
        app_path: String::new(),
        debug_port: port,
        helper_port: 57321,
        sync_active_relay: false,
    };
    let _ = save_requested_launch_status(
        &stop_req,
        "stopped",
        "Codex 已停止。",
        current_timestamp_ms(),
    );
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
        "manager.stop_requested",
        json!({ "debug_port": port }),
    );
    Ok(())
}

#[tauri::command]
pub async fn stop_chatgptx(request: Option<LaunchRequest>) -> CommandResult<Value> {
    let debug_port = request
        .as_ref()
        .map(|r| r.debug_port)
        .filter(|p| *p > 0)
        .or_else(|| {
            StatusStore::default()
                .load_latest()
                .ok()
                .flatten()
                .and_then(|s| s.debug_port)
                .filter(|p| *p > 0)
        })
        .unwrap_or(9229);
    match tauri::async_runtime::spawn_blocking(move || stop_main_codex_blocking(debug_port)).await {
        Ok(Ok(())) => ok("Codex 已停止。", json!({ "debugPort": debug_port })),
        Ok(Err(err)) => failed(&err, json!({ "debugPort": debug_port })),
        Err(err) => failed(&format!("停止 Codex 失败：{err}"), json!({ "debugPort": debug_port })),
    }
}

fn stop_chatgptx_for_restart(
    stop_codex: impl FnOnce() -> anyhow::Result<()>,
    stop_runtime: impl FnOnce() -> anyhow::Result<()>,
    wait_native: impl FnOnce() -> anyhow::Result<chatgptx_core::native_browser::NativeBrowserShutdown>,
) -> Result<chatgptx_core::native_browser::NativeBrowserShutdown, RestartStopError> {
    // 后台服务持有浏览器恢复任务。停止服务须等待它清理，不能结束主进程。
    stop_codex().map_err(RestartStopError::Codex)?;
    stop_runtime().map_err(RestartStopError::Runtime)?;
    wait_native().map_err(RestartStopError::NativeBrowser)
}

#[derive(Debug)]
enum RestartStopError {
    Codex(anyhow::Error),
    NativeBrowser(anyhow::Error),
    Runtime(anyhow::Error),
}

fn restart_stop_failure_message(error: &RestartStopError) -> String {
    match error {
        RestartStopError::Codex(error) => {
            format!("Codex 进程尚未确认退出，未清理原生浏览器或启动新实例：{error}")
        }
        RestartStopError::NativeBrowser(error) => {
            let summary = if error
                .downcast_ref::<chatgptx_core::native_browser::NativeBrowserCleanupStillRunning>()
                .is_some()
            {
                "仍在恢复"
            } else {
                "恢复失败"
            };
            format!("Codex 已请求停止，但原生浏览器文件{summary}，未启动新实例：{error}")
        }
        RestartStopError::Runtime(error) => {
            format!("Codex 已请求停止，但后台服务尚未清理完成，未启动新实例：{error}")
        }
    }
}

fn prepare_fixed_helper_port_for_restart(settings: Option<&BackendSettings>) -> anyhow::Result<()> {
    let loaded;
    let settings = match settings {
        Some(settings) => settings,
        None => match SettingsStore::default().load() {
            Ok(value) => {
                loaded = value;
                &loaded
            }
            // 读不到设置时仍交给 launcher 自己等。这里失败不应把一次普通重启拦死。
            Err(_) => return Ok(()),
        },
    };
    let Some(port) = chatgptx_core::launcher::required_fixed_helper_port(settings) else {
        return Ok(());
    };
    chatgptx_core::launcher::wait_for_fixed_helper_port(
        port,
        chatgptx_core::launcher::protocol_proxy_bind_retry_timeout_ms(),
        chatgptx_core::launcher::helper_bind_retry_interval_ms(),
        probe_loopback_port,
        |interval_ms| std::thread::sleep(std::time::Duration::from_millis(interval_ms)),
    )
}

fn probe_loopback_port(port: u16) -> std::io::Result<()> {
    let _listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    Ok(())
}

fn spawn_after_provider_guard_release(
    guard: chatgptx_data::ProviderSyncLifecycleGuard,
    request: &LaunchRequest,
    spawn: impl FnOnce(&LaunchRequest) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    guard
        .release()
        .map_err(|error| anyhow::anyhow!("释放历史同步锁失败，未启动新实例：{error}"))?;
    spawn(request)
}

fn restart_chatgptx_after_stop<F>(
    request: &LaunchRequest,
    home: &Path,
    settings: Option<&BackendSettings>,
    spawn: F,
) -> anyhow::Result<Vec<String>>
where
    F: FnOnce(&LaunchRequest) -> anyhow::Result<()>,
{
    let mut warnings = Vec::new();
    let snapshot = if let Some(settings) = settings {
        let snapshot = RelayLiveSnapshot::capture(home)?;
        if let Err(error) = sync_active_relay_to_home(settings, home) {
            if let Err(restore_error) = snapshot.restore(home) {
                anyhow::bail!("同步当前供应商失败：{error}；回滚 live 配置也失败：{restore_error}");
            }
            return Err(error);
        }
        if let Some(warning) =
            chatgptx_core::aggregate_catalog::subagent_conflict_warning(home, settings)
        {
            warnings.push(warning);
        }
        Some(snapshot)
    } else {
        None
    };

    if let Err(error) = spawn(request) {
        if let Some(snapshot) = snapshot {
            if let Err(restore_error) = snapshot.restore(home) {
                anyhow::bail!("启动 Codex 失败：{error}；回滚 live 配置也失败：{restore_error}");
            }
        }
        return Err(error);
    }
    Ok(warnings)
}

#[derive(Debug)]
struct RelayLiveSnapshot {
    config: Option<Vec<u8>>,
    auth: Option<Vec<u8>>,
}

impl RelayLiveSnapshot {
    fn capture(home: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            config: read_optional_file_bytes(&home.join("config.toml"))?,
            auth: read_optional_file_bytes(&home.join("auth.json"))?,
        })
    }

    fn restore(&self, home: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(home)?;
        restore_optional_file_bytes(&home.join("config.toml"), self.config.as_deref())?;
        restore_optional_file_bytes(&home.join("auth.json"), self.auth.as_deref())?;
        Ok(())
    }
}

fn read_optional_file_bytes(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn restore_optional_file_bytes(path: &Path, contents: Option<&[u8]>) -> anyhow::Result<()> {
    match contents {
        Some(contents) => chatgptx_core::settings::atomic_write(path, contents)?,
        None => match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        },
    }
    Ok(())
}

fn sync_active_relay_to_home(
    settings: &BackendSettings,
    home: &Path,
) -> anyhow::Result<chatgptx_core::relay_config::RelayApplyResult> {
    if !settings.relay_profiles_enabled {
        anyhow::bail!("供应商配置总开关已关闭，未同步 live 配置");
    }
    let relay = settings.active_relay_profile();
    if relay.relay_mode == chatgptx_core::settings::RelayMode::Aggregate {
        if settings.active_aggregate_relay_profile().is_none() {
            anyhow::bail!("当前聚合供应商配置不完整");
        }
        let aggregate = settings
            .active_aggregate_relay_profile()
            .ok_or_else(|| anyhow::anyhow!("当前聚合供应商配置不完整"))?;
        if aggregate.model_aggregation_enabled {
            return chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules(
                home,
                &relay,
                &relay_combined_common_config(settings),
            );
        }
        return chatgptx_core::relay_config::apply_relay_config_to_home_with_session_provider(
            home,
            &chatgptx_core::protocol_proxy::local_responses_proxy_base_url(
                chatgptx_core::protocol_proxy::protocol_proxy_port(),
            ),
            "chatgptx-aggregate",
            chatgptx_core::settings::RelayProtocol::Responses,
            chatgptx_core::protocol_proxy::protocol_proxy_port(),
            aggregate.session_provider,
        );
    }
    if relay.relay_mode == chatgptx_core::settings::RelayMode::Official
        && !relay.official_mix_api_key
    {
        let auth_contents =
            (!relay.auth_contents.trim().is_empty()).then_some(relay.auth_contents.as_str());
        return chatgptx_core::relay_config::clear_relay_config_to_home_with_auth(
            home,
            auth_contents,
        );
    }
    if relay_has_complete_files(&relay) {
        return chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules(
            home,
            &relay,
            &relay_combined_common_config(settings),
        );
    }

    let mut base_url = relay.base_url.trim().to_string();
    let mut protocol = relay.protocol;
    if relay.has_model_routes() {
        base_url = chatgptx_core::protocol_proxy::local_responses_proxy_base_url(
            chatgptx_core::protocol_proxy::protocol_proxy_port(),
        );
        protocol = chatgptx_core::settings::RelayProtocol::Responses;
    }
    if relay.relay_mode == chatgptx_core::settings::RelayMode::PureApi {
        return chatgptx_core::relay_config::apply_pure_api_config_to_home_with_session_provider(
            home,
            &base_url,
            &relay.api_key,
            protocol,
            chatgptx_core::protocol_proxy::protocol_proxy_port(),
            chatgptx_core::relay_config::relay_session_provider_from_config(
                &relay.config_contents,
            ),
        );
    }

    let auth = chatgptx_core::relay_config::chatgpt_auth_status_from_home(home);
    if !auth.authenticated {
        anyhow::bail!("未检测到 ChatGPT 登录状态，已停止同步 live 配置");
    }
    chatgptx_core::relay_config::apply_relay_config_to_home_with_session_provider(
        home,
        &base_url,
        &relay.api_key,
        protocol,
        chatgptx_core::protocol_proxy::protocol_proxy_port(),
        chatgptx_core::relay_config::relay_session_provider_from_config(&relay.config_contents),
    )
}

fn normalize_launch_request(mut request: LaunchRequest) -> LaunchRequest {
    // launcher 收到显式 --app-path 时不会回退自动探测（避免静默启动错误目录），
    // 所以这里先把明显无效的路径摘掉，让它走探测而不是永久失败（#1972）。
    let requested_app_path = request.app_path.trim().to_string();
    if !requested_app_path.is_empty()
        && chatgptx_core::app_paths::normalize_codex_app_path(Path::new(&requested_app_path))
            .is_none()
    {
        let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
            "manager.launch_app_path_rejected",
            json!({ "app_path": requested_app_path }),
        );
        request.app_path = String::new();
    }
    request
}

fn spawn_chatgptx_launch(request: LaunchRequest, accepted_message: &str) -> CommandResult<Value> {
    let request = normalize_launch_request(request);
    let debug_port = request.debug_port;
    let helper_port = request.helper_port;
    let launch_started_at_ms = current_timestamp_ms();
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
        "manager.launch_requested",
        json!({
            "debug_port": debug_port,
            "helper_port": helper_port,
            "app_path": request.app_path.trim()
        }),
    );
    if let Err(error) = save_requested_launch_status(
        &request,
        "starting",
        "ChatGPTX launcher is starting",
        launch_started_at_ms,
    ) {
        return failed(
            &format!("记录启动状态失败，未执行启动：{error}"),
            json!({
                "debugPort": debug_port,
                "helperPort": helper_port
            }),
        );
    }
    match start_embedded_launcher(&request) {
        Ok(()) => {
            let (debug_port, helper_port, started_at_ms) =
                accepted_launch_identity(&request, launch_started_at_ms);
            CommandResult {
                status: "accepted".to_string(),
                message: accepted_message.to_string(),
                payload: json!({
                    "debugPort": debug_port,
                    "helperPort": helper_port,
                    "launchStartedAtMs": started_at_ms
                }),
            }
        }
        Err(error) => {
            let message = format!("启动 Codex 失败：{error}");
            let _ =
                save_requested_launch_status(&request, "failed", &message, launch_started_at_ms);
            failed(
                &message,
                json!({
                    "debugPort": debug_port,
                    "helperPort": helper_port,
                    "launchStartedAtMs": launch_started_at_ms
                }),
            )
        }
    }
}

fn accepted_launch_identity(request: &LaunchRequest, requested_at: u64) -> (u16, u16, u64) {
    let latest = StatusStore::default().load_latest().ok().flatten();
    launch_identity_from_status(request, requested_at, latest.as_ref())
}

fn launch_identity_from_status(
    request: &LaunchRequest,
    requested_at: u64,
    latest: Option<&LaunchStatus>,
) -> (u16, u16, u64) {
    // 重复启动会激活当前 session，其身份不能被新请求覆盖；端口也可能已自动换位。
    if let Some(latest) =
        latest.filter(|status| matches!(status.status.as_str(), "running" | "running_degraded"))
    {
        (
            latest.debug_port.unwrap_or(request.debug_port),
            latest.helper_port.unwrap_or(request.helper_port),
            latest.started_at_ms,
        )
    } else {
        (request.debug_port, request.helper_port, requested_at)
    }
}

fn save_requested_launch_status(
    request: &LaunchRequest,
    status: &str,
    message: &str,
    started_at_ms: u64,
) -> anyhow::Result<()> {
    StatusStore::default().save_latest(&requested_launch_status(
        request,
        status,
        message,
        started_at_ms,
    ))
}

fn requested_launch_status(
    request: &LaunchRequest,
    status: &str,
    message: &str,
    started_at_ms: u64,
) -> LaunchStatus {
    LaunchStatus {
        status: status.to_string(),
        message: message.to_string(),
        started_at_ms,
        debug_port: Some(request.debug_port),
        helper_port: Some(request.helper_port),
        codex_app: (!request.app_path.trim().is_empty())
            .then(|| request.app_path.trim().to_string()),
        aumid: None,
        phase: None,
        progress: None,
    }
}

/// 停止阶段的状态：带 `phase`/`progress`，让界面在阻塞的十几秒里有明确中间态
/// （issue #2403：此前只有 `starting`，看不出是在处理还是卡死）。
fn requested_launch_status_with_phase(
    request: &LaunchRequest,
    status: &str,
    message: &str,
    started_at_ms: u64,
    phase: &str,
    progress: u8,
) -> LaunchStatus {
    LaunchStatus {
        phase: Some(phase.to_string()),
        progress: Some(progress.min(100)),
        ..requested_launch_status(request, status, message, started_at_ms)
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn start_embedded_launcher(request: &LaunchRequest) -> anyhow::Result<()> {
    anyhow::ensure!(!crate::app_is_exiting(), "ChatGPTX 正在退出，未启动 Codex");
    chatgptx_launcher::start(chatgptx_launcher::LaunchOptions {
        app_dir: (!request.app_path.trim().is_empty())
            .then(|| PathBuf::from(request.app_path.trim())),
        debug_port: request.debug_port,
        helper_port: request.helper_port,
        status_store: StatusStore::default(),
        ..Default::default()
    })
}

/// 重新打开管理界面时恢复已有 Codex 的 Helper、代理和注入，不启动官方应用。
pub fn resume_existing_codex_background(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        if crate::app_is_exiting() {
            return;
        }
        let settings = SettingsStore::default().load().unwrap_or_default();
        let latest = StatusStore::default().load_latest().unwrap_or(None);
        let options = chatgptx_launcher::LaunchOptions {
            app_dir: chatgptx_core::app_paths::resolve_codex_app_dir_with_saved(
                latest
                    .as_ref()
                    .and_then(|status| status.codex_app.as_deref())
                    .map(Path::new),
                Some(settings.codex_app_path.as_str()),
            ),
            debug_port: latest
                .as_ref()
                .and_then(|status| status.debug_port)
                .unwrap_or_else(default_debug_port),
            helper_port: latest
                .as_ref()
                .and_then(|status| status.helper_port)
                .unwrap_or_else(default_helper_port),
            status_store: StatusStore::default(),
            ..Default::default()
        };
        if let Err(error) = chatgptx_launcher::resume_if_running(options) {
            let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
                "manager.resume_existing_failed",
                json!({ "message": error.to_string() }),
            );
        }
        let _ = tauri::Emitter::emit(&app, "runtime-health-changed", ());
    });
}


#[tauri::command]
pub fn query_builtin_model_metadata(slug: String) -> CommandResult<Value> {
    match chatgptx_core::model_suffix::builtin_model_metadata(slug.as_str()) {
        Some(metadata) => ok(
            "内置元数据已匹配。",
            json!({
                "matched": true,
                "source": metadata.source,
                "entry": metadata.entry,
            }),
        ),
        None => {
            // 回退值从 bundled 静态资产首条实时取，不写死——
            // sync_official_models.py 调整首条后这里自动跟随。
            let (fallback_slug, fallback_window) =
                chatgptx_core::model_suffix::fallback_template_info()
                    .unwrap_or_else(|| ("gpt-5.5".to_string(), 272_000));
            ok(
                "未命中内置元数据，生成时回退官方模板。",
                json!({
                    "matched": false,
                    "fallback": {
                        "slug": fallback_slug,
                        "context_window": fallback_window,
                    },
                }),
            )
        }
    }
}

#[tauri::command]
pub fn builtin_model_metadata_index() -> CommandResult<Value> {
    let index = chatgptx_core::model_suffix::builtin_model_metadata_index();
    let count = index.len();
    ok(
        "内置元数据索引已读取。",
        json!({ "entries": index, "count": count }),
    )
}

#[tauri::command]
pub fn refresh_builtin_model_metadata() -> CommandResult<Value> {
    chatgptx_core::model_suffix::clear_builtin_metadata_cache();
    let index = chatgptx_core::model_suffix::builtin_model_metadata_index();
    let count = index.len();
    ok(
        "内置与运行时模型元数据缓存已刷新。",
        json!({ "entries": index, "count": count }),
    )
}

#[tauri::command]
pub fn find_desktop_codex_cli() -> CommandResult<Value> {
    // Windows 标准路径：桌面版在用户目录维护、可直接运行的 CLI。
    // Store 包目录（WindowsApps）内的资源受系统保护，第三方进程无法执行（#2028），
    // 因此这里不再返回包内路径，避免把必然失败的路径写进设置。
    #[cfg(windows)]
    {
        return match chatgptx_core::app_paths::find_desktop_managed_codex_cli() {
            Some(path) => ok(
                "已填入桌面版内置 Codex CLI。",
                json!({ "path": path.to_string_lossy() }),
            ),
            None => failed(
                "未找到可运行的桌面版内置 Codex CLI。请先通过 ChatGPTX 启动一次 Codex 桌面版后重试，\
                 或将「Codex CLI 路径」留空自动查找。",
                json!({ "path": null }),
            ),
        };
    }
    #[cfg(not(windows))]
    {
        let settings = match SettingsStore::default().load() {
            Ok(settings) => settings,
            Err(error) => {
                return failed(
                    &format!("读取 Codex 应用设置失败：{error}"),
                    json!({ "path": null }),
                );
            }
        };
        let Some(app_dir) = chatgptx_core::app_paths::resolve_codex_app_dir_with_saved(
            None,
            Some(settings.codex_app_path.as_str()),
        ) else {
            return failed("未找到 Codex Desktop 应用。", json!({ "path": null }));
        };
        // 包内 CLI 可能存在于受系统保护的目录而无法执行（#2028 同类问题），
        // 因此先验证能真正启动，失败时回退到用户目录中的独立 CLI。
        let bundled = chatgptx_core::app_paths::find_bundled_codex_cli(&app_dir);
        let standalone = chatgptx_core::app_paths::find_standalone_codex_cli();
        let Some(path) = [bundled, standalone]
            .into_iter()
            .flatten()
            .find(|candidate| codex_cli_can_start(candidate))
        else {
            return failed(
                "已找到 Codex Desktop，但没有可用的 Codex CLI；请安装或指定用户目录中的 Codex CLI。",
                json!({ "path": null }),
            );
        };
        ok(
            "已填入桌面版内置 Codex CLI。",
            json!({ "path": path.to_string_lossy() }),
        )
    }
}

fn codex_cli_can_start(path: &std::path::Path) -> bool {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(chatgptx_core::windows_create_no_window());
    }
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}


#[derive(serde::Serialize)]
pub struct NativeBrowserDiagnostics {
    compatibility: chatgptx_core::native_browser::BrowserStatus,
    connection: chatgptx_core::native_browser_connection::ConnectionStatus,
}

#[tauri::command]
pub async fn native_browser_status() -> NativeBrowserDiagnostics {
    let compatibility =
        tauri::async_runtime::spawn_blocking(chatgptx_core::native_browser::read_status)
            .await
            .unwrap_or_else(|_| chatgptx_core::native_browser::BrowserStatus {
                state: "unavailable".into(),
                detail: String::new(),
            });
    let connection = chatgptx_core::native_browser_connection::check_connection().await;
    NativeBrowserDiagnostics {
        compatibility,
        connection,
    }
}

#[tauri::command]
pub fn load_settings() -> CommandResult<SettingsPayload> {
    settings_payload("设置已加载。", "设置读取失败")
}

#[tauri::command]
pub fn save_settings(settings: BackendSettings) -> CommandResult<SettingsPayload> {
    let settings = normalize_settings_before_save(settings);
    // 保存诊断：供应商模型列表被改小/改大时能直接看出「提交上来的到底是什么」。
    // 用户反馈过「删掉一行保存后又变回三行」，只有把每次保存的行数记下来才能区分
    // 「界面没提交改动」和「提交了又被别处覆盖」。
    log_manager_event(
        "manager.save_settings",
        json!({
            "activeRelayId": settings.active_relay_id,
            "relayProfiles": settings.relay_profiles.iter().map(|profile| json!({
                "id": profile.id,
                "modelCount": profile.model_list.lines().filter(|line| !line.trim().is_empty()).count(),
                "models": profile.model_list.lines().filter(|line| !line.trim().is_empty()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
    );
    let Ok(_guard) = relay_switch_mutex().lock() else {
        return failed(
            "供应商切换锁已损坏，请重启管理器后再试。",
            SettingsPayload {
                settings,
                settings_path: chatgptx_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                user_scripts: user_script_inventory(),
            },
        );
    };
    let store = SettingsStore::default();
    match store.save(&settings) {
        Ok(()) => {
            // 自动接管是常驻行为，保存后立刻生效，不必重启管理器。
            crate::auto_takeover::apply_enabled(settings.codex_app_auto_takeover);
            settings_payload("设置已保存。", "设置保存后重新读取失败")
        }
        Err(error) => failed(
            &format!("保存设置失败：{error}"),
            SettingsPayload {
                settings,
                settings_path: chatgptx_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                user_scripts: user_script_inventory(),
            },
        ),
    }
}

#[tauri::command]
pub fn load_ccs_providers() -> CommandResult<CcsProvidersPayload> {
    let settings = SettingsStore::default().load().unwrap_or_default();
    match chatgptx_core::ccs_import::resolve_codex_provider_source(&settings.ccs_db_path) {
        Ok(source) => ok(
            &format!(
                "已读取 cc-switch Codex 供应商配置：{} 个。",
                source.providers.len()
            ),
            CcsProvidersPayload {
                db_path: source.db_path.to_string_lossy().to_string(),
                configured_db_path: source.configured_db_path,
                fallback_reason: source.fallback_reason,
                providers: source.providers,
            },
        ),
        Err(error) => failed(
            &format!("读取 cc-switch 供应商配置失败：{error}"),
            CcsProvidersPayload {
                db_path: chatgptx_core::ccs_import::default_ccs_db_path()
                    .to_string_lossy()
                    .to_string(),
                configured_db_path: settings.ccs_db_path,
                fallback_reason: None,
                providers: Vec::new(),
            },
        ),
    }
}

#[tauri::command]
pub fn import_ccs_providers() -> CommandResult<SettingsPayload> {
    let store = SettingsStore::default();
    let mut settings = store.load().unwrap_or_default();
    let providers =
        match chatgptx_core::ccs_import::resolve_codex_provider_source(&settings.ccs_db_path) {
            Ok(source) => source.providers,
            Err(error) => {
                let payload = settings_payload_value().unwrap_or_else(|(_, payload)| payload);
                return failed(&format!("读取 cc-switch 供应商配置失败：{error}"), payload);
            }
        };

    let mut existing_keys: Vec<String> = settings
        .relay_profiles
        .iter()
        .map(chatgptx_core::ccs_import::imported_provider_identity)
        .collect();
    let mut existing_ids: Vec<String> = settings
        .relay_profiles
        .iter()
        .map(|profile| profile.id.clone())
        .collect();
    let mut imported = 0usize;

    for provider in providers {
        let key = chatgptx_core::ccs_import::provider_identity_from_ccs(&provider);
        if existing_keys.iter().any(|existing| existing == &key) {
            continue;
        }
        let profile = chatgptx_core::ccs_import::relay_profile_from_ccs(&provider, &existing_ids);
        existing_ids.push(profile.id.clone());
        existing_keys.push(key);
        settings.relay_profiles.push(profile);
        imported += 1;
    }

    if imported == 0 {
        return settings_payload("没有新的 cc-switch 供应商配置需要导入。", "设置读取失败");
    }

    settings = normalize_settings_before_save(settings);
    match store.save(&settings) {
        Ok(()) => settings_payload(
            &format!("已从 cc-switch 导入供应商配置：{imported} 个。"),
            "导入供应商配置后重新读取设置失败",
        ),
        Err(error) => failed(
            &format!("保存 cc-switch 供应商配置失败：{error}"),
            settings_payload_value().unwrap_or_else(|(_, payload)| payload),
        ),
    }
}

#[tauri::command]
pub fn load_pending_provider_import() -> CommandResult<PendingProviderImportPayload> {
    match chatgptx_core::provider_import::load_pending_provider_import() {
        Ok(pending) => ok(
            "待确认供应商导入已读取。",
            PendingProviderImportPayload { pending },
        ),
        Err(error) => failed(
            &format!("读取待确认供应商导入失败：{error}"),
            PendingProviderImportPayload { pending: None },
        ),
    }
}

#[tauri::command]
pub fn confirm_pending_provider_import() -> CommandResult<SettingsPayload> {
    match chatgptx_core::provider_import::confirm_pending_provider_import() {
        Ok(Some(result)) => {
            let message = if result.imported {
                format!("已导入供应商配置：{}。", result.profile_name)
            } else {
                format!("供应商配置已存在：{}。", result.profile_name)
            };
            settings_payload(&message, "供应商导入后重新读取设置失败")
        }
        Ok(None) => settings_payload("没有待确认的供应商导入。", "设置读取失败"),
        Err(error) => failed(
            &format!("导入供应商配置失败：{error}"),
            settings_payload_value().unwrap_or_else(|(_, payload)| payload),
        ),
    }
}

#[tauri::command]
pub fn dismiss_pending_provider_import() -> CommandResult<PendingProviderImportPayload> {
    match chatgptx_core::provider_import::clear_pending_provider_import() {
        Ok(()) => ok(
            "已取消供应商导入。",
            PendingProviderImportPayload { pending: None },
        ),
        Err(error) => failed(
            &format!("取消供应商导入失败：{error}"),
            PendingProviderImportPayload { pending: None },
        ),
    }
}

#[tauri::command]
pub fn list_local_sessions(
    request: Option<ListLocalSessionsRequest>,
) -> CommandResult<LocalSessionsPayload> {
    let request = request.unwrap_or(ListLocalSessionsRequest {
        offset: 0,
        limit: DEFAULT_LOCAL_SESSIONS_PAGE_SIZE,
    });
    let offset = request.offset;
    let limit = request.limit.clamp(1, MAX_LOCAL_SESSIONS_PAGE_SIZE);
    let fetch_limit = offset.saturating_add(limit).saturating_add(1);
    let home = chatgptx_core::codex_sqlite::default_codex_home_dir();
    let db_paths = chatgptx_core::codex_sqlite::codex_session_db_paths_from_home(&home);
    let mut sessions = Vec::new();
    let mut session_ids = std::collections::HashSet::new();
    let mut errors = Vec::new();
    for db_path in &db_paths {
        let adapter = local_session_adapter(db_path);
        match adapter.list_local_session_ids() {
            Ok(ids) => session_ids.extend(ids),
            Err(error) if db_path.exists() => {
                errors.push(format!("{}: {error}", db_path.to_string_lossy()));
                continue;
            }
            Err(_) => continue,
        }
        match adapter.list_local_sessions_limited(fetch_limit) {
            Ok(mut items) => sessions.append(&mut items),
            Err(error) if db_path.exists() => {
                errors.push(format!("{}: {error}", db_path.to_string_lossy()));
            }
            Err(_) => {}
        }
    }
    sessions.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| right.id.cmp(&left.id))
    });
    let mut seen_session_ids = std::collections::HashSet::new();
    sessions.retain(|session| seen_session_ids.insert(session.id.clone()));
    let has_more = sessions.len() > offset.saturating_add(limit);
    let sessions = sessions.into_iter().skip(offset).take(limit).collect();
    let payload = LocalSessionsPayload {
        db_path: db_paths
            .first()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        db_paths: db_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        sessions,
        offset,
        limit,
        has_more,
        total_count: session_ids.len(),
    };
    let page = offset / limit + 1;
    if errors.is_empty() {
        ok(
            &format!(
                "已读取第 {page} 页，共 {} 个本地会话。",
                payload.sessions.len()
            ),
            payload,
        )
    } else {
        failed(
            &format!("读取部分本地会话失败：{}", errors.join("; ")),
            payload,
        )
    }
}

#[tauri::command]
pub fn delete_local_session(request: DeleteLocalSessionRequest) -> CommandResult<DeleteResult> {
    let session_id = request.session_id.trim();
    if session_id.is_empty() {
        return failed(
            "会话 ID 不能为空。",
            DeleteResult {
                status: chatgptx_core::models::DeleteStatus::Failed,
                session_id: String::new(),
                message: "会话 ID 不能为空。".to_string(),
                undo_token: None,
                backup_path: None,
            },
        );
    }
    let session = SessionRef {
        session_id: session_id.to_string(),
        title: request.title,
        host_id: Some("local".into()),
    };
    let home = chatgptx_core::codex_sqlite::default_codex_home_dir();
    let mut candidate_paths = Vec::new();
    if let Some(path) = request.db_path.as_deref() {
        let path = PathBuf::from(path);
        if !candidate_paths.iter().any(|candidate| candidate == &path) {
            candidate_paths.push(path);
        }
    }
    for path in chatgptx_core::codex_sqlite::codex_session_db_paths_from_home(&home) {
        if !candidate_paths.iter().any(|candidate| candidate == &path) {
            candidate_paths.push(path);
        }
    }
    log_manager_event(
        "manager.delete_local_session.start",
        json!({
            "session_id": session_id,
            "title": session.title,
            "requested_db_path": request.db_path,
            "candidate_paths": candidate_paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
        }),
    );
    let result = chatgptx_data::delete_local_from_paths(
        candidate_paths.clone(),
        chatgptx_data::BackupStore::new(
            chatgptx_core::paths::default_app_state_dir().join("backups"),
        ),
        &session,
        Some(&home),
    );
    log_manager_event(
        "manager.delete_local_session.finish",
        json!({
            "session_id": session_id,
            "final_status": format!("{:?}", result.status),
            "final_message": result.message,
            "candidate_paths": candidate_paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
        }),
    );
    let status = if matches!(
        result.status,
        chatgptx_core::models::DeleteStatus::LocalDeleted
    ) {
        "ok"
    } else {
        "failed"
    };
    CommandResult {
        status: status.to_string(),
        message: result.message.clone(),
        payload: result,
    }
}

fn local_session_adapter(db_path: &Path) -> chatgptx_data::SQLiteStorageAdapter {
    chatgptx_data::SQLiteStorageAdapter::new(
        db_path,
        chatgptx_data::BackupStore::new(
            chatgptx_core::paths::default_app_state_dir().join("backups"),
        ),
    )
}

/// 归一化「Codex 应用路径」。**无效路径一律丢弃，不落库。**
///
/// 之前的写法是 normalize 成功才覆盖、失败就原样保留，于是误选的路径会被存进
/// settings.json。而 launcher 拿到显式 --app-path 且无效时不回退自动探测，
/// 结果就是启动永久失败、只能手改配置文件才能恢复（#1972：用户误选了 ChatGPTX
/// 自己的 chatgptx.exe，因为文件选择器只按 exe 扩展名过滤）。
///
/// 清空之后 resolve_codex_app_dir_with_saved 会走自动探测，至少还能起来。
fn normalized_codex_app_path_for_save(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    chatgptx_core::app_paths::normalize_codex_app_path(Path::new(raw))
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn normalize_settings_before_save(mut settings: BackendSettings) -> BackendSettings {
    settings.codex_app_path = normalized_codex_app_path_for_save(&settings.codex_app_path);
    settings.relay_common_config_contents =
        chatgptx_core::relay_config::sanitize_common_config_contents(
            &settings.relay_common_config_contents,
        );
    let (common_without_context, extracted_context) =
        split_relay_context_config_sections(&settings.relay_common_config_contents);
    settings.relay_common_config_contents = common_without_context;
    settings.relay_context_config_contents =
        relay_join_config_sections(&[&settings.relay_context_config_contents, &extracted_context]);
    settings.relay_context_config_contents =
        chatgptx_core::relay_config::sanitize_common_config_contents(
            &settings.relay_context_config_contents,
        );
    for profile in &mut settings.relay_profiles {
        if let Err(error) =
            chatgptx_core::relay_config::normalize_relay_profile_for_storage(profile)
        {
            log_manager_event(
                "manager.normalize_relay_profile_for_storage.failed",
                json!({
                    "profileId": profile.id,
                    "profileName": profile.name,
                    "error": error.to_string()
                }),
            );
        }
    }
    let common_config = relay_combined_common_config(&settings);
    if !common_config.trim().is_empty() {
        for profile in &mut settings.relay_profiles {
            if !profile.use_common_config || profile.config_contents.trim().is_empty() {
                continue;
            }
            let goals_override = relay_config_goals_value(&profile.config_contents);
            match chatgptx_core::relay_config::strip_common_config_from_config(
                &profile.config_contents,
                &common_config,
            ) {
                Ok(stripped) => {
                    profile.config_contents =
                        strip_common_config_text_fallback(&stripped, &common_config);
                }
                Err(_) => {
                    profile.config_contents =
                        strip_common_config_text_fallback(&profile.config_contents, &common_config);
                }
            }
            if let Some(enabled) = goals_override {
                profile.config_contents =
                    relay_config_set_goals_override(&profile.config_contents, enabled);
            }
        }
    }
    settings.provider_sync_saved_providers =
        normalize_provider_sync_provider_list(settings.provider_sync_saved_providers);
    settings.provider_sync_manual_providers =
        normalize_provider_sync_provider_list(settings.provider_sync_manual_providers);
    settings.provider_sync_last_selected_provider = settings
        .provider_sync_last_selected_provider
        .trim()
        .to_string();
    settings
}

fn relay_config_goals_value(config: &str) -> Option<bool> {
    let doc = config.parse::<toml_edit::DocumentMut>().ok()?;
    doc.get("features")?
        .as_table_like()?
        .get("goals")?
        .as_bool()
}

fn relay_config_set_goals_override(config: &str, enabled: bool) -> String {
    let Ok(mut doc) = config.parse::<toml_edit::DocumentMut>() else {
        return config.to_string();
    };
    if !doc.as_table().contains_key("features")
        || doc
            .get("features")
            .and_then(toml_edit::Item::as_table_like)
            .is_none()
    {
        doc["features"] = toml_edit::table();
    }
    doc["features"]["goals"] = toml_edit::value(enabled);
    chatgptx_core::relay_config::normalize_config_text(&doc.to_string())
}

fn normalize_provider_sync_provider_list(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            result.push(trimmed.to_string());
        }
    }
    result.sort();
    result
}

fn relay_combined_common_config(settings: &BackendSettings) -> String {
    relay_join_config_sections(&[
        &settings.relay_common_config_contents,
        &settings.relay_context_config_contents,
    ])
}

fn relay_join_config_sections(sections: &[&str]) -> String {
    let sections = sections
        .iter()
        .map(|section| section.trim())
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>();
    if sections.is_empty() {
        String::new()
    } else {
        chatgptx_core::relay_config::normalize_config_text(&format!(
            "{}\n",
            sections.join("\n\n")
        ))
    }
}

fn split_relay_context_config_sections(config: &str) -> (String, String) {
    let mut common = Vec::new();
    let mut context = Vec::new();
    let mut in_context_table = false;

    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_context_table = trimmed.starts_with("[mcp_servers.")
                || trimmed.starts_with("[skills.")
                || trimmed.starts_with("[plugins.");
        }
        if in_context_table {
            context.push(line);
        } else {
            common.push(line);
        }
    }

    (
        relay_join_config_sections(&[&common.join("\n")]),
        relay_join_config_sections(&[&context.join("\n")]),
    )
}

fn strip_common_config_text_fallback(config_contents: &str, common_config: &str) -> String {
    let common = common_config_anchors(common_config);
    if common.root_keys.is_empty() && common.table_headers.is_empty() {
        return ensure_text_newline(config_contents.trim_end());
    }

    let mut kept = Vec::new();
    let mut skipping_table = false;
    let mut in_root_section = true;
    let mut removed_root_keys = std::collections::HashSet::new();
    let source_root_keys = toml_root_keys_before_first_table(config_contents);

    for line in config_contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_root_section = false;
            let header = trimmed.to_string();
            skipping_table = common.table_headers.contains(&header);
            if skipping_table {
                continue;
            }
        }

        if skipping_table {
            continue;
        }

        if in_root_section && let Some(key) = toml_key_from_line(trimmed) {
            if common.root_keys.contains(key) {
                let is_duplicate_common_key = removed_root_keys.contains(key)
                    || source_root_keys.contains(key)
                    || common.table_headers.contains("[features]")
                    || common
                        .table_headers
                        .contains("[marketplaces.openai-bundled]")
                    || common
                        .table_headers
                        .contains("[plugins.\"superpowers@openai-curated\"]");
                if is_duplicate_common_key {
                    removed_root_keys.insert(key.to_string());
                    continue;
                }
            }
        }

        kept.push(line);
    }

    ensure_text_newline(kept.join("\n").trim_end())
}

fn toml_root_keys_before_first_table(config_contents: &str) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();
    for line in config_contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            break;
        }
        if let Some(key) = toml_key_from_line(trimmed) {
            keys.insert(key.to_string());
        }
    }
    keys
}

struct CommonConfigAnchors {
    root_keys: std::collections::HashSet<String>,
    table_headers: std::collections::HashSet<String>,
}

fn common_config_anchors(common_config: &str) -> CommonConfigAnchors {
    let mut root_keys = std::collections::HashSet::new();
    let mut table_headers = std::collections::HashSet::new();
    let mut in_table = false;

    for line in common_config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_table = true;
            table_headers.insert(trimmed.to_string());
            continue;
        }
        if !in_table {
            if let Some(key) = toml_key_from_line(trimmed) {
                root_keys.insert(key.to_string());
            }
        }
    }

    CommonConfigAnchors {
        root_keys,
        table_headers,
    }
}

fn toml_key_from_line(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, _) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() { None } else { Some(key) }
}

fn ensure_text_newline(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n", value.trim_end())
    }
}

#[tauri::command]
pub async fn load_provider_sync_targets() -> CommandResult<Value> {
    let settings = SettingsStore::default().load().unwrap_or_default();
    let result =
        tauri::async_runtime::spawn_blocking(|| chatgptx_data::load_provider_sync_targets(None))
            .await
            .map_err(|error| anyhow::anyhow!("provider target discovery task failed: {error}"));
    match result {
        Ok(mut targets) => {
            let manual = settings
                .provider_sync_manual_providers
                .iter()
                .chain(settings.provider_sync_saved_providers.iter())
                .filter_map(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                })
                .collect::<Vec<_>>();
            merge_manual_provider_sync_targets(&mut targets, &manual, &settings, None);
            ok(
                "Provider 同步目标已加载。",
                serde_json::to_value(targets).unwrap_or_else(|_| json!({})),
            )
        }
        Err(error) => failed(&format!("Provider 同步目标加载失败：{error}"), json!({})),
    }
}

fn merge_manual_provider_sync_targets(
    targets: &mut chatgptx_data::ProviderSyncTargetList,
    manual: &[String],
    settings: &BackendSettings,
    codex_home: Option<&Path>,
) {
    for id in manual {
        if let Some(existing) = targets.targets.iter_mut().find(|target| target.id == *id) {
            if !existing
                .sources
                .contains(&chatgptx_data::ProviderSyncTargetSource::Manual)
            {
                existing
                    .sources
                    .push(chatgptx_data::ProviderSyncTargetSource::Manual);
                existing.sources.sort();
            }
            existing.is_manual = settings.provider_sync_manual_providers.contains(id);
            existing.is_saved = settings.provider_sync_saved_providers.contains(id);
        } else {
            let (is_resolvable, unavailable_reason) =
                match chatgptx_data::validate_provider_sync_target(codex_home, Some(id)) {
                    Ok(_) => (true, None),
                    Err(reason) => (false, Some(reason)),
                };
            targets
                .targets
                .push(chatgptx_data::ProviderSyncTargetOption {
                    id: id.clone(),
                    sources: vec![chatgptx_data::ProviderSyncTargetSource::Manual],
                    is_current_provider: *id == targets.current_provider,
                    is_manual: settings.provider_sync_manual_providers.contains(id),
                    is_saved: settings.provider_sync_saved_providers.contains(id),
                    is_resolvable,
                    unavailable_reason,
                });
        }
    }
    targets.targets.sort_by(|left, right| {
        right
            .is_current_provider
            .cmp(&left.is_current_provider)
            .then_with(|| left.id.cmp(&right.id))
    });
}

#[tauri::command]
pub async fn repair_session_index() -> CommandResult<Value> {
    let result =
        tauri::async_runtime::spawn_blocking(|| chatgptx_data::repair_session_index(None))
            .await
            .map_err(|error| anyhow::anyhow!("session index repair task failed: {error}"))
            .and_then(|result| result);
    match result {
        Ok(report) => ok(
            &format!("会话索引检查完成，恢复 {} 条消息。", report.repaired_items),
            json!(report),
        ),
        Err(error) => failed(&format!("修复会话索引失败：{error}"), json!({})),
    }
}

#[tauri::command]
pub async fn load_session_index_repair_report() -> CommandResult<Value> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        chatgptx_data::load_session_index_repair_report(None)
    })
    .await
    .map_err(|error| anyhow::anyhow!("session index report task failed: {error}"))
    .and_then(|result| result);
    match result {
        Ok(report) => ok("已读取会话索引修复报告。", json!({ "report": report })),
        Err(error) => failed(&format!("读取会话索引修复报告失败：{error}"), json!({})),
    }
}

#[tauri::command]
pub async fn preview_session_index_cleanup() -> CommandResult<Value> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        chatgptx_data::preview_session_index_cleanup(None)
    })
    .await
    .map_err(|error| anyhow::anyhow!("session index cleanup preview task failed: {error}"))
    .and_then(|result| result);
    match result {
        Ok(preview) => ok(
            &format!(
                "发现 {} 条仅存在于任务索引中的候选记录。",
                preview.candidates.len()
            ),
            json!({
                "snapshotSha256": preview.snapshot_sha256,
                "candidates": preview.candidates,
            }),
        ),
        Err(error) => failed(&format!("预览失效任务索引失败：{error}"), json!({})),
    }
}

#[tauri::command]
pub async fn apply_session_index_cleanup(
    snapshot_sha256: String,
    thread_ids: Vec<String>,
) -> CommandResult<Value> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        chatgptx_data::apply_session_index_cleanup(None, &snapshot_sha256, &thread_ids)
    })
    .await
    .map_err(|error| anyhow::anyhow!("session index cleanup task failed: {error}"));
    match result {
        Ok(Ok(cleanup)) => ok(
            &format!(
                "已清理 {} 条失效任务索引；原索引已完整备份。",
                cleanup.pruned_entries
            ),
            json!({
                "prunedEntries": cleanup.pruned_entries,
                "backupDir": cleanup.backup_dir,
            }),
        ),
        Ok(Err(error)) => {
            let backup_hint = error
                .backup_dir
                .as_ref()
                .map(|path| format!(" 备份目录：{}。", path.to_string_lossy()))
                .unwrap_or_default();
            failed(
                &format!("清理失效任务索引失败：{}{backup_hint}", error.message),
                json!({ "backupDir": error.backup_dir }),
            )
        }
        Err(error) => failed(&format!("清理失效任务索引失败：{error}"), json!({})),
    }
}

const PROVIDER_SYNC_PROGRESS_EVENT: &str = "provider-sync-progress";

/// issue #240：执行前先给「会动到什么、备份在哪」的只读预览。
///
/// 用户报「修复历史会话后会话从列表消失」，所以真实执行前要先看影响范围。
/// 这条命令不写盘：不开锁、不备份、不改 sqlite，只统计。
#[tauri::command]
pub async fn preview_provider_sync() -> CommandResult<Value> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        chatgptx_data::provider_sync::preview_provider_sync(None)
    })
    .await
    .map_err(|error| anyhow::anyhow!("provider sync preview task failed: {error}"))
    .and_then(|result| result);
    match result {
        Ok(preview) => {
            let audit = &preview.audit;
            let message = if audit.catalog_only_sessions > 0 {
                format!(
                    "预览：审计发现 {} 条仅存在于会话目录的记录，其中 {} 条仍有当前 rollout、{} 条只能从历史备份恢复、{} 条没有恢复来源。执行时的备份目录：{}",
                    audit.catalog_only_sessions,
                    audit.catalog_only_with_current_rollout,
                    audit.catalog_only_with_backup_database,
                    audit.catalog_only_without_recovery_source,
                    preview.backup_root.to_string_lossy(),
                )
            } else {
                format!(
                    "预览：未发现仅存在于会话目录的记录。执行时的备份目录：{}",
                    preview.backup_root.to_string_lossy(),
                )
            };
            ok(
                &message,
                json!({
                    "targetProvider": preview.target_provider,
                    "repairAudit": preview.audit,
                    "backupRoot": preview.backup_root,
                }),
            )
        }
        Err(error) => failed(&format!("预览历史会话修复失败：{error}"), json!({})),
    }
}

#[tauri::command]
pub async fn sync_providers_now(
    window: tauri::WebviewWindow,
    target_provider: Option<String>,
) -> CommandResult<Value> {
    let target_provider = target_provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let target_for_validation = target_provider.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        chatgptx_data::validate_provider_sync_target(None, target_for_validation.as_deref())
    })
    .await
    {
        // Keep None as "use current" so the sync reads the live selection again.
        Ok(Ok(_)) => {}
        Ok(Err(message)) => return provider_sync_preflight_failure(&message),
        Err(error) => {
            return provider_sync_preflight_failure(&format!(
                "provider target validation task failed: {error}"
            ));
        }
    };
    let target_for_settings = target_provider.clone();
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let progress_window = window.clone();
    let before_home = home.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        // issue #1341：快照要读整份 app state 文件，属阻塞 IO，必须留在阻塞线程里，
        // 否则同步一跑起来就把 async 运行时的那条工作线程占住，界面卡死。
        prepare_codex_app_state_before_provider_switch(
            &before_home,
            "manager.sync_providers_now.before",
        );
        chatgptx_data::run_provider_sync_with_target_and_progress(
            None,
            target_provider.as_deref(),
            |progress| {
                let _ = progress_window.emit_to(
                    progress_window.label(),
                    PROVIDER_SYNC_PROGRESS_EVENT,
                    progress,
                );
            },
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("provider sync task failed: {error}"));
    match result {
        Ok(sync) => {
            if is_success_sync_status(&sync.status) {
                persist_provider_sync_selection(
                    target_for_settings
                        .as_deref()
                        .unwrap_or(&sync.target_provider),
                );
                finish_codex_app_state_after_provider_switch(
                    &home,
                    "manager.sync_providers_now.after",
                );
            }
            let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
                "manager.provider_sync.completed",
                json!({
                    "status": sync.status.clone(),
                    "changedSessionFiles": sync.changed_session_files,
                    "sqliteRowsUpdated": sync.sqlite_rows_updated,
                    "sqliteCatalogRowsInserted": sync.sqlite_catalog_rows_inserted,
                    "sqliteCatalogRowsRemoved": sync.sqlite_catalog_rows_removed,
                    "skippedLockedRolloutFiles": sync.skipped_locked_rollout_files.len(),
                }),
            );
            provider_sync_command_result(sync)
        }
        Err(error) => {
            let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
                "manager.provider_sync.failed",
                json!({ "message": error.to_string() }),
            );
            failed(&format!("供应商同步失败：{error}"), json!({}))
        }
    }
}

fn provider_sync_preflight_failure(message: &str) -> CommandResult<Value> {
    failed(
        &format!("供应商同步未执行：{message}"),
        json!({
            "syncStatus": "skipped",
            "targetProvider": "",
            "syncMessage": message,
        }),
    )
}

fn is_success_sync_status(status: &chatgptx_data::ProviderSyncStatus) -> bool {
    matches!(status, chatgptx_data::ProviderSyncStatus::Synced)
}

fn provider_sync_command_result(sync: chatgptx_data::ProviderSyncResult) -> CommandResult<Value> {
    let succeeded = is_success_sync_status(&sync.status);
    let success_message = format!(
        "供应商已同步一次：{} 个会话文件，{} 行索引，跳过 {} 个占用文件。{}",
        sync.changed_session_files,
        sync.sqlite_rows_updated,
        sync.skipped_locked_rollout_files.len(),
        if sync.repair_audit.catalog_only_sessions > 0 {
            format!(
                " 审计发现 {} 条仅存在于会话目录的记录，其中 {} 条没有可用恢复来源。",
                sync.repair_audit.catalog_only_sessions,
                sync.repair_audit.catalog_only_without_recovery_source,
            )
        } else {
            String::new()
        }
    );
    // issue #240：改动了记录就把备份位置一并告诉用户，出事时能自己找回。
    let success_message = match sync.backup_dir.as_ref() {
        Some(path) if sync.changed_session_files > 0 || sync.sqlite_catalog_rows_inserted > 0 => {
            format!("{success_message} 备份目录：{}", path.to_string_lossy())
        }
        _ => success_message,
    };
    let failure_message = format!("历史会话修复未执行：{}", sync.message);
    let payload = json!({
        "syncStatus": sync.status,
        "targetProvider": sync.target_provider,
        "changedSessionFiles": sync.changed_session_files,
        "skippedLockedRolloutFiles": sync.skipped_locked_rollout_files,
        "sqliteRowsUpdated": sync.sqlite_rows_updated,
        "sqliteProviderRowsUpdated": sync.sqlite_provider_rows_updated,
        "sqliteUserEventRowsUpdated": sync.sqlite_user_event_rows_updated,
        "sqliteCwdRowsUpdated": sync.sqlite_cwd_rows_updated,
        "sqliteCatalogRowsInserted": sync.sqlite_catalog_rows_inserted,
        "sqliteCatalogRowsRemoved": sync.sqlite_catalog_rows_removed,
        "updatedWorkspaceRoots": sync.updated_workspace_roots,
        "encryptedContentWarning": sync.encrypted_content_warning,
        "repairAudit": sync.repair_audit,
        "backupDir": sync.backup_dir,
        "syncMessage": sync.message,
    });
    if succeeded {
        ok(&success_message, payload)
    } else {
        failed(&failure_message, payload)
    }
}

fn persist_provider_sync_selection(provider: &str) {
    let trimmed = provider.trim();
    if trimmed.is_empty() {
        return;
    }
    let store = SettingsStore::default();
    let mut settings = store.load().unwrap_or_default();
    settings.provider_sync_last_selected_provider = trimmed.to_string();
    if !settings
        .provider_sync_saved_providers
        .iter()
        .any(|item| item == trimmed)
    {
        settings
            .provider_sync_saved_providers
            .push(trimmed.to_string());
    }
    settings.provider_sync_saved_providers =
        normalize_provider_sync_provider_list(settings.provider_sync_saved_providers);
    let _ = store.save(&settings);
}


#[tauri::command]
pub async fn refresh_script_market() -> CommandResult<ScriptMarketPayload> {
    match script_market::fetch_market_manifest(script_market::DEFAULT_MARKET_INDEX_URL).await {
        Ok(manifest) => ok(
            "脚本市场已刷新。",
            script_market_payload_from_manifest(&manifest, "ok", "脚本市场已刷新。"),
        ),
        Err(error) => failed(
            &format!("脚本市场加载失败：{error}"),
            failed_script_market_payload(&format!("脚本市场加载失败：{error}")),
        ),
    }
}

#[tauri::command]
pub async fn refresh_plugin_market(source: String, refresh: bool) -> Value {
    match chatgptx_core::plugin_market::list_plugins(
        &chatgptx_core::codex_home::default_codex_home_dir(),
        &source,
        refresh,
    )
    .await
    {
        Ok(payload) => payload,
        Err(error) => json!({"status": "failed", "message": error.to_string(), "plugins": []}),
    }
}

#[tauri::command]
pub async fn install_plugin_market_item(source: String, id: String) -> Value {
    match chatgptx_core::plugin_market::install_plugin(
        &chatgptx_core::codex_home::default_codex_home_dir(),
        &source,
        &id,
    )
    .await
    {
        Ok(payload) => payload,
        Err(error) => json!({"status": "failed", "message": error.to_string()}),
    }
}

#[tauri::command]
pub fn plugin_market_install_status(source: String, id: String) -> Value {
    match chatgptx_core::plugin_market::install_status(
        &chatgptx_core::codex_home::default_codex_home_dir(),
        &source,
        &id,
    ) {
        Ok(payload) => payload,
        Err(error) => json!({"status": "failed", "message": error.to_string(), "busy": false}),
    }
}

#[tauri::command]
pub async fn refresh_user_script_inventory() -> CommandResult<SettingsPayload> {
    let debug_port = StatusStore::default()
        .load_latest()
        .ok()
        .flatten()
        .and_then(|status| status.debug_port)
        .unwrap_or_else(default_debug_port);
    let manager = default_user_script_manager();
    let (user_scripts, message) = match chatgptx_core::user_scripts::live_runtime_status(
        debug_port,
    )
    .await
    {
        Ok(runtime_status) => (
            manager
                .inventory_with_runtime_status(Some(&runtime_status))
                .unwrap_or_else(
                    |error| json!({ "enabled": true, "scripts": [], "error": error.to_string() }),
                ),
            "已同步 Codex 用户脚本运行状态。",
        ),
        Err(_) => (
            manager.inventory().unwrap_or_else(
                |error| json!({ "enabled": true, "scripts": [], "error": error.to_string() }),
            ),
            "Codex 未运行或暂不可连接，已显示本地脚本状态。",
        ),
    };
    ok(
        message,
        SettingsPayload {
            settings: SettingsStore::default().load().unwrap_or_default(),
            settings_path: chatgptx_core::paths::default_settings_path()
                .to_string_lossy()
                .to_string(),
            user_scripts,
        },
    )
}

#[tauri::command]
pub async fn reload_user_scripts() -> CommandResult<SettingsPayload> {
    let debug_port = StatusStore::default()
        .load_latest()
        .ok()
        .flatten()
        .and_then(|status| status.debug_port)
        .unwrap_or_else(default_debug_port);
    let manager = default_user_script_manager();
    match chatgptx_core::user_scripts::reload_live_scripts(debug_port, &manager).await {
        Ok(user_scripts) => {
            let page_reload = user_scripts["reload_mode"] == "page";
            let script_failed = user_scripts["scripts"]
                .as_array()
                .is_some_and(|scripts| scripts.iter().any(|script| script["status"] == "failed"));
            let payload = SettingsPayload {
                settings: SettingsStore::default().load().unwrap_or_default(),
                settings_path: chatgptx_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                user_scripts,
            };
            if script_failed {
                failed("部分脚本执行失败，请查看本地脚本状态。", payload)
            } else {
                ok(
                    if page_reload {
                        "已请求刷新 Codex 页面以安全重载旧脚本。"
                    } else {
                        "用户脚本已热重载。"
                    },
                    payload,
                )
            }
        }
        Err(error) => failed(
            &format!("用户脚本热重载失败：{error}"),
            fallback_settings_payload(),
        ),
    }
}

#[tauri::command]
pub async fn install_market_script(id: String) -> CommandResult<ScriptMarketPayload> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return failed(
            "脚本 id 不能为空。",
            failed_script_market_payload("脚本 id 不能为空。"),
        );
    }
    let manifest =
        match script_market::fetch_market_manifest(script_market::DEFAULT_MARKET_INDEX_URL).await {
            Ok(manifest) => manifest,
            Err(error) => {
                return failed(
                    &format!("脚本市场加载失败：{error}"),
                    failed_script_market_payload(&format!("脚本市场加载失败：{error}")),
                );
            }
        };
    let Some(script) = manifest.scripts.iter().find(|script| script.id == trimmed) else {
        return failed(
            "市场清单中未找到该脚本。",
            script_market_payload_from_manifest(&manifest, "failed", "市场清单中未找到该脚本。"),
        );
    };
    let manager = default_user_script_manager();
    match script_market::install_market_script(&manager, script).await {
        Ok(()) => ok(
            "脚本已安装。",
            script_market_payload_from_manifest(&manifest, "ok", "脚本已安装。"),
        ),
        Err(error) => failed(
            &format!("安装脚本失败：{error}"),
            script_market_payload_from_manifest(
                &manifest,
                "failed",
                &format!("安装脚本失败：{error}"),
            ),
        ),
    }
}

#[tauri::command]
pub fn set_user_scripts_enabled(enabled: bool) -> CommandResult<SettingsPayload> {
    let manager = default_user_script_manager();
    match manager.set_global_enabled(enabled) {
        Ok(_) => settings_payload(
            if enabled {
                "本地拓展整体已开启。"
            } else {
                "本地拓展整体已关闭。"
            },
            "本地拓展整体开关设置失败",
        ),
        Err(error) => failed(
            &format!("本地拓展整体开关设置失败：{error}"),
            fallback_settings_payload(),
        ),
    }
}

#[tauri::command]
pub fn set_user_script_enabled(key: String, enabled: bool) -> CommandResult<SettingsPayload> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return failed("脚本 key 不能为空。", fallback_settings_payload());
    }
    let manager = default_user_script_manager();
    match manager.set_script_enabled(trimmed, enabled) {
        Ok(_) => settings_payload(
            if enabled {
                "脚本已启用。"
            } else {
                "脚本已禁用。"
            },
            "脚本启停失败",
        ),
        Err(error) => failed(
            &format!("脚本启停失败：{error}"),
            fallback_settings_payload(),
        ),
    }
}

#[tauri::command]
pub fn delete_user_script(key: String) -> CommandResult<SettingsPayload> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return failed("脚本 key 不能为空。", fallback_settings_payload());
    }
    let manager = default_user_script_manager();
    match manager.delete_user_script(trimmed) {
        Ok(_) => settings_payload("脚本已删除。", "脚本删除失败"),
        Err(error) => failed(
            &format!("脚本删除失败：{error}"),
            fallback_settings_payload(),
        ),
    }
}

/// 拉取所有启用仓库的 skill 清单，合并本地安装/启用状态后返回。
///
/// 单个仓库失败（限流、网络、仓库删了）不影响其它仓库，错误单独收集回前端。
#[tauri::command]
pub async fn refresh_skill_catalog() -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    let repos = manager.list_repos();
    let mut remote = Vec::new();
    let mut repo_errors = Vec::new();

    for repo in repos.iter().filter(|repo| repo.enabled) {
        let cached = cached_repo_skills(&repo.key());
        match chatgptx_core::skills::fetch_repo_skills(repo, &cached).await {
            Ok(skills) => {
                store_repo_skills(&repo.key(), &skills);
                remote.extend(skills);
            }
            Err(error) => {
                repo_errors.push(format!("{}/{}：{error}", repo.owner, repo.name));
                // 拉不动就先用上一次的结果撑着，别让已知的 skill 从列表里消失
                remote.extend(cached.into_values());
            }
        }
    }

    let message = if repo_errors.is_empty() {
        "Skills 列表已刷新。".to_string()
    } else {
        format!("Skills 列表已刷新，{} 个仓库拉取失败。", repo_errors.len())
    };
    let payload = skills_payload(&manager, &remote, repo_errors);
    if payload.repo_errors.is_empty() {
        ok(&message, payload)
    } else {
        failed(&message, payload)
    }
}

/// 只读本地状态，不联网。切到 Skills 页时先用它把已装的列出来。
#[tauri::command]
pub fn list_installed_skills() -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    let payload = skills_payload(&manager, &[], Vec::new());
    ok("已加载本地 Skills。", payload)
}

#[tauri::command]
pub async fn install_skill(repo_key: String, id: String) -> CommandResult<SkillsPayload> {
    install_or_update_skill(&repo_key, &id, "Skill 已安装。", "安装 Skill 失败").await
}

#[tauri::command]
pub async fn update_skill(repo_key: String, id: String) -> CommandResult<SkillsPayload> {
    install_or_update_skill(&repo_key, &id, "Skill 已更新。", "更新 Skill 失败").await
}

async fn install_or_update_skill(
    repo_key: &str,
    id: &str,
    success_message: &str,
    failure_prefix: &str,
) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    let Some(repo) = chatgptx_core::skills::parse_repo_key(repo_key) else {
        return failed(
            &format!("{failure_prefix}：仓库标识无法解析（{repo_key}）。"),
            current_skills_payload(&manager),
        );
    };

    // 装之前重新拉一次树，拿到当前的 repo_path 和哈希，避免用陈旧缓存装错版本。
    let cached = cached_repo_skills(repo_key);
    let skills = match chatgptx_core::skills::fetch_repo_skills(&repo, &cached).await {
        Ok(skills) => {
            store_repo_skills(repo_key, &skills);
            skills
        }
        Err(error) => {
            return failed(
                &format!("{failure_prefix}：{error}"),
                current_skills_payload(&manager),
            );
        }
    };
    let Some(skill) = skills.iter().find(|skill| skill.id == id) else {
        return failed(
            &format!("{failure_prefix}：仓库里没有找到 {id}。"),
            current_skills_payload(&manager),
        );
    };

    let zip = match chatgptx_core::skills::download_repo_zip(&repo).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return failed(
                &format!("{failure_prefix}：{error}"),
                current_skills_payload(&manager),
            );
        }
    };
    match manager.install_from_zip(skill, &zip) {
        Ok(_) => ok(success_message, current_skills_payload(&manager)),
        Err(error) => failed(
            &format!("{failure_prefix}：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

#[tauri::command]
pub fn set_skill_enabled(id: String, enabled: bool) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    match manager.set_enabled(id.trim(), enabled) {
        Ok(()) => ok(
            if enabled {
                "Skill 已启用，下次对话生效。"
            } else {
                "Skill 已停用。"
            },
            current_skills_payload(&manager),
        ),
        Err(error) => failed(
            &format!("Skill 启停失败：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

#[tauri::command]
pub fn uninstall_skill(id: String) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    match manager.uninstall(id.trim()) {
        Ok(_) => ok(
            "Skill 已卸载，源目录已备份，可随时恢复。",
            current_skills_payload(&manager),
        ),
        Err(error) => failed(
            &format!("卸载 Skill 失败：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

#[tauri::command]
pub fn restore_skill_backup(backup_id: String) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    match manager.restore_backup(backup_id.trim()) {
        Ok(_) => ok("Skill 已从备份恢复。", current_skills_payload(&manager)),
        Err(error) => failed(
            &format!("从备份恢复失败：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

#[tauri::command]
pub fn delete_skill_backup(backup_id: String) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    match manager.delete_backup(backup_id.trim()) {
        Ok(_) => ok("备份已删除。", current_skills_payload(&manager)),
        Err(error) => failed(
            &format!("删除备份失败：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

#[tauri::command]
pub fn upsert_skill_repo(repo: chatgptx_core::skills::SkillRepo) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    match manager.upsert_repo(repo) {
        Ok(_) => ok("仓库源已保存。", current_skills_payload(&manager)),
        Err(error) => failed(
            &format!("保存仓库源失败：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

#[tauri::command]
pub fn delete_skill_repo(key: String) -> CommandResult<SkillsPayload> {
    let manager = default_skills_manager();
    match manager.delete_repo(key.trim()) {
        Ok(_) => {
            forget_repo_skills(key.trim());
            ok("仓库源已删除。", current_skills_payload(&manager))
        }
        Err(error) => failed(
            &format!("删除仓库源失败：{error}"),
            current_skills_payload(&manager),
        ),
    }
}

fn skills_payload(
    manager: &chatgptx_core::skills::SkillsManager,
    remote: &[chatgptx_core::skills::RemoteSkill],
    repo_errors: Vec<String>,
) -> SkillsPayload {
    SkillsPayload {
        skills: if remote.is_empty() {
            manager.local_inventory()
        } else {
            manager.merge_entries(remote)
        },
        repos: manager.list_repos(),
        backups: manager.list_backups(),
        repo_errors,
        skills_dir: manager.source_dir().to_string_lossy().to_string(),
        codex_skills_dir: manager.linked_dir().to_string_lossy().to_string(),
    }
}

fn current_skills_payload(manager: &chatgptx_core::skills::SkillsManager) -> SkillsPayload {
    skills_payload(manager, &all_cached_repo_skills(), Vec::new())
}

fn default_skills_manager() -> chatgptx_core::skills::SkillsManager {
    chatgptx_core::skills::SkillsManager::new(
        chatgptx_core::paths::default_skills_source_dir(),
        chatgptx_core::paths::default_skill_backups_dir(),
        chatgptx_core::paths::default_skills_state_path(),
        chatgptx_core::codex_home::default_codex_home_dir(),
    )
}

/// 上一次成功拉取的远端清单，按仓库 key 存。
///
/// 两个用途：拉取时传给 `fetch_repo_skills` 跳过没变的 SKILL.md 请求；
/// 以及在只读命令里还原出完整视图，不必每次都联网。进程内缓存，重启即失效。
type RepoSkillCache = std::collections::HashMap<
    String,
    std::collections::BTreeMap<String, chatgptx_core::skills::RemoteSkill>,
>;

fn repo_skill_cache() -> &'static std::sync::Mutex<RepoSkillCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<RepoSkillCache>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(RepoSkillCache::new()))
}

fn cached_repo_skills(
    repo_key: &str,
) -> std::collections::BTreeMap<String, chatgptx_core::skills::RemoteSkill> {
    repo_skill_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(repo_key).cloned())
        .unwrap_or_default()
}

fn all_cached_repo_skills() -> Vec<chatgptx_core::skills::RemoteSkill> {
    repo_skill_cache()
        .lock()
        .ok()
        .map(|cache| {
            cache
                .values()
                .flat_map(|skills| skills.values().cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn store_repo_skills(repo_key: &str, skills: &[chatgptx_core::skills::RemoteSkill]) {
    if let Ok(mut cache) = repo_skill_cache().lock() {
        cache.insert(
            repo_key.to_string(),
            skills
                .iter()
                .map(|skill| (skill.id.clone(), skill.clone()))
                .collect(),
        );
    }
}

fn forget_repo_skills(repo_key: &str) {
    if let Ok(mut cache) = repo_skill_cache().lock() {
        cache.remove(repo_key);
    }
}

#[tauri::command]
pub fn open_external_url(url: String) -> CommandResult<Value> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return failed("只允许打开 http 或 https 链接。", json!({}));
    }
    match open_url(trimmed) {
        Ok(()) => ok("已在系统浏览器打开链接。", json!({ "url": trimmed })),
        Err(error) => failed(&format!("打开链接失败：{error}"), json!({ "url": trimmed })),
    }
}

#[tauri::command]
pub async fn repair_shortcuts() -> InstallActionResult {
    tauri::async_runtime::spawn_blocking(install::repair_shortcuts)
        .await
        .unwrap_or_else(|error| install_background_failure("修复快捷方式", error))
}

#[tauri::command]
pub async fn check_update() -> CommandResult<Value> {
    match chatgptx_core::update::check_for_update(chatgptx_core::version::VERSION).await {
        Ok(update) => {
            let status = if update.update_available {
                "ok"
            } else {
                "not_checked"
            };
            CommandResult {
                status: status.to_string(),
                message: if update.update_available {
                    "发现可用更新。".to_string()
                } else {
                    "当前已是最新版本。".to_string()
                },
                payload: json!({
                    "currentVersion": update.current_version,
                    "latestVersion": update.latest_version,
                    "releaseSummary": update.release_summary,
                    "assetName": update.asset_name,
                    "assetUrl": update.asset_url,
                    "updateAvailable": update.update_available,
                    "progress": 0
                }),
            }
        }
        Err(error) => failed(
            &format!("检查更新失败：{error}"),
            json!({
                "currentVersion": chatgptx_core::version::VERSION,
                "latestVersion": Value::Null,
                "releaseSummary": "",
                "assetName": Value::Null,
                "assetUrl": Value::Null,
                "updateAvailable": false,
                "progress": 0
            }),
        ),
    }
}

#[cfg(any(windows, target_os = "macos"))]
#[tauri::command]
pub async fn perform_update(
    app: tauri::AppHandle,
    release: Option<chatgptx_core::update::Release>,
) -> CommandResult<Value> {
    let result = perform_update_inner(release).await;
    if result.status == "ok" {
        // 返回下载结果后正常退出，安装器等待后台服务清理完成再替换程序。
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(750));
            app.exit(0);
        });
    }
    result
}

#[cfg(not(any(windows, target_os = "macos")))]
#[tauri::command]
pub async fn perform_update(
    release: Option<chatgptx_core::update::Release>,
) -> CommandResult<Value> {
    perform_update_inner(release).await
}

// 无需 app runtime 的业务核心；其它平台测试不应引入 Wry 析构依赖。
async fn perform_update_inner(
    release: Option<chatgptx_core::update::Release>,
) -> CommandResult<Value> {
    let Some(release) = release else {
        return failed(
            "请先检查更新并选择可下载的 Release asset。",
            json!({
                "currentVersion": chatgptx_core::version::VERSION,
                "progress": 0
            }),
        );
    };
    let download_dir = chatgptx_core::paths::default_app_state_dir().join("updates");
    match chatgptx_core::update::perform_update(&release, &download_dir).await {
        Ok(result) => ok(
            if cfg!(target_os = "macos") {
                "安装包已验证，ChatGPTX 将关闭以安装更新，完成后自动重启；失败时保留旧版。"
            } else {
                "安装包已下载并启动，请按安装向导完成更新。"
            },
            json!({
            "currentVersion": chatgptx_core::version::VERSION,
            "latestVersion": result.release.version,
            "releaseSummary": result.release.body,
            "installedPath": result.installer_path.to_string_lossy(),
            "launched": result.launched,
            "progress": 100
            }),
        ),
        Err(error) => failed(
            &format!("安装更新失败：{error}"),
            json!({
                "currentVersion": chatgptx_core::version::VERSION,
                "latestVersion": release.version,
                "releaseSummary": release.body,
                "progress": 0
            }),
        ),
    }
}

#[tauri::command]
pub fn load_watcher_state() -> CommandResult<WatcherPayload> {
    ok("开机启动状态已加载。", watcher_payload())
}

/// 界面上的「开机打开 ChatGPTX」开关：开启 = 注册并启用，关闭 = 注销并禁用。
#[tauri::command]
pub fn set_watcher_enabled(enabled: bool) -> CommandResult<WatcherPayload> {
    let launcher_path =
        chatgptx_core::install::companion_binary_path(chatgptx_core::install::SILENT_BINARY);
    match chatgptx_core::watcher::set_watcher_enabled(enabled, &launcher_path, default_debug_port())
    {
        Ok(()) if enabled => ok("开机启动已开启。", watcher_payload()),
        Ok(()) => ok("开机启动已关闭。", watcher_payload()),
        Err(error) if enabled => failed(&format!("开启开机启动失败：{error}"), watcher_payload()),
        Err(error) => failed(&format!("关闭开机启动失败：{error}"), watcher_payload()),
    }
}

#[tauri::command]
pub fn read_latest_logs(request: LogRequest) -> CommandResult<LogsPayload> {
    let path = chatgptx_core::paths::default_diagnostic_log_path();
    match read_tail(&path, request.lines) {
        Ok(tail) => ok(
            "日志已读取。",
            LogsPayload {
                path: path.to_string_lossy().to_string(),
                text: tail.text,
                lines: request.lines,
                truncated: tail.truncated,
                file_size: tail.file_size,
            },
        ),
        Err(error) => failed(
            &format!("读取日志失败：{error}"),
            LogsPayload {
                path: path.to_string_lossy().to_string(),
                text: String::new(),
                lines: request.lines,
                truncated: false,
                file_size: 0,
            },
        ),
    }
}

#[tauri::command]
pub fn clear_logs() -> CommandResult<LogsPayload> {
    let path = chatgptx_core::paths::default_diagnostic_log_path();
    match chatgptx_core::diagnostic_log::clear_diagnostic_log() {
        Ok(()) => ok(
            "日志已清理。",
            LogsPayload {
                path: path.to_string_lossy().to_string(),
                text: String::new(),
                lines: 0,
                truncated: false,
                file_size: 0,
            },
        ),
        Err(error) => failed(
            &format!("清理日志失败：{error}"),
            LogsPayload {
                path: path.to_string_lossy().to_string(),
                text: String::new(),
                lines: 0,
                truncated: false,
                file_size: 0,
            },
        ),
    }
}

#[tauri::command]
pub fn copy_diagnostics() -> CommandResult<DiagnosticsPayload> {
    ok(
        "诊断报告已生成。",
        DiagnosticsPayload {
            report: diagnostics_report(),
        },
    )
}

#[tauri::command]
pub fn reset_settings() -> CommandResult<SettingsPayload> {
    let settings = BackendSettings::default();
    match SettingsStore::default().save(&settings) {
        Ok(()) => settings_payload("设置已重置为默认值。", "设置重置后重新读取失败"),
        Err(error) => failed(
            &format!("重置设置失败：{error}"),
            SettingsPayload {
                settings,
                settings_path: chatgptx_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                user_scripts: user_script_inventory(),
            },
        ),
    }
}

#[tauri::command]
pub fn reset_image_overlay_settings() -> CommandResult<SettingsPayload> {
    let store = SettingsStore::default();
    let mut settings = store.load().unwrap_or_default();
    let defaults = BackendSettings::default();
    settings.codex_app_image_overlay_enabled = defaults.codex_app_image_overlay_enabled;
    settings.codex_app_image_overlay_path = defaults.codex_app_image_overlay_path;
    settings.codex_app_image_overlay_opacity = defaults.codex_app_image_overlay_opacity;
    settings.codex_app_image_overlay_fit_mode = defaults.codex_app_image_overlay_fit_mode;
    let settings = normalize_settings_before_save(settings);
    match store.save(&settings) {
        Ok(()) => settings_payload("图片覆盖层设置已重置。", "图片覆盖层重置后重新读取失败"),
        Err(error) => failed(
            &format!("重置图片覆盖层失败：{error}"),
            SettingsPayload {
                settings,
                settings_path: chatgptx_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                user_scripts: user_script_inventory(),
            },
        ),
    }
}

#[tauri::command]
pub fn relay_status() -> CommandResult<RelayPayload> {
    let status = chatgptx_core::relay_config::default_relay_status();
    let message = if status.authenticated {
        "已检测到 ChatGPT 登录状态。"
    } else {
        "未检测到 ChatGPT 登录状态，请先在 Codex/ChatGPT 中正常登录。"
    };
    ok(message, relay_payload(status, None))
}

#[tauri::command]
pub fn read_relay_files() -> CommandResult<RelayFilesPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    match relay_files_payload_from_home(&home) {
        Ok(payload) => ok("配置文件内容已读取。", payload),
        Err(error) => failed(
            &format!("读取配置文件失败：{error}"),
            RelayFilesPayload {
                config_path: home.join("config.toml").to_string_lossy().to_string(),
                auth_path: home.join("auth.json").to_string_lossy().to_string(),
                config_contents: String::new(),
                auth_contents: String::new(),
            },
        ),
    }
}

#[tauri::command]
pub fn check_env_conflicts() -> CommandResult<EnvConflictsPayload> {
    let conflicts = chatgptx_core::env_conflicts::detect_env_conflicts();
    let message = if conflicts.is_empty() {
        "未检测到会覆盖 Codex 供应商配置的 OPENAI 环境变量。"
    } else {
        "检测到可能覆盖 Codex 供应商配置的 OPENAI 环境变量。"
    };
    ok(message, EnvConflictsPayload { conflicts })
}

#[tauri::command]
pub fn check_relay_environment() -> CommandResult<RelayEnvironmentReport> {
    let report = chatgptx_core::relay_environment::inspect_relay_environment();
    let message = if report.all_passed() {
        "中转站环境配置检测全部通过。"
    } else {
        "检测到可能影响中转站配置的环境问题。"
    };
    ok(message, report)
}

#[tauri::command]
pub fn remove_env_conflicts(
    request: RemoveEnvConflictsRequest,
) -> CommandResult<RemoveEnvConflictsPayload> {
    let backup_dir = chatgptx_core::paths::default_app_state_dir().join("backups");
    match chatgptx_core::env_conflicts::remove_env_conflicts(&request.names, backup_dir) {
        Ok(result) => {
            let remaining = chatgptx_core::env_conflicts::detect_env_conflicts();
            ok(
                "环境变量已按确认项删除；重新启动 Codex 后生效。",
                RemoveEnvConflictsPayload {
                    removed: result.removed,
                    backup_path: result.backup_path,
                    remaining,
                },
            )
        }
        Err(error) => failed(
            &format!("删除环境变量失败：{error}"),
            RemoveEnvConflictsPayload {
                removed: Vec::new(),
                backup_path: None,
                remaining: chatgptx_core::env_conflicts::detect_env_conflicts(),
            },
        ),
    }
}

#[tauri::command]
pub fn save_relay_file(request: SaveRelayFileRequest) -> CommandResult<RelayFilesPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let Ok(_guard) = relay_switch_mutex().lock() else {
        return failed(
            "供应商切换锁已损坏，请重启管理器后再试。",
            relay_files_payload_from_home(&home).unwrap_or_else(|_| RelayFilesPayload {
                config_path: home.join("config.toml").to_string_lossy().to_string(),
                auth_path: home.join("auth.json").to_string_lossy().to_string(),
                config_contents: String::new(),
                auth_contents: String::new(),
            }),
        );
    };
    let active_profile = SettingsStore::default()
        .load()
        .unwrap_or_default()
        .active_relay_profile();
    let contents =
        match prepare_relay_file_contents(&request.kind, &request.contents, &active_profile) {
            Ok(contents) => contents,
            Err(error) => {
                return failed(
                    &format!("保存配置文件失败：{error}"),
                    relay_files_payload_from_home(&home).unwrap_or_else(|_| RelayFilesPayload {
                        config_path: home.join("config.toml").to_string_lossy().to_string(),
                        auth_path: home.join("auth.json").to_string_lossy().to_string(),
                        config_contents: String::new(),
                        auth_contents: String::new(),
                    }),
                );
            }
        };
    match save_relay_file_in_home(&home, &request.kind, &contents)
        .and_then(|_| relay_files_payload_from_home(&home))
    {
        Ok(payload) => ok("配置文件已保存。", payload),
        Err(error) => failed(
            &format!("保存配置文件失败：{error}"),
            relay_files_payload_from_home(&home).unwrap_or_else(|_| RelayFilesPayload {
                config_path: home.join("config.toml").to_string_lossy().to_string(),
                auth_path: home.join("auth.json").to_string_lossy().to_string(),
                config_contents: String::new(),
                auth_contents: String::new(),
            }),
        ),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProfileSwitchRequest {
    pub settings: BackendSettings,
    #[serde(default)]
    pub previous_active_relay_id: String,
}

#[tauri::command]
pub fn switch_relay_profile(
    request: RelayProfileSwitchRequest,
) -> CommandResult<RelaySwitchPayload> {
    let Ok(_guard) = relay_switch_mutex().lock() else {
        let status = chatgptx_core::relay_config::default_relay_status();
        return failed(
            "供应商切换锁已损坏，请重启管理器后再试。",
            relay_switch_payload(
                SettingsStore::default().load().unwrap_or_default(),
                status,
                None,
            ),
        );
    };
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let store = SettingsStore::default();
    let settings = normalize_settings_before_save(request.settings);
    let previous_active_relay_id = if settings
        .relay_profiles
        .iter()
        .any(|p| p.id == request.previous_active_relay_id)
    {
        request.previous_active_relay_id
    } else {
        String::new()
    };
    chatgptx_core::official_accounts::sync_live_account_tokens_to_store(&home);
    log_manager_event(
        "manager.switch_relay_profile.start",
        json!({
            "previousActiveRelayId": previous_active_relay_id,
            "targetRelayId": settings.active_relay_id
        }),
    );
    match chatgptx_core::relay_switch::switch_relay_profile_in_home(
        &store,
        &home,
        settings,
        &previous_active_relay_id,
    ) {
        Ok(result) => {
            let active_prof = result.settings.active_relay_profile();
            if active_prof.relay_mode != chatgptx_core::settings::RelayMode::Aggregate {
                let _ = chatgptx_core::official_accounts::deactivate_official_account_for_pure_api(
                    &home,
                    &active_prof.api_key,
                );
            }
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_manager_event(
                "manager.switch_relay_profile.ok",
                json!({
                    "targetRelayId": result.settings.active_relay_id,
                    "configured": status.configured,
                    "backupPath": result.backup_path.as_ref()
                }),
            );
            ok(
                "供应商已切换（纯 API 模式）。",
                relay_switch_payload(result.settings, status, result.backup_path),
            )
        }
        Err(error) => {
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            let settings = store.load().unwrap_or_default();
            log_manager_event(
                "manager.switch_relay_profile.failed",
                json!({
                    "previousActiveRelayId": previous_active_relay_id,
                    "activeRelayId": settings.active_relay_id,
                    "error": error.to_string()
                }),
            );
            failed(
                &format!("供应商切换失败：{error}"),
                relay_switch_payload(settings, status, None),
            )
        }
    }
}

#[tauri::command]
pub fn write_diagnostic_event(event: String, detail: Value) -> CommandResult<Value> {
    let event = sanitize_manager_event(&event);
    match chatgptx_core::diagnostic_log::append_diagnostic_log(&event, detail) {
        Ok(()) => ok("诊断日志已写入。", json!({})),
        Err(error) => failed(&format!("写入诊断日志失败：{error}"), json!({})),
    }
}

/// 预检 live config.toml 能否被回填流程解析（issue #618）。
///
/// core 侧的 `backfill_relay_profile_from_home_with_common` 会先做重复表头/重复
/// 根键的语义归一化（`normalize_duplicate_toml_text`，公开入口是
/// `normalize_config_text`），再对归一化结果做整份 TOML 解析。所以这里必须走同一
/// 条归一化路径，否则会对「只是有重复表头」的文件误报语法错误。
///
/// 返回 `Some(错误描述)` 表示回填注定失败：要么读文件失败，要么归一化后仍不是合法
/// TOML。返回 `None` 表示可以继续回填。文件不存在等同于空文件，属可回填。
fn live_config_backfill_blocking_error(home: &Path) -> Option<String> {
    let config_path = home.join("config.toml");
    let contents = match fs::read_to_string(&config_path) {
        Ok(contents) => contents,
        // 不存在按空文件处理，与 core 的 read_optional_text 一致。
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => return Some(format!("读取 {} 失败：{error}", config_path.display())),
    };
    // 与 core 的 parse_toml_document 一样先剥掉 BOM。
    let contents = contents.trim_start_matches('\u{feff}');
    if contents.trim().is_empty() {
        return None;
    }
    let normalized = chatgptx_core::relay_config::normalize_config_text(contents);
    match normalized.parse::<toml_edit::DocumentMut>() {
        Ok(_) => None,
        Err(error) => Some(format!("{error}")),
    }
}

#[tauri::command]
pub fn backfill_relay_profile_from_live(
    request: BackfillRelayProfileRequest,
) -> CommandResult<SettingsBackfillPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let mut settings = request.settings;
    let requested_profile_id = request.profile_id.clone();
    log_manager_event(
        "manager.backfill_relay_profile_from_live.start",
        json!({
            "profileId": requested_profile_id,
            "activeRelayId": settings.active_relay_id
        }),
    );
    let Some(profile_index) = settings
        .relay_profiles
        .iter()
        .position(|profile| profile.id == request.profile_id)
    else {
        log_manager_event(
            "manager.backfill_relay_profile_from_live.missing_profile",
            json!({
                "profileId": requested_profile_id
            }),
        );
        return failed(
            "当前供应商已不在配置列表中，已停止切换以避免覆盖用户改动。",
            SettingsBackfillPayload { settings },
        );
    };

    // live 的 config.toml 只要有一处手写语法错误，core 的整份 TOML 解析就会失败
    // （relay_config.rs 的 parse_toml_document），回填因此无法进行。但「回填」只是
    // 切换前的一次快照采集，不是切换到新供应商的必要条件；让整条切换流程失败会
    // 把用户卡死在中转态（issue #618）。这里比照同文件 2089 / 2350 两处既有写法，
    // 在解析失败时降级：保留原 settings 不动、只提示用户先修 config.toml 语法。
    if let Some(error) = live_config_backfill_blocking_error(&home) {
        log_manager_event(
            "manager.backfill_relay_profile_from_live.degraded",
            json!({
                "profileId": requested_profile_id,
                "error": error
            }),
        );
        return degraded(
            &format!("config.toml 有语法错误，已跳过回填并保留原有配置（切换照常进行）：{error}"),
            SettingsBackfillPayload { settings },
        );
    }

    if !request.adopt_provider_identity {
        match chatgptx_core::relay_config::live_config_matches_other_profile_in_home(
            &home,
            &settings,
            &request.profile_id,
        ) {
            Ok(true) => {
                return ok(
                    "实时配置属于其它已保存供应商，已保留原快照。",
                    SettingsBackfillPayload { settings },
                );
            }
            Ok(false) => {}
            Err(error) => {
                return failed(
                    &format!("读取实时配置归属失败，已保留原配置：{error}"),
                    SettingsBackfillPayload { settings },
                );
            }
        }
    }

    // 回填会就地改写 profile 与公共配置。先在副本上跑，成功才提交，避免中途
    // 失败时留下「改了一半」的 profile 被后续切换流程用上。
    let mut next_profile = settings.relay_profiles[profile_index].clone();
    let mut next_context = settings.relay_context_config_contents.clone();
    match chatgptx_core::relay_config::backfill_relay_profile_from_home_with_common_and_policy(
        &home,
        &mut next_profile,
        &mut next_context,
        if request.adopt_provider_identity {
            chatgptx_core::relay_config::RelayBackfillPolicy::AdoptLiveIdentity
        } else {
            chatgptx_core::relay_config::RelayBackfillPolicy::PreserveIdentity
        },
    ) {
        Ok(()) => {
            let changed = next_profile != settings.relay_profiles[profile_index]
                || next_context != settings.relay_context_config_contents;
            settings.relay_profiles[profile_index] = next_profile;
            settings.relay_context_config_contents = next_context;
            log_manager_event(
                "manager.backfill_relay_profile_from_live.ok",
                json!({
                    "profileId": requested_profile_id
                }),
            );
            ok(
                if !changed {
                    "实时配置没有需要回填的内容，已保留当前供应商快照。"
                } else if request.adopt_provider_identity {
                    "已采纳当前 URL 与认证，并重置旧自定义请求头。"
                } else {
                    "当前供应商配置已从 live 文件回填。"
                },
                SettingsBackfillPayload { settings },
            )
        }
        Err(error) => {
            log_manager_event(
                "manager.backfill_relay_profile_from_live.failed",
                json!({
                    "profileId": requested_profile_id,
                    "error": error.to_string()
                }),
            );
            failed(
                &format!("回填当前供应商配置失败：{error}"),
                SettingsBackfillPayload { settings },
            )
        }
    }
}

#[tauri::command]
pub fn list_context_entries(
    request: ContextSettingsRequest,
) -> CommandResult<ContextEntriesPayload> {
    match chatgptx_core::relay_config::list_context_entries_from_common_config(
        &request.settings.relay_context_config_contents,
    ) {
        Ok(entries) => ok(
            "MCP&插件列表已读取。",
            ContextEntriesPayload {
                settings: request.settings,
                entries,
            },
        ),
        Err(error) => failed(
            &format!("读取MCP&插件列表失败：{error}"),
            ContextEntriesPayload {
                settings: request.settings,
                entries: empty_context_entries(),
            },
        ),
    }
}

#[tauri::command]
pub fn read_live_context_entries() -> CommandResult<LiveContextEntriesPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let config_path = home.join("config.toml");
    let config = read_optional_text_file(&config_path).unwrap_or_default();
    match chatgptx_core::relay_config::list_context_entries_from_common_config(&config) {
        Ok(entries) => ok(
            "live MCP&插件已读取。",
            LiveContextEntriesPayload { entries },
        ),
        Err(error) => failed(
            &format!("读取 live MCP&插件失败：{error}"),
            LiveContextEntriesPayload {
                entries: empty_context_entries(),
            },
        ),
    }
}

#[tauri::command]
pub fn upsert_context_entry(request: ContextEntryRequest) -> CommandResult<ContextEntriesPayload> {
    let mut settings = request.settings;
    match chatgptx_core::relay_config::upsert_context_entry_in_common_config(
        &settings.relay_context_config_contents,
        &request.kind,
        &request.id,
        &request.toml_body,
    ) {
        Ok(common) => {
            settings.relay_context_config_contents = common;
            list_context_entries(ContextSettingsRequest { settings })
        }
        Err(error) => failed(
            &format!("保存MCP&插件失败：{error}"),
            ContextEntriesPayload {
                settings,
                entries: empty_context_entries(),
            },
        ),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpFormPayload {
    pub form: chatgptx_core::mcp_config::McpServerForm,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTomlPayload {
    pub toml_body: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpImportRequest {
    pub settings: BackendSettings,
    pub json: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpImportPreviewPayload {
    pub entries: Vec<chatgptx_core::mcp_config::McpJsonEntry>,
    pub warnings: Vec<String>,
}

/// 把 TOML 表体拆成表单字段。纯转换，不碰 settings——
/// 打开编辑器时解析一次即可，不必每次按键都往返后端。
#[tauri::command]
pub fn parse_mcp_entry(toml_body: String) -> CommandResult<McpFormPayload> {
    match chatgptx_core::mcp_config::parse_mcp_toml_body(&toml_body) {
        Ok(form) => ok("已解析 MCP 配置。", McpFormPayload { form }),
        Err(error) => failed(
            &format!("解析 MCP 配置失败：{error}"),
            McpFormPayload {
                form: Default::default(),
            },
        ),
    }
}

/// 表单字段拼回 TOML 表体。同样是纯转换。
#[tauri::command]
pub fn build_mcp_entry(
    form: chatgptx_core::mcp_config::McpServerForm,
) -> CommandResult<McpTomlPayload> {
    match chatgptx_core::mcp_config::build_mcp_toml_body(&form) {
        Ok(toml_body) => ok("已生成 MCP 配置。", McpTomlPayload { toml_body }),
        Err(error) => failed(
            &format!("生成 MCP 配置失败：{error}"),
            McpTomlPayload {
                toml_body: String::new(),
            },
        ),
    }
}

/// 只解析不写入，让用户先看清会导入哪几条、有哪些字段被改写。
#[tauri::command]
pub fn preview_mcp_servers_json(json: String) -> CommandResult<McpImportPreviewPayload> {
    match chatgptx_core::mcp_config::parse_mcp_servers_json(&json) {
        Ok(import) => {
            let message = if import.warnings.is_empty() {
                format!("解析出 {} 个 MCP 服务器。", import.entries.len())
            } else {
                format!(
                    "解析出 {} 个 MCP 服务器，{} 处需要注意。",
                    import.entries.len(),
                    import.warnings.len()
                )
            };
            ok(
                &message,
                McpImportPreviewPayload {
                    entries: import.entries,
                    warnings: import.warnings,
                },
            )
        }
        Err(error) => failed(
            &format!("解析 JSON 失败：{error}"),
            McpImportPreviewPayload {
                entries: Vec::new(),
                warnings: Vec::new(),
            },
        ),
    }
}

/// 批量写入。一次性更新 settings，避免 N 条 MCP 就往返 N 次。
#[tauri::command]
pub fn import_mcp_servers_json(request: McpImportRequest) -> CommandResult<ContextEntriesPayload> {
    let mut settings = request.settings;
    let import = match chatgptx_core::mcp_config::parse_mcp_servers_json(&request.json) {
        Ok(import) => import,
        Err(error) => {
            return failed(
                &format!("解析 JSON 失败：{error}"),
                ContextEntriesPayload {
                    settings,
                    entries: empty_context_entries(),
                },
            );
        }
    };

    let total = import.entries.len();
    let mut common = settings.relay_context_config_contents.clone();
    for entry in &import.entries {
        match chatgptx_core::relay_config::upsert_context_entry_in_common_config(
            &common,
            "mcp",
            &entry.id,
            &entry.toml_body,
        ) {
            Ok(updated) => common = updated,
            Err(error) => {
                return failed(
                    &format!("导入 {} 失败：{error}", entry.id),
                    ContextEntriesPayload {
                        settings,
                        entries: empty_context_entries(),
                    },
                );
            }
        }
    }

    settings.relay_context_config_contents = common;
    let mut result = list_context_entries(ContextSettingsRequest { settings });
    result.message = if import.warnings.is_empty() {
        format!("已导入 {total} 个 MCP 服务器。")
    } else {
        format!(
            "已导入 {total} 个 MCP 服务器：{}",
            import.warnings.join("；")
        )
    };
    result
}

#[tauri::command]
pub fn sync_live_context_entries(
    request: ContextSettingsRequest,
) -> CommandResult<LiveContextEntriesPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let config_path = home.join("config.toml");
    let current_config = match read_optional_text_file(&config_path) {
        Ok(config) => config,
        Err(error) => {
            return failed(
                &format!("读取 live config.toml 失败：{error}"),
                LiveContextEntriesPayload {
                    entries: empty_context_entries(),
                },
            );
        }
    };
    let updated_config = match chatgptx_core::relay_config::sync_live_config_context_entries(
        &current_config,
        &request.settings.relay_context_config_contents,
    ) {
        Ok(config) => config,
        Err(error) => {
            return failed(
                &format!("同步 live MCP&插件失败：{error}"),
                LiveContextEntriesPayload {
                    entries: empty_context_entries(),
                },
            );
        }
    };
    if let Some(parent) = config_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return failed(
                &format!("创建 Codex 配置目录失败：{error}"),
                LiveContextEntriesPayload {
                    entries: empty_context_entries(),
                },
            );
        }
    }
    if let Err(error) = std::fs::write(&config_path, &updated_config) {
        return failed(
            &format!("写入 live config.toml 失败：{error}"),
            LiveContextEntriesPayload {
                entries: empty_context_entries(),
            },
        );
    }
    match chatgptx_core::relay_config::list_context_entries_from_common_config(&updated_config) {
        Ok(entries) => ok(
            "live MCP&插件已同步。",
            LiveContextEntriesPayload { entries },
        ),
        Err(error) => failed(
            &format!("读取同步后的 live MCP&插件失败：{error}"),
            LiveContextEntriesPayload {
                entries: empty_context_entries(),
            },
        ),
    }
}

#[tauri::command]
pub fn delete_context_entry(request: ContextDeleteRequest) -> CommandResult<ContextEntriesPayload> {
    let mut settings = request.settings;
    match chatgptx_core::relay_config::delete_context_entry_from_common_config(
        &settings.relay_context_config_contents,
        &request.kind,
        &request.id,
    ) {
        Ok(common) => {
            settings.relay_context_config_contents = common;
            list_context_entries(ContextSettingsRequest { settings })
        }
        Err(error) => failed(
            &format!("删除MCP&插件失败：{error}"),
            ContextEntriesPayload {
                settings,
                entries: empty_context_entries(),
            },
        ),
    }
}

#[tauri::command]
pub fn extract_relay_common_config(
    request: ExtractRelayCommonConfigRequest,
) -> CommandResult<ExtractRelayCommonConfigPayload> {
    match chatgptx_core::relay_config::extract_common_config_from_config(&request.config_contents)
        .and_then(|common_config_contents| {
            let profile_config_contents =
                chatgptx_core::relay_config::strip_common_config_from_config(
                    &request.config_contents,
                    &common_config_contents,
                )?;
            Ok(ExtractRelayCommonConfigPayload {
                common_config_contents,
                profile_config_contents,
            })
        }) {
        Ok(payload) => ok("通用配置已按兼容切换规则提取。", payload),
        Err(error) => failed(
            &format!("提取通用配置失败：{error}"),
            ExtractRelayCommonConfigPayload {
                common_config_contents: String::new(),
                profile_config_contents: request.config_contents,
            },
        ),
    }
}

#[tauri::command]
pub async fn test_relay_profile(profile: RelayProfile) -> CommandResult<RelayProfileTestPayload> {
    let profile_name = if profile.name.trim().is_empty() {
        "未命名供应商"
    } else {
        profile.name.trim()
    };
    let settings = SettingsStore::default().load().unwrap_or_default();
    let test_model: String = if !profile.test_model.trim().is_empty() {
        // 1. 使用者在該供應商明確填的測試模型
        profile.test_model.trim().to_string()
    } else {
        // 2. 該供應商自己 config.toml 裡的 model（避免串味）
        let from_profile = chatgptx_core::relay_config::relay_profile_model(&profile);
        if from_profile.trim().is_empty() {
            // 3. 最後才用全域預設
            settings.relay_test_model.trim().to_string()
        } else {
            from_profile
        }
    };
    match chatgptx_core::relay_config::test_relay_profile(&profile, &test_model).await {
        Ok(result) => {
            let status = if result.http_status < 400 {
                "ok"
            } else {
                "failed"
            };
            let preview = result.response_preview.trim();
            let detail = if preview.is_empty() {
                "响应内容为空".to_string()
            } else {
                format!("响应：{preview}")
            };
            CommandResult {
                status: status.to_string(),
                message: format!(
                    "已向「{profile_name}」用模型「{test_model}」发送 hi，HTTP {}。{detail}",
                    result.http_status
                ),
                payload: RelayProfileTestPayload {
                    http_status: result.http_status,
                    endpoint: result.endpoint,
                    response_preview: result.response_preview,
                },
            }
        }
        Err(error) => failed(
            &format!("测试「{profile_name}」失败：{error}"),
            RelayProfileTestPayload {
                http_status: 0,
                endpoint: String::new(),
                response_preview: String::new(),
            },
        ),
    }
}

#[tauri::command]
pub async fn test_stepwise_settings(
    settings: BackendSettings,
) -> CommandResult<StepwiseTestPayload> {
    let configured_protocol = chatgptx_core::settings::normalize_stepwise_protocol(
        &settings.codex_app_stepwise_protocol,
    );
    match chatgptx_core::stepwise::test_connection(&settings).await {
        Ok(result) => {
            let error = result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let item_count = result
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            let protocol = result
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or(&configured_protocol)
                .to_string();
            if error.is_empty() {
                ok(
                    &format!(
                        "下一步建议连接正常（{}），测试返回 {item_count} 条建议。",
                        stepwise_protocol_label(&protocol)
                    ),
                    StepwiseTestPayload { item_count, error },
                )
            } else {
                failed(
                    &format!("下一步建议测试失败：{error}"),
                    StepwiseTestPayload { item_count, error },
                )
            }
        }
        Err(error) => failed(
            &format!("下一步建议测试失败：{error}"),
            StepwiseTestPayload {
                item_count: 0,
                error: error.to_string(),
            },
        ),
    }
}

fn stepwise_protocol_label(protocol: &str) -> &str {
    match protocol {
        "chat_completions" => "Chat Completions",
        "responses" => "Responses",
        "anthropic_messages" => "Anthropic Messages",
        "auto" => "自动兼容",
        _ => protocol,
    }
}

#[tauri::command]
pub async fn fetch_relay_profile_models(
    profile: RelayProfile,
) -> CommandResult<RelayProfileModelsPayload> {
    let profile_name = if profile.name.trim().is_empty() {
        "未命名供应商"
    } else {
        profile.name.trim()
    };
    match chatgptx_core::model_catalog::fetch_relay_profile_model_ids(&profile).await {
        Ok((models, endpoint)) => ok(
            &format!("已从「{profile_name}」获取 {} 个模型。", models.len()),
            RelayProfileModelsPayload { models, endpoint },
        ),
        Err(error) => failed(
            &format!("从「{profile_name}」获取模型失败：{error}"),
            RelayProfileModelsPayload {
                models: Vec::new(),
                endpoint: String::new(),
            },
        ),
    }
}

#[tauri::command]
pub async fn fetch_sub2api_billing(profile: RelayProfile) -> CommandResult<Sub2ApiBillingPayload> {
    let profile_name = if profile.name.trim().is_empty() {
        "未命名供应商"
    } else {
        profile.name.trim()
    };
    match chatgptx_core::sub2api::fetch_sub2api_billing_info(&profile).await {
        Ok(info) => ok(
            &format!(
                "已从「{profile_name}」获取倍率：{}x。",
                format_multiplier(info.effective_rate_multiplier)
            ),
            Sub2ApiBillingPayload {
                endpoint: info.endpoint,
                group_rate_multiplier: info.group_rate_multiplier,
                user_rate_multiplier: info.user_rate_multiplier,
                resolved_rate_multiplier: info.resolved_rate_multiplier,
                peak_rate_enabled: info.peak_rate_enabled,
                peak_rate_multiplier: info.peak_rate_multiplier,
                applied_peak_multiplier: info.applied_peak_multiplier,
                effective_rate_multiplier: info.effective_rate_multiplier,
                observed_at: info.observed_at,
            },
        ),
        Err(error) => failed(
            &format!("从「{profile_name}」获取 sub2api 倍率失败：{error}"),
            Sub2ApiBillingPayload {
                endpoint: chatgptx_core::sub2api::sub2api_billing_endpoint(
                    if profile.upstream_base_url.trim().is_empty() {
                        profile.base_url.trim()
                    } else {
                        profile.upstream_base_url.trim()
                    },
                ),
                group_rate_multiplier: 0.0,
                user_rate_multiplier: None,
                resolved_rate_multiplier: 0.0,
                peak_rate_enabled: false,
                peak_rate_multiplier: None,
                applied_peak_multiplier: None,
                effective_rate_multiplier: 0.0,
                observed_at: String::new(),
            },
        ),
    }
}

fn format_multiplier(value: f64) -> String {
    let mut text = format!("{value:.4}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[tauri::command]
pub async fn diagnose_relay_profile(profile: RelayProfile) -> CommandResult<ProviderDoctorPayload> {
    let profile_name = if profile.name.trim().is_empty() {
        "未命名供应商".to_string()
    } else {
        profile.name.trim().to_string()
    };
    let settings = SettingsStore::default().load().unwrap_or_default();
    let test_model = if !profile.test_model.trim().is_empty() {
        profile.test_model.trim().to_string()
    } else {
        let from_profile = chatgptx_core::relay_config::relay_profile_model(&profile);
        if from_profile.trim().is_empty() {
            settings.relay_test_model.trim().to_string()
        } else {
            from_profile
        }
    };
    let mut checks = Vec::new();

    if profile.relay_mode == chatgptx_core::settings::RelayMode::Official
        && !profile.official_mix_api_key
    {
        checks.push(ProviderDoctorCheck {
            id: "config".to_string(),
            title: "配置完整性".to_string(),
            status: "ok".to_string(),
            detail: "官方登录供应商不需要 Base URL / API Key。".to_string(),
        });
        let payload = ProviderDoctorPayload {
            profile_name,
            model: test_model,
            summary: "官方登录供应商无需 API 诊断。".to_string(),
            recommendation: "如果 Codex 官方账号可用，直接使用官方登录模式即可。".to_string(),
            checks,
        };
        return ok("Provider Doctor：官方登录供应商无需 API 诊断。", payload);
    }

    if chatgptx_core::relay_config::relay_profile_base_url(&profile)
        .trim()
        .is_empty()
        || (!profile.uses_no_auth()
            && chatgptx_core::relay_config::relay_profile_api_key(&profile)
                .trim()
                .is_empty())
    {
        checks.push(ProviderDoctorCheck {
            id: "config".to_string(),
            title: "配置完整性".to_string(),
            status: "failed".to_string(),
            detail: "Base URL 为空，或需要认证但 API Key 为空。".to_string(),
        });
        let payload = ProviderDoctorPayload {
            profile_name,
            model: test_model,
            summary: "配置不完整，无法发起上游诊断。".to_string(),
            recommendation:
                "先填写 Base URL，并填写 API Key 或为可信上游开启无需认证；如果是官方账号，请切换到官方登录模式。"
                    .to_string(),
            checks,
        };
        return failed("Provider Doctor：配置不完整。", payload);
    }

    checks.push(ProviderDoctorCheck {
        id: "config".to_string(),
        title: "配置完整性".to_string(),
        status: "ok".to_string(),
        detail: format!(
            "{} / {}",
            chatgptx_core::relay_config::relay_profile_base_url(&profile),
            match profile.protocol {
                chatgptx_core::settings::RelayProtocol::Responses => "Responses API",
                chatgptx_core::settings::RelayProtocol::ChatCompletions => "Chat Completions",
            }
        ),
    });

    match chatgptx_core::model_catalog::fetch_relay_profile_model_ids(&profile).await {
        Ok((models, endpoint)) => {
            let contains_model = !test_model.trim().is_empty()
                && models.iter().any(|model| model == test_model.trim());
            let status = if models.is_empty() {
                "failed"
            } else if contains_model || test_model.trim().is_empty() {
                "ok"
            } else {
                "warning"
            };
            let detail = if models.is_empty() {
                format!("{endpoint} 返回 0 个模型。")
            } else if contains_model || test_model.trim().is_empty() {
                format!("{endpoint} 返回 {} 个模型。", models.len())
            } else {
                format!(
                    "{endpoint} 返回 {} 个模型，但未看到测试模型「{}」。",
                    models.len(),
                    test_model
                )
            };
            checks.push(ProviderDoctorCheck {
                id: "models".to_string(),
                title: "模型列表".to_string(),
                status: status.to_string(),
                detail,
            });
        }
        Err(error) => checks.push(ProviderDoctorCheck {
            id: "models".to_string(),
            title: "模型列表".to_string(),
            status: "failed".to_string(),
            detail: error.to_string(),
        }),
    }

    match chatgptx_core::relay_config::test_relay_profile(&profile, &test_model).await {
        Ok(result) => {
            let status = if result.http_status < 400 {
                "ok"
            } else {
                "failed"
            };
            let preview = result.response_preview.trim();
            checks.push(ProviderDoctorCheck {
                id: "request".to_string(),
                title: "真实请求".to_string(),
                status: status.to_string(),
                detail: if preview.is_empty() {
                    format!(
                        "{} 返回 HTTP {}，响应内容为空。",
                        result.endpoint, result.http_status
                    )
                } else {
                    format!(
                        "{} 返回 HTTP {}：{}",
                        result.endpoint, result.http_status, preview
                    )
                },
            });
        }
        Err(error) => checks.push(ProviderDoctorCheck {
            id: "request".to_string(),
            title: "真实请求".to_string(),
            status: "failed".to_string(),
            detail: error.to_string(),
        }),
    }

    let failed_count = checks
        .iter()
        .filter(|check| check.status == "failed")
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check.status == "warning")
        .count();
    let status = if failed_count > 0 {
        "failed"
    } else if warning_count > 0 {
        "ok"
    } else {
        "ok"
    };
    let summary = if failed_count > 0 {
        format!("发现 {failed_count} 项失败，Codex 可能无法使用该供应商。")
    } else if warning_count > 0 {
        format!("基础连接可用，但有 {warning_count} 项需要确认。")
    } else {
        "供应商基础诊断通过。".to_string()
    };
    let recommendation = provider_doctor_recommendation(&checks);
    let message = format!("Provider Doctor：{summary}");
    CommandResult {
        status: status.to_string(),
        message,
        payload: ProviderDoctorPayload {
            profile_name,
            model: test_model,
            summary,
            recommendation,
            checks,
        },
    }
}

fn provider_doctor_recommendation(checks: &[ProviderDoctorCheck]) -> String {
    if checks
        .iter()
        .any(|check| check.id == "config" && check.status == "failed")
    {
        return "先补齐 Base URL 和 API Key；如果使用官方账号，请切换到官方登录模式。".to_string();
    }
    if checks
        .iter()
        .any(|check| check.id == "models" && check.status == "failed")
    {
        return "优先检查 Base URL 是否包含正确的 /v1 前缀，以及供应商是否支持 /v1/models。"
            .to_string();
    }
    if checks
        .iter()
        .any(|check| check.id == "request" && check.status == "failed")
    {
        return "优先检查测试模型名称、上游协议选择和 Key 权限；如果 Chat Completions 可用，请切到对应协议。".to_string();
    }
    if checks.iter().any(|check| check.status == "warning") {
        return "连接可用，但测试模型没有出现在模型列表里；建议改用上游返回的模型名。".to_string();
    }
    "可以作为 Codex 供应商使用；如果真实对话仍失败，请查看协议代理日志里的上游响应。".to_string()
}

#[tauri::command]
pub fn apply_relay_injection() -> CommandResult<RelayPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let Ok(_guard) = relay_switch_mutex().lock() else {
        let status = chatgptx_core::relay_config::relay_status_from_home(&home);
        return failed(
            "供应商切换锁已损坏，请重启管理器后再试。",
            relay_payload(status, None),
        );
    };
    let settings = SettingsStore::default().load().unwrap_or_default();
    if !settings.relay_profiles_enabled {
        let status = chatgptx_core::relay_config::relay_status_from_home(&home);
        return failed(
            "供应商配置总开关已关闭，未写入 config.toml / auth.json。",
            relay_payload(status, None),
        );
    }
    prepare_codex_app_state_before_provider_switch(&home, "manager.apply_relay_injection.before");
    let relay = settings.active_relay_profile();
    log_relay_apply_request("manager.apply_relay_injection", &settings, &relay);
    if let Some(aggregate) = settings.active_aggregate_relay_profile() {
        let response = apply_aggregate_relay_injection_to_home(&home, aggregate.session_provider);
        if response.status == "ok" {
            finish_codex_app_state_after_provider_switch(
                &home,
                "manager.apply_relay_injection.aggregate",
            );
        }
        return response;
    }
    if relay_has_complete_files(&relay) {
        return match chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules(
            &home,
            &relay,
            &relay_combined_common_config(&settings),
        ) {
            Ok(result) => {
                finish_codex_app_state_after_provider_switch(
                    &home,
                    "manager.apply_relay_injection.profile",
                );
                let status = chatgptx_core::relay_config::relay_status_from_home(&home);
                log_relay_apply_result(
                    "manager.apply_relay_injection.ok",
                    &relay,
                    &status,
                    result.backup_path.as_ref(),
                    None,
                );
                ok(
                    "已按兼容切换规则切换供应商。",
                    relay_payload(status, result.backup_path),
                )
            }
            Err(error) => {
                let status = chatgptx_core::relay_config::relay_status_from_home(&home);
                log_relay_apply_result(
                    "manager.apply_relay_injection.failed",
                    &relay,
                    &status,
                    None,
                    Some(error.to_string()),
                );
                failed(
                    &format!("切换完整中转配置失败：{error}"),
                    relay_payload(status, None),
                )
            }
        };
    }

    let auth = chatgptx_core::relay_config::chatgpt_auth_status_from_home(&home);
    if !auth.authenticated {
        let status = chatgptx_core::relay_config::relay_status_from_home(&home);
        log_relay_apply_result(
            "manager.apply_relay_injection.failed",
            &relay,
            &status,
            None,
            Some("未检测到 ChatGPT 登录状态".to_string()),
        );
        return failed(
            "未检测到 ChatGPT 登录状态，已停止写入中转配置。",
            relay_payload(status, None),
        );
    }

    match chatgptx_core::relay_config::apply_relay_config_to_home_with_session_provider(
        &home,
        &relay.base_url,
        &relay.api_key,
        relay.protocol,
        chatgptx_core::protocol_proxy::protocol_proxy_port(),
        chatgptx_core::relay_config::relay_session_provider_from_config(&relay.config_contents),
    ) {
        Ok(result) => {
            finish_codex_app_state_after_provider_switch(
                &home,
                "manager.apply_relay_injection.generated",
            );
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_relay_apply_result(
                "manager.apply_relay_injection.ok",
                &relay,
                &status,
                result.backup_path.as_ref(),
                None,
            );
            ok(
                "中转配置已写入，密钥未在界面明文显示。",
                relay_payload(status, result.backup_path),
            )
        }
        Err(error) => {
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_relay_apply_result(
                "manager.apply_relay_injection.failed",
                &relay,
                &status,
                None,
                Some(error.to_string()),
            );
            failed(
                &format!("写入中转配置失败：{error}"),
                relay_payload(status, None),
            )
        }
    }
}

fn apply_aggregate_relay_injection_to_home(
    home: &Path,
    session_provider: RelaySessionProvider,
) -> CommandResult<RelayPayload> {
    // 模型聚合需要和普通供应商切换共用 catalog 写盘链路；旧的直接代理
    // 写入路径保留给未开启模型聚合的旧配置和无 settings 测试调用。
    if let Ok(settings) = SettingsStore::default().load() {
        let profile = settings.active_relay_profile();
        if profile.relay_mode == RelayMode::Aggregate
            && settings
                .active_aggregate_relay_profile()
                .is_some_and(|aggregate| aggregate.model_aggregation_enabled)
        {
            let common_config = relay_combined_common_config(&settings);
            match chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules(
                home,
                &profile,
                &common_config,
            ) {
                Ok(result) => {
                    let status = chatgptx_core::relay_config::relay_status_from_home(home);
                    let mut message =
                        "聚合供应商配置和模型目录已写入，选中的模型会固定发送到对应成员。"
                            .to_string();
                    if let Some(warning) =
                        chatgptx_core::aggregate_catalog::subagent_conflict_warning(
                            home, &settings,
                        )
                    {
                        message.push_str("\n\n");
                        message.push_str(&warning);
                    }
                    return ok(&message, relay_payload(status, result.backup_path));
                }
                Err(error) => {
                    let status = chatgptx_core::relay_config::relay_status_from_home(home);
                    return failed(
                        &format!("写入聚合供应商配置失败：{error}"),
                        relay_payload(status, None),
                    );
                }
            }
        }
    }
    match chatgptx_core::relay_config::apply_relay_config_to_home_with_session_provider(
        home,
        &chatgptx_core::protocol_proxy::local_responses_proxy_base_url(
            chatgptx_core::protocol_proxy::protocol_proxy_port(),
        ),
        "chatgptx-aggregate",
        chatgptx_core::settings::RelayProtocol::Responses,
        chatgptx_core::protocol_proxy::protocol_proxy_port(),
        session_provider,
    ) {
        Ok(result) => {
            let status = chatgptx_core::relay_config::relay_status_from_home(home);
            ok(
                "聚合供应商配置已写入，真实请求会由本地代理按策略轮转。",
                relay_payload(status, result.backup_path),
            )
        }
        Err(error) => {
            let status = chatgptx_core::relay_config::relay_status_from_home(home);
            failed(
                &format!("写入聚合供应商配置失败：{error}"),
                relay_payload(status, None),
            )
        }
    }
}

#[tauri::command]
pub fn apply_pure_api_injection() -> CommandResult<RelayPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let Ok(_guard) = relay_switch_mutex().lock() else {
        let status = chatgptx_core::relay_config::relay_status_from_home(&home);
        return failed(
            "供应商切换锁已损坏，请重启管理器后再试。",
            relay_payload(status, None),
        );
    };
    let settings = SettingsStore::default().load().unwrap_or_default();
    if !settings.relay_profiles_enabled {
        let status = chatgptx_core::relay_config::relay_status_from_home(&home);
        return failed(
            "供应商配置总开关已关闭，未写入 config.toml / auth.json。",
            relay_payload(status, None),
        );
    }
    prepare_codex_app_state_before_provider_switch(
        &home,
        "manager.apply_pure_api_injection.before",
    );
    let relay = settings.active_relay_profile();
    log_relay_apply_request("manager.apply_pure_api_injection", &settings, &relay);
    if relay_has_complete_files(&relay) {
        return match chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules(
            &home,
            &relay,
            &relay_combined_common_config(&settings),
        ) {
            Ok(result) => {
                finish_codex_app_state_after_provider_switch(
                    &home,
                    "manager.apply_pure_api_injection.profile",
                );
                let status = chatgptx_core::relay_config::relay_status_from_home(&home);
                log_relay_apply_result(
                    "manager.apply_pure_api_injection.ok",
                    &relay,
                    &status,
                    result.backup_path.as_ref(),
                    None,
                );
                if !status.configured {
                    return failed(
                        "纯 API 配置写入后未检测到完整 custom provider，请检查 config.toml 和供应商 API Key。",
                        relay_payload(status, result.backup_path),
                    );
                }
                ok(
                    "已按兼容切换规则切换供应商。",
                    relay_payload(status, result.backup_path),
                )
            }
            Err(error) => {
                let status = chatgptx_core::relay_config::relay_status_from_home(&home);
                log_relay_apply_result(
                    "manager.apply_pure_api_injection.failed",
                    &relay,
                    &status,
                    None,
                    Some(error.to_string()),
                );
                failed(
                    &format!("切换纯 API 配置失败：{error}"),
                    relay_payload(status, None),
                )
            }
        };
    }

    match chatgptx_core::relay_config::apply_pure_api_config_to_home_with_session_provider(
        &home,
        &relay.base_url,
        &relay.api_key,
        relay.protocol,
        chatgptx_core::protocol_proxy::protocol_proxy_port(),
        chatgptx_core::relay_config::relay_session_provider_from_config(&relay.config_contents),
    ) {
        Ok(result) => {
            finish_codex_app_state_after_provider_switch(
                &home,
                "manager.apply_pure_api_injection.generated",
            );
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_relay_apply_result(
                "manager.apply_pure_api_injection.ok",
                &relay,
                &status,
                result.backup_path.as_ref(),
                None,
            );
            if !status.configured {
                return failed(
                    "纯 API 配置写入后未检测到完整 custom provider，请检查 config.toml 和供应商 API Key。",
                    relay_payload(status, result.backup_path),
                );
            }
            ok(
                "纯 API 模式已写入：config.toml 已写入 custom provider，auth.json 已切换为当前供应商。",
                relay_payload(status, result.backup_path),
            )
        }
        Err(error) => {
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_relay_apply_result(
                "manager.apply_pure_api_injection.failed",
                &relay,
                &status,
                None,
                Some(error.to_string()),
            );
            failed(
                &format!("写入纯 API 模式失败：{error}"),
                relay_payload(status, None),
            )
        }
    }
}

#[tauri::command]
pub fn clear_relay_injection() -> CommandResult<RelayPayload> {
    let home = chatgptx_core::relay_config::default_codex_home_dir();
    let Ok(_guard) = relay_switch_mutex().lock() else {
        let status = chatgptx_core::relay_config::relay_status_from_home(&home);
        return failed(
            "供应商切换锁已损坏，请重启管理器后再试。",
            relay_payload(status, None),
        );
    };
    let settings = SettingsStore::default().load().unwrap_or_default();
    let relay = settings.active_relay_profile();
    log_manager_event("manager.clear_relay_injection.start", json!({}));
    prepare_codex_app_state_before_provider_switch(&home, "manager.clear_relay_injection.before");
    let auth_contents = (relay.relay_mode == chatgptx_core::settings::RelayMode::Official
        && !relay.official_mix_api_key
        && !relay.auth_contents.trim().is_empty())
    .then_some(relay.auth_contents.as_str());
    match chatgptx_core::relay_config::clear_relay_config_to_home_with_auth(&home, auth_contents)
    {
        Ok(result) => {
            finish_codex_app_state_after_provider_switch(
                &home,
                "manager.clear_relay_injection.after",
            );
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_manager_event(
                "manager.clear_relay_injection.ok",
                json!({
                    "configured": status.configured,
                    "backupPath": result.backup_path.as_ref()
                }),
            );
            ok(
                "已清除 custom 中转 API 模式，并切换到官方 ChatGPT 登录模式。",
                relay_payload(status, result.backup_path),
            )
        }
        Err(error) => {
            let status = chatgptx_core::relay_config::relay_status_from_home(&home);
            log_manager_event(
                "manager.clear_relay_injection.failed",
                json!({
                    "configured": status.configured,
                    "error": error.to_string()
                }),
            );
            failed(
                &format!("清除中转配置失败：{error}"),
                relay_payload(status, None),
            )
        }
    }
}

fn prepare_codex_app_state_before_provider_switch(home: &Path, source: &str) {
    chatgptx_core::codex_app_state::capture_app_state_snapshot_nonfatal(home, source);
}

fn finish_codex_app_state_after_provider_switch(home: &Path, source: &str) {
    chatgptx_core::codex_app_state::sync_app_state_after_provider_switch_nonfatal(home, source);
}

fn relay_has_complete_files(relay: &chatgptx_core::settings::RelayProfile) -> bool {
    if relay.relay_mode == chatgptx_core::settings::RelayMode::Official
        && relay.official_mix_api_key
    {
        return !relay.config_contents.trim().is_empty();
    }
    !relay.config_contents.trim().is_empty() && !relay.auth_contents.trim().is_empty()
}

fn log_relay_apply_request(
    event: &str,
    settings: &BackendSettings,
    relay: &chatgptx_core::settings::RelayProfile,
) {
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(
        event,
        json!({
            "activeRelayId": settings.active_relay_id,
            "relayId": relay.id,
            "relayName": relay.name,
            "relayMode": relay.relay_mode,
            "protocol": relay.protocol,
            "baseUrl": relay.base_url,
            "hasConfigContents": !relay.config_contents.trim().is_empty(),
            "hasAuthContents": !relay.auth_contents.trim().is_empty(),
            "configContainsProxy": relay.config_contents.contains("127.0.0.1:57321")
        }),
    );
}

fn log_relay_apply_result(
    event: &str,
    relay: &chatgptx_core::settings::RelayProfile,
    status: &chatgptx_core::relay_config::RelayStatus,
    backup_path: Option<&String>,
    error: Option<String>,
) {
    log_manager_event(
        event,
        json!({
            "relayId": relay.id,
            "relayName": relay.name,
            "relayMode": relay.relay_mode,
            "protocol": relay.protocol,
            "configured": status.configured,
            "requiresOpenaiAuth": status.requires_openai_auth,
            "hasBearerToken": status.has_bearer_token,
            "backupPath": backup_path,
            "error": error
        }),
    );
}

fn log_manager_event(event: &str, detail: Value) {
    let _ = chatgptx_core::diagnostic_log::append_diagnostic_log(event, detail);
}

fn sanitize_manager_event(event: &str) -> String {
    let suffix = event
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let suffix = suffix.trim_matches(['.', '_', '-']).trim();
    if suffix.is_empty() {
        "manager.ui.event".to_string()
    } else if suffix.starts_with("manager.") {
        suffix.to_string()
    } else {
        format!("manager.ui.{suffix}")
    }
}

fn relay_payload(
    status: chatgptx_core::relay_config::RelayStatus,
    backup_path: Option<String>,
) -> RelayPayload {
    RelayPayload {
        authenticated: status.authenticated,
        auth_source: status.auth_source,
        account_label: status.account_label,
        config_path: status.config_path,
        configured: status.configured,
        requires_openai_auth: status.requires_openai_auth,
        has_bearer_token: status.has_bearer_token,
        backup_path,
    }
}

fn relay_switch_payload(
    settings: BackendSettings,
    status: chatgptx_core::relay_config::RelayStatus,
    backup_path: Option<String>,
) -> RelaySwitchPayload {
    RelaySwitchPayload {
        settings,
        relay: relay_payload(status, backup_path),
        settings_path: chatgptx_core::paths::default_settings_path()
            .to_string_lossy()
            .to_string(),
        user_scripts: user_script_inventory(),
    }
}

fn relay_switch_mutex() -> &'static Mutex<()> {
    static RELAY_SWITCH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    RELAY_SWITCH_LOCK.get_or_init(|| Mutex::new(()))
}

fn empty_context_entries() -> chatgptx_core::relay_config::CodexContextEntries {
    chatgptx_core::relay_config::CodexContextEntries {
        mcp_servers: Vec::new(),
        skills: Vec::new(),
        plugins: Vec::new(),
    }
}

fn relay_files_payload_from_home(home: &std::path::Path) -> anyhow::Result<RelayFilesPayload> {
    let config_path = home.join("config.toml");
    let auth_path = home.join("auth.json");
    Ok(RelayFilesPayload {
        config_path: config_path.to_string_lossy().to_string(),
        auth_path: auth_path.to_string_lossy().to_string(),
        config_contents: read_optional_text_file(&config_path)?,
        auth_contents: read_optional_text_file(&auth_path)?,
    })
}

fn save_relay_file_in_home(
    home: &std::path::Path,
    kind: &str,
    contents: &str,
) -> anyhow::Result<()> {
    let path = match kind {
        "config" => home.join("config.toml"),
        "auth" => home.join("auth.json"),
        other => anyhow::bail!("未知配置文件类型：{other}"),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

fn prepare_relay_file_contents(
    kind: &str,
    contents: &str,
    profile: &RelayProfile,
) -> anyhow::Result<String> {
    if kind == "config" {
        return chatgptx_core::relay_config::apply_deepseek_responses_compatibility(
            profile, contents,
        );
    }
    Ok(contents.to_string())
}

fn read_optional_text_file(path: &std::path::Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}


fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        chatgptx_core::windows_open_url(url)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("启动系统浏览器失败：{error}"))
    }
}

fn settings_payload(message: &str, failure_context: &str) -> CommandResult<SettingsPayload> {
    match settings_payload_value() {
        Ok(payload) => ok(message, payload),
        Err((error, payload)) => failed(&format!("{failure_context}：{error}"), payload),
    }
}

fn settings_payload_value() -> Result<SettingsPayload, (anyhow::Error, SettingsPayload)> {
    let store = SettingsStore::default();
    let settings_path = chatgptx_core::paths::default_settings_path()
        .to_string_lossy()
        .to_string();
    match store.load() {
        Ok(settings) => Ok(SettingsPayload {
            settings,
            settings_path,
            user_scripts: user_script_inventory(),
        }),
        Err(error) => Err((
            error,
            SettingsPayload {
                settings: BackendSettings::default(),
                settings_path,
                user_scripts: user_script_inventory(),
            },
        )),
    }
}

fn fallback_settings_payload() -> SettingsPayload {
    SettingsPayload {
        settings: SettingsStore::default().load().unwrap_or_default(),
        settings_path: chatgptx_core::paths::default_settings_path()
            .to_string_lossy()
            .to_string(),
        user_scripts: user_script_inventory(),
    }
}

fn user_script_inventory() -> Value {
    default_user_script_manager()
        .inventory()
        .unwrap_or_else(|error| {
            json!({
                "enabled": true,
                "scripts": [],
                "error": error.to_string()
            })
        })
}

fn failed_script_market_payload(message: &str) -> ScriptMarketPayload {
    ScriptMarketPayload {
        market: json!({
            "status": "failed",
            "message": message,
            "indexUrl": script_market::DEFAULT_MARKET_INDEX_URL,
            "updatedAt": "",
            "scripts": []
        }),
        user_scripts: user_script_inventory(),
    }
}

fn script_market_payload_from_manifest(
    manifest: &ScriptMarketManifest,
    status: &str,
    message: &str,
) -> ScriptMarketPayload {
    let user_scripts = user_script_inventory();
    let installed = installed_market_versions(&user_scripts);
    let scripts = manifest
        .scripts
        .iter()
        .map(|script| market_script_payload(script, &installed))
        .collect::<Vec<_>>();
    ScriptMarketPayload {
        market: json!({
            "status": status,
            "message": message,
            "indexUrl": script_market::DEFAULT_MARKET_INDEX_URL,
            "updatedAt": manifest.updated_at.clone().unwrap_or_default(),
            "scripts": scripts
        }),
        user_scripts,
    }
}

fn installed_market_versions(user_scripts: &Value) -> BTreeMap<String, String> {
    user_scripts
        .get("scripts")
        .and_then(Value::as_array)
        .map(|scripts| {
            scripts
                .iter()
                .filter_map(|script| {
                    let id = script.get("market_id").and_then(Value::as_str)?;
                    if id.is_empty() {
                        return None;
                    }
                    let version = script
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    Some((id.to_string(), version))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn market_script_payload(script: &MarketScript, installed: &BTreeMap<String, String>) -> Value {
    let installed_version = installed.get(&script.id).cloned().unwrap_or_default();
    let is_installed = !installed_version.is_empty();
    json!({
        "id": script.id,
        "name": script.name,
        "description": script.description,
        "version": script.version,
        "author": script.author,
        "tags": script.tags,
        "homepage": script.homepage,
        "script_url": script.script_url,
        "installed": is_installed,
        "installedVersion": installed_version,
        "updateAvailable": is_installed && installed.get(&script.id).map(|version| version != &script.version).unwrap_or(false)
    })
}

fn default_user_script_manager() -> UserScriptManager {
    let config_dir = user_scripts_config_dir();
    UserScriptManager::new(
        builtin_user_scripts_dir(),
        config_dir.join("user_scripts"),
        config_dir.join("user_scripts.json"),
    )
}

fn user_scripts_config_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(roaming) = std::env::var_os("APPDATA") {
            return PathBuf::from(roaming).join("ChatGPTX");
        }
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("ChatGPTX")
}

fn builtin_user_scripts_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|path| path.join("user_scripts"))
        .unwrap_or_else(|| PathBuf::from("user_scripts"))
}

fn diagnostics_report() -> String {
    let (codex_app_path, entrypoints, latest_launch, runtime_health) = load_overview_payload();
    let overview = ok(
        "概览已加载。",
        OverviewPayload {
            codex_version: codex_app_path
                .as_deref()
                .and_then(chatgptx_core::app_paths::codex_app_version),
            codex_app: path_state(codex_app_path),
            silent_shortcut: shortcut_state(entrypoints.silent_shortcut),
            management_shortcut: shortcut_state(entrypoints.management_shortcut),
            latest_launch,
            runtime_health,
            current_version: chatgptx_core::version::VERSION.to_string(),
            update_status: "not_checked".to_string(),
            settings_path: chatgptx_core::paths::default_settings_path()
                .to_string_lossy()
                .to_string(),
            logs_path: chatgptx_core::paths::default_diagnostic_log_path()
                .to_string_lossy()
                .to_string(),
        },
    );
    let settings = SettingsStore::default().load().unwrap_or_default();
    let generated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    serde_json::to_string_pretty(&json!({
        "generatedAtMs": generated_at_ms,
        "version": chatgptx_core::version::VERSION,
        "overview": overview.payload,
        "settings": settings,
        "logs": {
            "diagnosticLogPath": chatgptx_core::paths::default_diagnostic_log_path(),
            "latestStatusPath": chatgptx_core::paths::default_latest_status_path()
        },
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH
        }
    }))
    .unwrap_or_else(|error| format!("诊断报告序列化失败：{error}"))
}

fn load_overview_payload() -> (
    Option<PathBuf>,
    install::EntryPointState,
    Option<LaunchStatus>,
    crate::runtime_health::RuntimeHealth,
) {
    let settings = SettingsStore::default().load().unwrap_or_default();
    let latest = StatusStore::default().load_latest().unwrap_or(None);
    let runtime_health = crate::runtime_health::inspect(&settings, latest.as_ref());
    (
        chatgptx_core::app_paths::resolve_codex_app_dir_with_saved(
            None,
            Some(settings.codex_app_path.as_str()),
        ),
        install::inspect_entrypoints(),
        latest,
        runtime_health,
    )
}

fn install_background_failure(action: &str, error: impl std::fmt::Display) -> InstallActionResult {
    let state = install::inspect_entrypoints();
    InstallActionResult {
        status: "failed".to_string(),
        message: format!("{action}后台任务失败：{error}"),
        silent_shortcut: state.silent_shortcut,
        management_shortcut: state.management_shortcut,
    }
}

fn watcher_payload() -> WatcherPayload {
    let flag = chatgptx_core::watcher::default_watcher_disabled_flag();
    WatcherPayload {
        enabled: chatgptx_core::watcher::watcher_is_installed() && !flag.exists(),
        disabled_flag: flag.to_string_lossy().to_string(),
    }
}

const MAX_LOG_TAIL_READ_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
struct TailRead {
    text: String,
    truncated: bool,
    file_size: u64,
}

fn read_tail(path: &Path, max_lines: usize) -> std::io::Result<TailRead> {
    let mut file = fs::File::open(path)?;
    let file_size = file.metadata()?.len();
    if max_lines == 0 || file_size == 0 {
        return Ok(TailRead {
            text: String::new(),
            truncated: false,
            file_size,
        });
    }

    let read_len = MAX_LOG_TAIL_READ_BYTES.min(file_size);
    let start = file_size - read_len;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(read_len as usize);
    file.read_to_end(&mut bytes)?;
    let truncated = start > 0;
    if truncated {
        if let Some(pos) = bytes.iter().position(|byte| *byte == b'\n') {
            bytes.drain(..=pos);
        }
    }

    let contents = String::from_utf8_lossy(&bytes);
    let mut lines = contents.lines().rev().take(max_lines).collect::<Vec<_>>();
    lines.reverse();
    Ok(TailRead {
        text: lines.join("\n"),
        truncated,
        file_size,
    })
}

fn path_state(path: Option<PathBuf>) -> PathState {
    match path {
        Some(path) => PathState {
            status: "found".to_string(),
            path: Some(path.to_string_lossy().to_string()),
        },
        None => PathState {
            status: "missing".to_string(),
            path: None,
        },
    }
}

fn shortcut_state(shortcut: install::ShortcutState) -> PathState {
    PathState {
        status: if shortcut.installed {
            "installed".to_string()
        } else {
            "missing".to_string()
        },
        path: shortcut.path,
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestVlmRequest {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub image_data_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestVlmResult {
    pub vlm_status: String,
    pub http_code: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub description: Option<String>,
    pub model: String,
    pub raw_request: Option<String>,
    pub raw_response: Option<String>,
}

/// 用表单当前 VLM 配置 + 用户上传图片（data URL）测试 VLM 可用性。
/// 用表单当前值（未保存亦可）；失败也返回结构化 payload 供前端渲染诊断。
#[tauri::command]
pub async fn test_vlm(request: TestVlmRequest) -> CommandResult<TestVlmResult> {
    // 加固 spec §4.1 第二道门：前端校验可被绕过（IPC 直调），类型与大小在
    // 信任边界重新校验；非法输入不发起网络请求。
    if let Err(reason) = chatgptx_core::vision::validate_image_data_url(&request.image_data_url) {
        return failed(
            "VLM 测试失败：invalid_image",
            TestVlmResult {
                vlm_status: "invalid_image".to_string(),
                http_code: None,
                duration_ms: 0,
                error: Some(reason),
                description: None,
                model: request.model,
                raw_request: None,
                raw_response: None,
            },
        );
    }
    let config = chatgptx_core::vision::VlmConfig {
        api_key: request.api_key,
        model: request.model.clone(),
        base_url: request.base_url,
    };
    let client = match chatgptx_core::http_client::vlm_http_client() {
        Ok(c) => c,
        Err(e) => {
            return failed(
                &format!("VLM HTTP 客户端构建失败：{e}"),
                TestVlmResult {
                    vlm_status: "client_error".to_string(),
                    http_code: None,
                    duration_ms: 0,
                    // 加固 spec §4.2：client_error 路径同样过脱敏，全仓库一条规则
                    error: Some(chatgptx_core::vision::redact_secrets(
                        &e.to_string(),
                        &config.api_key,
                    )),
                    description: None,
                    model: request.model,
                    raw_request: None,
                    raw_response: None,
                },
            );
        }
    };
    let outcome =
        chatgptx_core::vision::test_vlm_once(&config, &request.image_data_url, &client).await;
    let result = TestVlmResult {
        vlm_status: outcome.status.clone(),
        http_code: outcome.http_code,
        duration_ms: outcome.duration_ms,
        error: outcome.error,
        description: outcome.text,
        model: request.model,
        raw_request: outcome.raw_request,
        raw_response: outcome.raw_response,
    };
    if outcome.status == "ok" {
        ok("VLM 测试成功。", result)
    } else {
        failed(&format!("VLM 测试失败：{}", outcome.status), result)
    }
}

fn ok<T: Serialize>(message: &str, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "ok".to_string(),
        message: message.to_string(),
        payload,
    }
}

fn failed<T: Serialize>(message: &str, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "failed".to_string(),
        message: message.to_string(),
        payload,
    }
}

/// 「功能降级但流程可继续」：与 `failed` 区分开，让调用方能继续走后面的步骤，
/// 同时仍然把原因提示给用户（issue #618 的回填跳过即属此列）。
fn degraded<T: Serialize>(message: &str, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "degraded".to_string(),
        message: message.to_string(),
        payload,
    }
}

// Hold ownership across stop and config sync, then release before the successor starts.
fn ensure_provider_sync_is_idle_before_stop()
-> Result<chatgptx_data::ProviderSyncLifecycleGuard, String> {
    chatgptx_data::try_acquire_provider_sync_lifecycle_guard(None)
        .map_err(|error| format!("历史会话同步正在进行或锁不可用，未执行重启；请稍后重试：{error}"))
}

fn default_debug_port() -> u16 {
    9229
}

fn default_helper_port() -> u16 {
    57321
}

fn default_log_lines() -> usize {
    200
}

// ============================================================================
// 多官方账号登录、管理、切换、第三方 API 绑定与 Codex 启动代理注入命令
// ============================================================================

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteOfficialOAuthRequest {
    #[serde(default)]
    pub login_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true_bool")]
    pub set_active: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitOfficialOAuthCallbackRequest {
    pub callback_url: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true_bool")]
    pub set_active: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOfficialAccountsRequest {
    pub input: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOfficialAccountRequest {
    pub account_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub bound_relay_profile_id: Option<String>,
    #[serde(default)]
    pub proxy_mode: Option<String>,
    #[serde(default)]
    pub egress_proxy_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindRelayOfficialAccountRequest {
    pub relay_profile_id: String,
    #[serde(default)]
    pub official_account_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRelayLaunchProxyRequest {
    pub relay_profile_id: String,
    #[serde(default)]
    pub proxy_spec: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveGlobalLaunchProxyRequest {
    pub global_proxy_mode: String,
    #[serde(default)]
    pub global_proxy_url: String,
    #[serde(default)]
    pub proxy_resources: Option<Vec<chatgptx_core::official_accounts::OfficialProxyResource>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOfficialAccountPayload {
    pub accounts: chatgptx_core::official_accounts::OfficialAccountsSnapshot,
    pub settings: BackendSettings,
}

fn default_true_bool() -> bool {
    true
}

#[tauri::command]
pub fn load_official_accounts()
-> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    let snapshot = chatgptx_core::official_accounts::get_official_accounts_snapshot();
    ok("已加载官方账号与启动代理配置。", snapshot)
}

#[tauri::command]
pub async fn start_official_oauth_login()
-> CommandResult<Option<chatgptx_core::official_accounts::OAuthLoginStartResponse>> {
    match chatgptx_core::official_accounts::start_official_oauth_login().await {
        Ok(resp) => ok("已启动本地 OAuth 授权监听。", Some(resp)),
        Err(err) => failed(&format!("启动 OAuth 登录失败：{err}"), None),
    }
}

#[tauri::command]
pub async fn complete_official_oauth_login(
    request: CompleteOfficialOAuthRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::wait_for_official_oauth_callback(
        &request.login_id,
        request.label,
        request.set_active,
    )
    .await
    {
        Ok(account) => {
            let _ = chatgptx_core::official_accounts::refresh_official_account_quota(&account.id)
                .await;
            let snapshot = chatgptx_core::official_accounts::get_official_accounts_snapshot();
            ok(&format!("官方账号「{}」授权登录成功。", account.email), snapshot)
        }
        Err(err) => failed(
            &format!("OAuth 登录未完成：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub async fn submit_official_oauth_callback(
    request: SubmitOfficialOAuthCallbackRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::submit_official_oauth_callback_url(
        &request.callback_url,
        request.label,
        request.set_active,
    )
    .await
    {
        Ok(account) => {
            let _ = chatgptx_core::official_accounts::refresh_official_account_quota(&account.id)
                .await;
            let snapshot = chatgptx_core::official_accounts::get_official_accounts_snapshot();
            ok(&format!("官方账号「{}」验证并添加成功。", account.email), snapshot)
        }
        Err(err) => failed(
            &format!("解析回调或兑换 Token 失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub fn cancel_official_oauth_login() -> CommandResult<Value> {
    chatgptx_core::official_accounts::cancel_official_oauth_login();
    ok("已取消 OAuth 登录监听。", json!({}))
}

#[tauri::command]
pub async fn import_official_account_from_local(
    label: Option<String>,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    let codex_home = chatgptx_core::codex_home::default_codex_home_dir();
    match chatgptx_core::official_accounts::import_official_account_from_local_auth(
        &codex_home,
        label,
    ) {
        Ok(account) => {
            let _ = chatgptx_core::official_accounts::refresh_official_account_quota(&account.id)
                .await;
            let snapshot = chatgptx_core::official_accounts::get_official_accounts_snapshot();
            ok(
                &format!("已从本机 auth.json 导入官方账号「{}」。", account.email),
                snapshot,
            )
        }
        Err(err) => failed(
            &format!("从本机 auth.json 导入失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub async fn import_official_accounts(
    request: ImportOfficialAccountsRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::import_official_accounts_from_input(
        &request.input,
        request.label,
    )
    .await
    {
        Ok(accounts) => {
            for acc in &accounts {
                let _ =
                    chatgptx_core::official_accounts::refresh_official_account_quota(&acc.id)
                        .await;
            }
            let snapshot = chatgptx_core::official_accounts::get_official_accounts_snapshot();
            ok(&format!("成功导入 {} 个官方账号。", accounts.len()), snapshot)
        }
        Err(err) => failed(
            &format!("导入官方账号失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub fn export_official_accounts(account_ids: Option<Vec<String>>) -> CommandResult<String> {
    match chatgptx_core::official_accounts::export_official_accounts_json(account_ids.as_deref())
    {
        Ok(json_text) => ok("已导出官方账号 JSON。", json_text),
        Err(err) => failed(&format!("导出失败：{err}"), String::new()),
    }
}

#[tauri::command]
pub async fn switch_official_account(
    account_id: String,
    restart_codex: Option<bool>,
) -> CommandResult<Option<SwitchOfficialAccountPayload>> {
    let should_restart = restart_codex.unwrap_or(false);
    let codex_home = chatgptx_core::codex_home::default_codex_home_dir();
    chatgptx_core::official_accounts::sync_live_account_tokens_to_store(&codex_home);
    let _ = chatgptx_core::official_accounts::prepare_official_account_for_switch(&account_id).await;

    let debug_port = StatusStore::default()
        .load_latest()
        .ok()
        .flatten()
        .and_then(|s| s.debug_port)
        .filter(|p| *p > 0)
        .unwrap_or(9229);
    let was_running = chatgptx_core::cdp::endpoint_available(debug_port);
    if should_restart && was_running {
        let _ = tauri::async_runtime::spawn_blocking(move || stop_main_codex_blocking(debug_port)).await;
    }

    let store = SettingsStore::default();
    match chatgptx_core::official_accounts::switch_official_account(
        &account_id,
        &codex_home,
        &store,
    ) {
        Ok(settings) => {
            let accounts = chatgptx_core::official_accounts::get_official_accounts_snapshot_with(
                &codex_home,
                &settings,
            );
            if should_restart {
                let launch_req = LaunchRequest {
                    app_path: settings.codex_app_path.clone(),
                    debug_port,
                    helper_port: 57321,
                    sync_active_relay: false,
                };
                let _ = launch_chatgptx(launch_req).await;
            }
            ok(
                if should_restart {
                    "已切换官方账号并正在启动 Codex。"
                } else {
                    "已切换官方账号（重启 Codex 后生效）。"
                },
                Some(SwitchOfficialAccountPayload { accounts, settings }),
            )
        }
        Err(err) => failed(&format!("切换官方账号失败：{err}"), None),
    }
}

#[tauri::command]
pub fn delete_official_account(
    account_id: String,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::delete_official_account(&account_id) {
        Ok(()) => ok(
            "已删除官方账号。",
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
        Err(err) => failed(
            &format!("删除官方账号失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub fn update_official_account(
    request: UpdateOfficialAccountRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::update_official_account_meta(
        &request.account_id,
        request.label,
        request.bound_relay_profile_id,
        request.proxy_mode,
        request.egress_proxy_url,
    ) {
        Ok(_) => ok(
            "已更新官方账号设置。",
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
        Err(err) => failed(
            &format!("更新官方账号失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub async fn refresh_official_account_quota(
    account_id: Option<String>,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    if let Some(id) = account_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        match chatgptx_core::official_accounts::refresh_official_account_quota(&id).await {
            Ok(_) => ok(
                "已刷新账号配额。",
                chatgptx_core::official_accounts::get_official_accounts_snapshot(),
            ),
            Err(err) => failed(
                &format!("刷新配额失败：{err}"),
                chatgptx_core::official_accounts::get_official_accounts_snapshot(),
            ),
        }
    } else {
        let ids: Vec<String> = chatgptx_core::official_accounts::load_official_accounts_state()
            .accounts
            .into_iter()
            .map(|a| a.id)
            .collect();
        let mut ok_count = 0usize;
        for id in &ids {
            if chatgptx_core::official_accounts::refresh_official_account_quota(id)
                .await
                .is_ok()
            {
                ok_count += 1;
            }
        }
        let snapshot = chatgptx_core::official_accounts::get_official_accounts_snapshot();
        ok(
            &format!("已刷新全部官方账号配额（成功 {ok_count}/{}）。", ids.len()),
            snapshot,
        )
    }
}

#[tauri::command]
pub fn bind_relay_official_account(
    request: BindRelayOfficialAccountRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::bind_relay_profile_official_account(
        &request.relay_profile_id,
        request.official_account_id.as_deref(),
    ) {
        Ok(()) => ok(
            "已更新供应商与官方账号的绑定关系。",
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
        Err(err) => failed(
            &format!("绑定官方账号失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub fn set_relay_launch_proxy(
    request: SetRelayLaunchProxyRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::set_relay_profile_launch_proxy(
        &request.relay_profile_id,
        request.proxy_spec.as_deref(),
    ) {
        Ok(()) => ok(
            "已更新供应商专属启动代理设置。",
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
        Err(err) => failed(
            &format!("更新供应商代理失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub fn save_global_launch_proxy(
    request: SaveGlobalLaunchProxyRequest,
) -> CommandResult<chatgptx_core::official_accounts::OfficialAccountsSnapshot> {
    match chatgptx_core::official_accounts::update_global_launch_proxy_settings(
        &request.global_proxy_mode,
        &request.global_proxy_url,
        request.proxy_resources,
    ) {
        Ok(_) => ok(
            "已保存 Codex 启动代理配置。",
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
        Err(err) => failed(
            &format!("保存启动代理配置失败：{err}"),
            chatgptx_core::official_accounts::get_official_accounts_snapshot(),
        ),
    }
}

#[tauri::command]
pub async fn test_launch_proxy(
    proxy_url: String,
) -> CommandResult<Option<chatgptx_core::official_accounts::ProxyConnectivityResult>> {
    match chatgptx_core::official_accounts::test_proxy_connectivity(&proxy_url).await {
        Ok(res) if res.reachable => ok(&res.detail.clone(), Some(res)),
        Ok(res) => failed(&res.detail.clone(), Some(res)),
        Err(err) => failed(&format!("测试代理失败：{err}"), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进程级全局状态的测试锁。
    ///
    /// 不止保护 `CODEX_HOME`：`paths::set_settings_path_for_tests`、
    /// `GROK_HOME` 同样是进程级的，两个测试并行跑就会互相把对方的值冲掉
    /// （表现为「单独跑过、全量跑挂」）。凡是动这几样的测试都必须先拿这把锁。
    static GLOBAL_STATE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_codex_home_for_test() -> std::sync::MutexGuard<'static, ()> {
        GLOBAL_STATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn native_browser_diagnostics_keeps_connection_independent_from_patch_status() {
        let result = NativeBrowserDiagnostics {
            compatibility: chatgptx_core::native_browser::BrowserStatus {
                state: "runtime_unverified".into(),
                detail: "Legacy runtime mismatch".into(),
            },
            connection: chatgptx_core::native_browser_connection::ConnectionStatus {
                state: "available".into(),
                failed_checks: 0,
                browsers: vec![
                    chatgptx_core::native_browser_connection::ConnectedBrowser {
                        family: "edge".into(),
                        header_enabled: Some(true),
                        recognized: true,
                    },
                ],
            },
        };
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["compatibility"]["state"], "runtime_unverified");
        assert_eq!(json["connection"]["state"], "available");
        assert_eq!(json["connection"]["browsers"][0]["headerEnabled"], true);
        assert_eq!(json["connection"]["failedChecks"], 0);
    }

    #[test]
    fn requested_launch_status_identifies_the_current_request() {
        let request = LaunchRequest {
            app_path: "C:/Program Files/Codex".to_string(),
            debug_port: 9333,
            helper_port: 57322,
            sync_active_relay: false,
        };

        let status = requested_launch_status(&request, "starting", "starting", 12345);

        assert_eq!(status.status, "starting");
        assert_eq!(status.message, "starting");
        assert_eq!(status.started_at_ms, 12345);
        assert_eq!(status.debug_port, Some(9333));
        assert_eq!(status.helper_port, Some(57322));
        assert_eq!(status.codex_app.as_deref(), Some("C:/Program Files/Codex"));
    }

    #[test]
    fn repeated_activation_returns_the_current_session_identity_and_actual_ports() {
        let request = launch_request(false);
        let current = LaunchStatus {
            status: "running".into(),
            started_at_ms: 123,
            debug_port: Some(9333),
            helper_port: Some(57322),
            ..LaunchStatus::default()
        };
        assert_eq!(
            launch_identity_from_status(&request, 456, Some(&current)),
            (9333, 57322, 123)
        );
        let starting = LaunchStatus {
            status: "starting".into(),
            ..current
        };
        assert_eq!(
            launch_identity_from_status(&request, 456, Some(&starting)),
            (request.debug_port, request.helper_port, 456)
        );
    }

    #[test]
    fn requested_launch_status_omits_an_unresolved_app_path() {
        let request = LaunchRequest {
            app_path: "  ".to_string(),
            debug_port: 9229,
            helper_port: 57321,
            sync_active_relay: false,
        };

        let status = requested_launch_status(&request, "starting", "starting", 1);

        assert_eq!(status.codex_app, None);
    }

    /// issue #2403：停止阶段要给出可判断的中间态（phase + progress），
    /// 否则十几秒的阻塞等待在界面上和「卡死」无法区分。
    #[test]
    fn stopping_status_carries_phase_and_progress() {
        let request = launch_request(false);

        let status = requested_launch_status_with_phase(
            &request,
            "stopping",
            "正在停止旧实例",
            12345,
            "restart_stopping",
            10,
        );

        assert_eq!(status.status, "stopping");
        assert_eq!(status.phase.as_deref(), Some("restart_stopping"));
        assert_eq!(status.progress, Some(10));
        // 中间态仍要带上请求标识，供「最近启动」判断是否属于本次重启。
        assert_eq!(status.started_at_ms, 12345);
        assert_eq!(status.debug_port, Some(request.debug_port));
        assert_eq!(status.helper_port, Some(request.helper_port));
    }

    /// 进度百分比必须收敛到 0~100，越界值不能写进状态文件。
    #[test]
    fn stopping_status_clamps_progress() {
        let request = launch_request(false);

        let status =
            requested_launch_status_with_phase(&request, "stopping", "msg", 1, "phase", 250);

        assert_eq!(status.progress, Some(100));
    }

    /// 普通启动状态不带阶段信息，保持既有行为（进度字段为 None）。
    #[test]
    fn plain_launch_status_has_no_phase() {
        let status = requested_launch_status(&launch_request(false), "starting", "starting", 1);

        assert_eq!(status.phase, None);
        assert_eq!(status.progress, None);
    }

    /// issue #618：live config.toml 有语法错误时必须被判成「可降级」，而不是让
    /// 整条供应商切换流程失败。缺失文件等同于空文件，同样不阻断。
    #[test]
    fn live_config_backfill_blocking_error_only_flags_unparseable_toml() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();

        // 文件不存在（全新安装）→ 不阻断。
        assert_eq!(live_config_backfill_blocking_error(home), None);

        // 空文件 / 只有空白 → 不阻断。
        std::fs::write(home.join("config.toml"), "  \n").unwrap();
        assert_eq!(live_config_backfill_blocking_error(home), None);

        // 正常 TOML → 不阻断。
        std::fs::write(
            home.join("config.toml"),
            "model_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://relay.example/v1\"\n",
        )
        .unwrap();
        assert_eq!(live_config_backfill_blocking_error(home), None);

        // 手写坏掉的 TOML（未闭合数组）→ 必须阻断并带上位置信息。
        std::fs::write(
            home.join("config.toml"),
            "model_provider = \"custom\"\nmodel_list = [\"a\", \n",
        )
        .unwrap();
        let error = live_config_backfill_blocking_error(home)
            .expect("unparseable live config must be reported as blocking");
        assert!(!error.is_empty(), "error detail must not be empty");
    }

    /// 重复表头/重复根键属于 core 归一层能修好的形态，预检不能误报成语法错误——
    /// 否则 #618 的降级路径会吃掉本来能成功的回填。
    #[test]
    fn live_config_backfill_blocking_error_tolerates_duplicate_tables_and_root_keys() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "\
model = \"a\"
model = \"b\"

[mcp_servers.node_repl]
command = \"node\"

[mcp_servers.node_repl]
cwd = \"/tmp\"
",
        )
        .unwrap();

        assert_eq!(live_config_backfill_blocking_error(temp.path()), None);
    }

    /// 预检必须与 core 的读取路径同样剥掉 BOM，否则带 BOM 的正常文件会被误判。
    #[test]
    fn live_config_backfill_blocking_error_ignores_utf8_bom() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "\u{feff}model_provider = \"openai\"\n",
        )
        .unwrap();

        assert_eq!(live_config_backfill_blocking_error(temp.path()), None);
    }

    #[test]
    fn provider_switch_state_helpers_restore_only_safe_state() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("codex-home");
        std::fs::create_dir(&home).unwrap();
        let state_path = home.join(".codex-global-state.json");
        std::fs::write(
            &state_path,
            json!({
                "electron-saved-workspace-roots": ["C:/work/app"],
                "thread-writable-roots": {"thread-1": ["C:/work/app"]},
                "electron-persisted-atom-state": {
                    "default-service-tier": "priority",
                    "electron:onboarding-workspace-autolaunch-applied": true,
                    "heartbeat-thread-permissions-by-id": {"thread-1": "do-not-copy"},
                    "prompt-history": ["do-not-copy"]
                },
                "provider-token-cache": "do-not-copy"
            })
            .to_string(),
        )
        .unwrap();

        prepare_codex_app_state_before_provider_switch(&home, "test.before");
        std::fs::write(
            &state_path,
            json!({"electron-saved-workspace-roots": ["D:/fresh/app"]}).to_string(),
        )
        .unwrap();
        finish_codex_app_state_after_provider_switch(&home, "test.after");

        let state: Value =
            serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
        assert_eq!(
            state["electron-saved-workspace-roots"],
            json!(["D:\\fresh\\app", "C:\\work\\app"])
        );
        assert_eq!(
            state["thread-writable-roots"]["thread-1"],
            json!(["C:/work/app"])
        );
        assert_eq!(
            state["electron-persisted-atom-state"]["default-service-tier"],
            "priority"
        );
        assert_eq!(
            state["electron-persisted-atom-state"]["electron:onboarding-workspace-autolaunch-applied"],
            true
        );
        assert!(state.get("provider-token-cache").is_none());
        assert!(
            state["electron-persisted-atom-state"]
                .get("heartbeat-thread-permissions-by-id")
                .is_none()
        );
        assert!(
            state["electron-persisted-atom-state"]
                .get("prompt-history")
                .is_none()
        );
    }

    #[test]
    fn backend_version_returns_structured_payload() {
        let result = backend_version();

        assert_eq!(result.status, "ok");
        assert!(!result.payload.version.is_empty());
    }

    fn provider_sync_result_for_test(
        status: chatgptx_data::ProviderSyncStatus,
        message: &str,
    ) -> chatgptx_data::ProviderSyncResult {
        chatgptx_data::ProviderSyncResult {
            status,
            message: message.to_string(),
            target_provider: "custom".to_string(),
            backup_dir: None,
            changed_session_files: 0,
            skipped_locked_rollout_files: Vec::new(),
            sqlite_rows_updated: 0,
            sqlite_provider_rows_updated: 0,
            sqlite_user_event_rows_updated: 0,
            sqlite_cwd_rows_updated: 0,
            sqlite_catalog_rows_inserted: 0,
            sqlite_catalog_rows_removed: 0,
            updated_workspace_roots: 0,
            encrypted_content_warning: None,
            repair_audit: chatgptx_data::ProviderSyncAudit::default(),
        }
    }

    #[test]
    fn provider_sync_success_reports_backup_directory_when_something_changed() {
        let mut sync = provider_sync_result_for_test(
            chatgptx_data::ProviderSyncStatus::Synced,
            "Provider sync complete",
        );
        sync.changed_session_files = 3;
        sync.backup_dir = Some(std::path::PathBuf::from("/tmp/backups/provider-sync/run-1"));

        let result = provider_sync_command_result(sync);

        assert_eq!(result.status, "ok");
        assert!(
            result.message.contains("/tmp/backups/provider-sync/run-1"),
            "有改动时应告知备份目录：{}",
            result.message
        );
    }

    /// 什么都没改的时候不该多出一行备份目录，避免噪音。
    #[test]
    fn provider_sync_success_omits_backup_hint_without_changes() {
        let sync = provider_sync_result_for_test(
            chatgptx_data::ProviderSyncStatus::Synced,
            "Provider sync already up to date",
        );

        let result = provider_sync_command_result(sync);

        assert_eq!(result.status, "ok");
        assert!(
            !result.message.contains("备份目录："),
            "无改动时不该提示备份目录：{}",
            result.message
        );
    }

    #[test]
    fn provider_sync_skipped_is_reported_as_command_failure() {
        let result = provider_sync_command_result(provider_sync_result_for_test(
            chatgptx_data::ProviderSyncStatus::Skipped,
            "Provider sync lock exists",
        ));

        assert_eq!(result.status, "failed");
        assert!(result.message.contains("Provider sync lock exists"));
        assert_eq!(result.payload["syncStatus"], "skipped");
    }

    #[test]
    fn provider_sync_preflight_failure_is_structured_as_skipped() {
        let result = provider_sync_preflight_failure("target is not resolvable");

        assert_eq!(result.status, "failed");
        assert_eq!(result.payload["syncStatus"], "skipped");
        assert_eq!(result.payload["targetProvider"], "");
        assert_eq!(result.payload["syncMessage"], "target is not resolvable");
    }

    #[test]
    fn manual_provider_sync_targets_keep_unresolvable_history_visible() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            r#"model_provider = "relay-live"

[model_providers.relay-live]
base_url = "https://example.invalid/v1"
"#,
        )
        .unwrap();
        let mut settings = BackendSettings::default();
        settings.provider_sync_manual_providers =
            vec!["relay-live".to_string(), "relay-history".to_string()];
        let manual = settings.provider_sync_manual_providers.clone();
        let mut targets = chatgptx_data::ProviderSyncTargetList {
            current_provider: "relay-live".to_string(),
            targets: Vec::new(),
        };

        merge_manual_provider_sync_targets(&mut targets, &manual, &settings, Some(temp.path()));

        let live = targets
            .targets
            .iter()
            .find(|target| target.id == "relay-live")
            .unwrap();
        assert!(live.is_resolvable);
        assert!(live.unavailable_reason.is_none());
        let history = targets
            .targets
            .iter()
            .find(|target| target.id == "relay-history")
            .unwrap();
        assert!(!history.is_resolvable);
        assert!(history.unavailable_reason.is_some());
    }

    #[test]
    fn provider_sync_synced_is_reported_as_command_success() {
        let result = provider_sync_command_result(provider_sync_result_for_test(
            chatgptx_data::ProviderSyncStatus::Synced,
            "Provider sync complete",
        ));

        assert_eq!(result.status, "ok");
        assert_eq!(result.payload["syncStatus"], "synced");
    }

    #[test]
    fn restart_absent_app_uses_launch_but_explicit_sync_still_restarts() {
        assert_eq!(
            restart_disposition(false, false),
            RestartDisposition::LaunchOnly
        );
        for (sync, running) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                restart_disposition(sync, running),
                RestartDisposition::StopAndRestart
            );
        }
    }

    #[test]
    fn restart_and_launch_share_a_single_flight_guard() {
        let first = try_acquire_launch_guard().unwrap();
        assert!(try_acquire_launch_guard().is_err());
        drop(first);
        assert!(try_acquire_launch_guard().is_ok());
    }

    #[test]
    fn restart_provider_guard_is_held_until_spawn_and_released_first() {
        let temp = tempfile::tempdir().unwrap();
        let guard =
            chatgptx_data::try_acquire_provider_sync_lifecycle_guard(Some(temp.path())).unwrap();
        assert!(
            chatgptx_data::try_acquire_provider_sync_lifecycle_guard(Some(temp.path())).is_err()
        );
        let spawned = std::cell::Cell::new(false);
        spawn_after_provider_guard_release(guard, &launch_request(false), |_| {
            let next =
                chatgptx_data::try_acquire_provider_sync_lifecycle_guard(Some(temp.path()))?;
            next.release()?;
            spawned.set(true);
            Ok(())
        })
        .unwrap();
        assert!(spawned.get());
    }

    #[test]
    fn restart_provider_guard_release_failure_blocks_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let guard =
            chatgptx_data::try_acquire_provider_sync_lifecycle_guard(Some(temp.path())).unwrap();
        fs::write(
            temp.path().join("tmp/provider-sync.lock/owner.json"),
            json!({"pid": std::process::id(), "startedAt":1, "lockId":"replacement"}).to_string(),
        )
        .unwrap();
        let spawned = std::cell::Cell::new(false);
        assert!(
            spawn_after_provider_guard_release(guard, &launch_request(false), |_| {
                spawned.set(true);
                Ok(())
            })
            .is_err()
        );
        assert!(!spawned.get());
    }

    #[test]
    fn startup_options_returns_structured_payload() {
        let result = startup_options();

        assert_eq!(result.status, "ok");
    }

    #[test]
    fn startup_options_honors_show_update_environment() {
        unsafe {
            std::env::set_var("CHATGPTX_SHOW_UPDATE", "1");
        }

        let result = startup_options();

        unsafe {
            std::env::remove_var("CHATGPTX_SHOW_UPDATE");
        }

        assert_eq!(result.status, "ok");
        assert!(result.payload.show_update);
    }

    #[test]
    fn startup_options_honors_show_update_argument() {
        assert!(should_show_update(
            ["chatgptx-manager.exe", "--show-update"],
            None
        ));
    }

    #[test]
    fn overview_contains_expected_operational_fields() {
        let result = tauri::async_runtime::block_on(load_overview());

        assert_eq!(result.status, "ok");
        assert!(!result.payload.current_version.is_empty());
        assert!(
            result.payload.codex_version.is_none()
                || result
                    .payload
                    .codex_version
                    .as_deref()
                    .is_some_and(|version| !version.is_empty())
        );
        assert!(matches!(
            result.payload.codex_app.status.as_str(),
            "found" | "missing"
        ));
        assert!(matches!(
            result.payload.silent_shortcut.status.as_str(),
            "installed" | "missing"
        ));
    }

    #[test]
    fn update_install_requires_release_payload() {
        let result = tauri::async_runtime::block_on(perform_update_inner(None));

        assert_eq!(result.status, "failed");
        assert!(result.message.contains("请先检查更新"));
    }

    #[test]
    fn watcher_state_returns_disabled_flag_path() {
        let result = load_watcher_state();

        assert_eq!(result.status, "ok");
        assert!(result.payload.disabled_flag.contains("watcher.disabled"));
    }

    #[test]
    fn missing_logs_return_failed_status() {
        let result = read_latest_logs(LogRequest { lines: 25 });

        if result.payload.text.is_empty() {
            assert_eq!(result.status, "failed");
        }
    }

    #[test]
    fn tray_menu_only_has_show_and_quit_in_both_languages() {
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();

        assert!(source.contains("Menu::with_items(app, &[&show_item, &quit_item])"));
        assert!(source.contains("Menu::with_items(&app, &[&show, &quit])"));
        assert!(!source.contains("tray_apply_dream_skin"));
        assert!(!source.contains("apply_dream_skin_from_tray"));
    }

    #[test]
    fn read_tail_returns_requested_lines_from_file_end() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("chatgptx.log");
        std::fs::write(&path, "one\ntwo\nthree\nfour\n").unwrap();

        let result = read_tail(&path, 2).unwrap();

        assert_eq!(result.text, "three\nfour");
        assert_eq!(result.file_size, 19);
        assert!(!result.truncated);
    }

    #[test]
    fn read_tail_does_not_load_prefix_when_log_is_large() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("chatgptx.log");
        let mut contents = String::from("prefix-should-not-appear\n");
        contents.push_str(&"x".repeat((MAX_LOG_TAIL_READ_BYTES as usize) + 128));
        contents.push_str("\nlast-1\nlast-2\n");
        std::fs::write(&path, contents).unwrap();

        let result = read_tail(&path, 2).unwrap();

        assert_eq!(result.text, "last-1\nlast-2");
        assert!(result.truncated);
        assert!(!result.text.contains("prefix-should-not-appear"));
    }

    #[test]
    fn relay_payload_does_not_expose_token_text() {
        let payload = relay_payload(
            chatgptx_core::relay_config::RelayStatus {
                authenticated: true,
                auth_source: "registry.json".to_string(),
                account_label: Some("user@example.test".to_string()),
                config_path: "config.toml".to_string(),
                configured: true,
                requires_openai_auth: true,
                has_bearer_token: true,
            },
            None,
        );
        let text = serde_json::to_string(&payload).unwrap();

        assert!(!text.contains("sk-"));
        assert!(text.contains("hasBearerToken"));
    }

    #[test]
    fn provider_doctor_recommendation_prioritizes_actionable_failures() {
        let recommendation = provider_doctor_recommendation(&[
            ProviderDoctorCheck {
                id: "models".to_string(),
                title: "模型列表".to_string(),
                status: "failed".to_string(),
                detail: "上游不支持 /v1/models".to_string(),
            },
            ProviderDoctorCheck {
                id: "request".to_string(),
                title: "真实请求".to_string(),
                status: "failed".to_string(),
                detail: "HTTP 404".to_string(),
            },
        ]);

        assert!(recommendation.contains("/v1/models"));
    }

    #[test]
    fn provider_doctor_recommendation_reports_model_warning() {
        let recommendation = provider_doctor_recommendation(&[
            ProviderDoctorCheck {
                id: "config".to_string(),
                title: "配置完整性".to_string(),
                status: "ok".to_string(),
                detail: "https://example.test/v1 / Responses API".to_string(),
            },
            ProviderDoctorCheck {
                id: "models".to_string(),
                title: "模型列表".to_string(),
                status: "warning".to_string(),
                detail: "未看到测试模型".to_string(),
            },
            ProviderDoctorCheck {
                id: "request".to_string(),
                title: "真实请求".to_string(),
                status: "ok".to_string(),
                detail: "HTTP 200".to_string(),
            },
        ]);

        assert!(recommendation.contains("测试模型"));
    }

    #[test]
    fn aggregate_relay_injection_writes_local_proxy_without_chatgpt_auth() {
        let temp = tempfile::tempdir().unwrap();

        let result = apply_aggregate_relay_injection_to_home(
            temp.path(),
            chatgptx_core::settings::RelaySessionProvider::Custom,
        );
        let config = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();

        assert_eq!(result.status, "ok");
        assert!(result.payload.configured);
        assert!(!result.payload.authenticated);
        assert!(config.contains(r#"base_url = "http://127.0.0.1:57321/v1""#));
        assert!(config.contains(r#"experimental_bearer_token = "chatgptx-aggregate""#));
    }

    fn launch_request(sync_active_relay: bool) -> LaunchRequest {
        LaunchRequest {
            app_path: String::new(),
            debug_port: 9229,
            helper_port: 57321,
            sync_active_relay,
        }
    }

    fn routed_pure_api_settings() -> BackendSettings {
        BackendSettings {
            active_relay_id: "source".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "source".to_string(),
                name: "Source".to_string(),
                base_url: "https://source.example/v1".to_string(),
                upstream_base_url: "https://source.example/v1".to_string(),
                api_key: "sk-source".to_string(),
                protocol: chatgptx_core::settings::RelayProtocol::Responses,
                relay_mode: chatgptx_core::settings::RelayMode::PureApi,
                config_contents: "model_provider = \"custom\"\n\n[model_providers.custom]\nname = \"custom\"\nwire_api = \"responses\"\nbase_url = \"https://source.example/v1\"\n"
                    .to_string(),
                auth_contents: "{\"OPENAI_API_KEY\":\"sk-source\"}\n".to_string(),
                model_routes: vec![chatgptx_core::settings::RelayModelRoute {
                    model: "gpt-5.6-luna".to_string(),
                    target_relay_id: "target".to_string(),
                    target_model: String::new(),
                }],
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        }
    }

    #[test]
    fn launch_request_defaults_active_relay_sync_to_false() {
        let request: LaunchRequest = serde_json::from_value(json!({})).unwrap();
        let requested: LaunchRequest =
            serde_json::from_value(json!({ "syncActiveRelay": true })).unwrap();

        assert!(!request.sync_active_relay);
        assert!(requested.sync_active_relay);
    }

    #[test]
    fn active_routed_pure_api_sync_writes_local_proxy() {
        let temp = tempfile::tempdir().unwrap();

        sync_active_relay_to_home(&routed_pure_api_settings(), temp.path()).unwrap();

        let config = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
        assert!(config.contains(r#"base_url = "http://127.0.0.1:57321/v1""#));
        assert!(!config.contains(r#"base_url = "https://source.example/v1""#));
    }

    #[test]
    fn active_official_sync_clears_custom_provider_selection() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "model_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://old.example/v1\"\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("auth.json"),
            "{\"OPENAI_API_KEY\":\"sk-old\",\"auth_mode\":\"chatgpt\"}\n",
        )
        .unwrap();
        let settings = BackendSettings {
            active_relay_id: "official".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "official".to_string(),
                relay_mode: chatgptx_core::settings::RelayMode::Official,
                official_mix_api_key: false,
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };

        sync_active_relay_to_home(&settings, temp.path()).unwrap();

        let config = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
        let parsed = config.parse::<toml_edit::DocumentMut>().unwrap();
        let auth = std::fs::read_to_string(temp.path().join("auth.json")).unwrap();
        assert!(parsed.get("model_provider").is_none());
        // 切回官方后残留的 base_url 会让请求继续发往中转站（issue #2216），
        // 所以整段中转站 provider 都要清掉，而不是只清掉「选择」。
        assert!(
            parsed
                .get("model_providers")
                .and_then(|providers| providers.get("custom"))
                .is_none(),
            "leftover relay provider must be removed: {config}"
        );
        assert!(!config.contains("old.example/v1"));
        assert!(!auth.contains("OPENAI_API_KEY"));
        assert!(auth.contains("auth_mode"));
    }

    #[test]
    fn active_aggregate_sync_writes_local_proxy() {
        let temp = tempfile::tempdir().unwrap();
        let settings = BackendSettings {
            active_relay_id: "aggregate".to_string(),
            active_aggregate_relay_id: "aggregate".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "aggregate".to_string(),
                relay_mode: chatgptx_core::settings::RelayMode::Aggregate,
                ..RelayProfile::default()
            }],
            aggregate_relay_profiles: vec![chatgptx_core::settings::AggregateRelayProfile {
                id: "aggregate".to_string(),
                name: "Aggregate".to_string(),
                session_provider: chatgptx_core::settings::RelaySessionProvider::Custom,
                model_aggregation_enabled: false,
                strategy: chatgptx_core::settings::AggregateRelayStrategy::Failover,
                members: Vec::new(),
                routes: Vec::new(),
                default_relay_id: String::new(),
                sync_default_member_login: false,
            }],
            ..BackendSettings::default()
        };

        sync_active_relay_to_home(&settings, temp.path()).unwrap();

        let config = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
        assert!(config.contains(r#"base_url = "http://127.0.0.1:57321/v1""#));
        assert!(config.contains(r#"experimental_bearer_token = "chatgptx-aggregate""#));
    }

    #[test]
    /// 回归（issue #1604）：用户点「重启 Codex」走的就是这条同步路径。
    /// 现场遗留的 0 字节 auth.json 必须被修复成合法 JSON，否则仍然停在登录页。
    fn active_aggregate_sync_repairs_empty_auth_json() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("auth.json"), "").unwrap();
        let settings = BackendSettings {
            active_relay_id: "aggregate".to_string(),
            active_aggregate_relay_id: "aggregate".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "aggregate".to_string(),
                relay_mode: chatgptx_core::settings::RelayMode::Aggregate,
                ..RelayProfile::default()
            }],
            aggregate_relay_profiles: vec![chatgptx_core::settings::AggregateRelayProfile {
                id: "aggregate".to_string(),
                name: "Aggregate".to_string(),
                session_provider: chatgptx_core::settings::RelaySessionProvider::Custom,
                model_aggregation_enabled: false,
                strategy: chatgptx_core::settings::AggregateRelayStrategy::Failover,
                members: Vec::new(),
                routes: Vec::new(),
                default_relay_id: String::new(),
                sync_default_member_login: false,
            }],
            ..BackendSettings::default()
        };

        sync_active_relay_to_home(&settings, temp.path()).unwrap();

        let raw = std::fs::read_to_string(temp.path().join("auth.json")).unwrap();
        let auth: serde_json::Value =
            serde_json::from_str(&raw).expect("重启同步后 auth.json 必须是合法 JSON");
        assert_eq!(
            auth.get("OPENAI_API_KEY").and_then(|item| item.as_str()),
            Some("chatgptx-aggregate")
        );
    }

    /// 模型聚合 + `[features] multi_agent_v2` 时，重启同步要带回冲突提示，
    /// 否则跨供应商子代理会在加密任务上静默失败（issue：feature flag 优先于目录）。
    #[test]
    fn model_aggregation_restart_reports_multi_agent_v2_conflict() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "model = \"old\"\n[features]\nmulti_agent_v2 = true\n",
        )
        .unwrap();
        let settings = BackendSettings {
            active_relay_id: "aggregate".to_string(),
            active_aggregate_relay_id: "aggregate".to_string(),
            relay_profiles: vec![
                RelayProfile {
                    id: "deepseek".to_string(),
                    name: "DeepSeek".to_string(),
                    model: "deepseek-chat".to_string(),
                    base_url: "https://deepseek.example/v1".to_string(),
                    api_key: "sk-test-redacted".to_string(),
                    relay_mode: chatgptx_core::settings::RelayMode::PureApi,
                    ..RelayProfile::default()
                },
                RelayProfile {
                    id: "aggregate".to_string(),
                    relay_mode: chatgptx_core::settings::RelayMode::Aggregate,
                    ..RelayProfile::default()
                },
            ],
            aggregate_relay_profiles: vec![chatgptx_core::settings::AggregateRelayProfile {
                id: "aggregate".to_string(),
                name: "Aggregate".to_string(),
                session_provider: chatgptx_core::settings::RelaySessionProvider::Custom,
                model_aggregation_enabled: true,
                strategy: chatgptx_core::settings::AggregateRelayStrategy::Failover,
                members: vec![chatgptx_core::settings::AggregateRelayMember {
                    relay_id: "deepseek".to_string(),
                    weight: 1,
                }],
                routes: Vec::new(),
                default_relay_id: String::new(),
                sync_default_member_login: false,
            }],
            ..BackendSettings::default()
        };

        let warnings = restart_chatgptx_after_stop(
            &launch_request(true),
            temp.path(),
            Some(&settings),
            |_| Ok(()),
        )
        .unwrap();

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("multi_agent_v2"), "{warnings:?}");
    }

    #[test]
    fn failed_active_relay_sync_does_not_spawn_or_change_live_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("config.toml"), "model = \"old\"\n").unwrap();
        std::fs::write(temp.path().join("auth.json"), "{\"old\":true}\n").unwrap();
        let settings = BackendSettings {
            active_relay_id: "broken".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "broken".to_string(),
                base_url: String::new(),
                api_key: String::new(),
                relay_mode: chatgptx_core::settings::RelayMode::PureApi,
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };
        let spawned = std::cell::Cell::new(false);

        let result = restart_chatgptx_after_stop(
            &launch_request(true),
            temp.path(),
            Some(&settings),
            |_| {
                spawned.set(true);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert!(!spawned.get());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("config.toml")).unwrap(),
            "model = \"old\"\n"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("auth.json")).unwrap(),
            "{\"old\":true}\n"
        );
    }

    #[test]
    fn restart_stops_codex_then_runtime_before_checking_native_cleanup() {
        let events = std::cell::RefCell::new(Vec::new());
        stop_chatgptx_for_restart(
            || {
                events.borrow_mut().push("codex");
                Ok(())
            },
            || {
                events.borrow_mut().push("runtime");
                Ok(())
            },
            || {
                events.borrow_mut().push("cleanup");
                Ok(chatgptx_core::native_browser::NativeBrowserShutdown::Ready)
            },
        )
        .unwrap();
        assert_eq!(*events.borrow(), ["codex", "runtime", "cleanup"]);
    }

    #[test]
    fn restore_failure_is_reported_after_runtime_has_stopped() {
        let stopped = std::cell::Cell::new(false);
        let shutdown = stop_chatgptx_for_restart(
            || Ok(()),
            || {
                stopped.set(true);
                Ok(())
            },
            || Ok(chatgptx_core::native_browser::NativeBrowserShutdown::RestoreFailed),
        )
        .unwrap();
        assert!(stopped.get());
        assert_eq!(
            shutdown,
            chatgptx_core::native_browser::NativeBrowserShutdown::RestoreFailed
        );
    }

    #[test]
    fn restart_stop_errors_name_the_step_that_failed() {
        let browser = restart_stop_failure_message(&RestartStopError::NativeBrowser(
            anyhow::Error::new(chatgptx_core::native_browser::NativeBrowserCleanupStillRunning),
        ));
        let runtime = restart_stop_failure_message(&RestartStopError::Runtime(anyhow::anyhow!(
            "runtime still exiting"
        )));
        assert!(browser.contains("原生浏览器文件仍在恢复"));
        assert!(runtime.contains("后台服务尚未清理完成"));
        assert!(!runtime.contains("原生浏览器文件仍在恢复"));
        let codex = restart_stop_failure_message(&RestartStopError::Codex(anyhow::anyhow!(
            "app remains running"
        )));
        assert!(codex.contains("Codex 进程尚未确认退出"));
        let failed = restart_stop_failure_message(&RestartStopError::NativeBrowser(
            anyhow::anyhow!("Invalid native cleanup receipt"),
        ));
        assert!(failed.contains("原生浏览器文件恢复失败"));
    }

    #[test]
    fn restart_does_not_continue_when_runtime_cleanup_fails() {
        let result = stop_chatgptx_for_restart(
            || Ok(()),
            || anyhow::bail!("runtime still running"),
            || panic!("must not continue before runtime cleanup"),
        );
        assert!(matches!(result, Err(RestartStopError::Runtime(_))));
    }

    #[test]
    fn restart_does_not_cleanup_if_app_stop_fails() {
        let result = stop_chatgptx_for_restart(
            || anyhow::bail!("app remains running"),
            || panic!("must not stop runtime"),
            || panic!("must not wait on cleanup"),
        );
        assert!(matches!(result, Err(RestartStopError::Codex(_))));
    }

    #[test]
    fn failed_spawn_rolls_back_synced_live_files_for_retry() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("config.toml"), "model = \"old\"\n").unwrap();
        std::fs::write(temp.path().join("auth.json"), "{\"old\":true}\n").unwrap();

        let result = restart_chatgptx_after_stop(
            &launch_request(true),
            temp.path(),
            Some(&routed_pure_api_settings()),
            |_| anyhow::bail!("synthetic spawn failure"),
        );

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("config.toml")).unwrap(),
            "model = \"old\"\n"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("auth.json")).unwrap(),
            "{\"old\":true}\n"
        );
    }

    #[test]
    fn ordinary_restart_does_not_read_or_change_live_relay_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("config.toml"), "model = \"old\"\n").unwrap();
        std::fs::write(temp.path().join("auth.json"), "{\"old\":true}\n").unwrap();
        let spawned = std::cell::Cell::new(false);

        restart_chatgptx_after_stop(&launch_request(false), temp.path(), None, |_| {
            spawned.set(true);
            Ok(())
        })
        .unwrap();

        assert!(spawned.get());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("config.toml")).unwrap(),
            "model = \"old\"\n"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("auth.json")).unwrap(),
            "{\"old\":true}\n"
        );
    }

    #[test]
    fn relay_files_payload_reads_config_and_auth_contents() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "model_provider = \"custom\"\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("auth.json"),
            "{\"OPENAI_API_KEY\":\"sk-test\"}\n",
        )
        .unwrap();

        let payload = relay_files_payload_from_home(temp.path()).unwrap();

        assert!(payload.config_path.ends_with("config.toml"));
        assert!(payload.auth_path.ends_with("auth.json"));
        assert_eq!(payload.config_contents, "model_provider = \"custom\"\n");
        assert_eq!(payload.auth_contents, "{\"OPENAI_API_KEY\":\"sk-test\"}\n");
    }

    #[test]
    fn env_conflict_commands_ignore_codex_home_and_remove_openai_vars() {
        let _codex_home_guard = lock_codex_home_for_test();
        let test_openai_name = "OPENAI_CHATGPTX_ENV_CONFLICT_TEST";
        let previous_openai = std::env::var_os(test_openai_name);
        let previous_codex_home = std::env::var_os("CODEX_HOME");
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var(test_openai_name, "sk-test");
            std::env::set_var("CODEX_HOME", temp.path());
        }

        let check = check_env_conflicts();
        assert_eq!(check.status, "ok");
        assert!(
            check
                .payload
                .conflicts
                .iter()
                .any(|item| item.name == test_openai_name)
        );
        assert!(
            !check
                .payload
                .conflicts
                .iter()
                .any(|item| item.name == "CODEX_HOME")
        );

        chatgptx_core::env_conflicts::remove_process_env_conflicts_for_tests(
            &[test_openai_name.to_string(), "CODEX_HOME".to_string()],
            chatgptx_core::paths::default_app_state_dir().join("test-backups"),
        )
        .unwrap();
        assert!(std::env::var_os(test_openai_name).is_none());
        assert_eq!(
            std::env::var_os("CODEX_HOME"),
            Some(temp.path().as_os_str().to_os_string())
        );

        unsafe {
            match previous_openai {
                Some(value) => std::env::set_var(test_openai_name, value),
                None => std::env::remove_var(test_openai_name),
            }
            match previous_codex_home {
                Some(value) => std::env::set_var("CODEX_HOME", value),
                None => std::env::remove_var("CODEX_HOME"),
            }
        }
    }

    #[test]
    fn delete_local_session_falls_back_when_requested_db_no_longer_contains_thread() {
        let _codex_home_guard = lock_codex_home_for_test();
        let temp = tempfile::tempdir().unwrap();
        let previous_codex_home = std::env::var_os("CODEX_HOME");
        let codex_home = temp.path().join("codex-home");
        let sqlite_dir = codex_home.join("sqlite");
        std::fs::create_dir_all(&sqlite_dir).unwrap();
        let stale_db = sqlite_dir.join("codex-dev.db");
        let active_db = sqlite_dir.join("state_5.sqlite");
        let rollout_path = temp.path().join("rollout.jsonl");
        std::fs::write(&rollout_path, "{\"type\":\"message\"}\n").unwrap();
        let stale = rusqlite::Connection::open(&stale_db).unwrap();
        stale
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT)",
                [],
            )
            .unwrap();
        drop(stale);
        let active = rusqlite::Connection::open(&active_db).unwrap();
        active
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT)",
                [],
            )
            .unwrap();
        active
            .execute(
                "INSERT INTO threads VALUES ('t1', ?1, 'Active Thread')",
                [rollout_path.to_string_lossy().to_string()],
            )
            .unwrap();
        drop(active);

        unsafe {
            std::env::set_var("CODEX_HOME", &codex_home);
        }
        let result = delete_local_session(DeleteLocalSessionRequest {
            session_id: "t1".to_string(),
            title: "Active Thread".to_string(),
            db_path: Some(stale_db.to_string_lossy().to_string()),
        });
        unsafe {
            if let Some(value) = previous_codex_home {
                std::env::set_var("CODEX_HOME", value);
            } else {
                std::env::remove_var("CODEX_HOME");
            }
        }

        assert_eq!(result.status, "ok");
        assert_eq!(
            result.payload.status,
            chatgptx_core::models::DeleteStatus::LocalDeleted
        );
        let active = rusqlite::Connection::open(&active_db).unwrap();
        assert_eq!(
            active
                .query_row("SELECT COUNT(*) FROM threads WHERE id = 't1'", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn list_local_sessions_deduplicates_threads_across_current_and_legacy_dbs() {
        let _codex_home_guard = lock_codex_home_for_test();
        let temp = tempfile::tempdir().unwrap();
        let previous_codex_home = std::env::var_os("CODEX_HOME");
        let codex_home = temp.path().join("codex-home");
        let sqlite_dir = codex_home.join("sqlite");
        std::fs::create_dir_all(&sqlite_dir).unwrap();
        let current_db = sqlite_dir.join("state_5.sqlite");
        let legacy_db = codex_home.join("state_5.sqlite");
        create_minimal_thread_db(&current_db, "t1", "Current Copy", 100);
        create_minimal_thread_db(&legacy_db, "t1", "Legacy Copy", 200);

        unsafe {
            std::env::set_var("CODEX_HOME", &codex_home);
        }
        let result = list_local_sessions(None);

        assert_eq!(result.status, "ok");
        assert_eq!(result.payload.sessions.len(), 1);
        assert_eq!(result.payload.total_count, 1);
        assert_eq!(result.payload.sessions[0].id, "t1");
        assert_eq!(result.payload.sessions[0].title, "Legacy Copy");
        assert_eq!(
            result.payload.sessions[0].db_path,
            legacy_db.to_string_lossy()
        );

        rusqlite::Connection::open(&current_db)
            .unwrap()
            .execute("INSERT INTO threads VALUES ('t2', '', 'Newest', 300)", [])
            .unwrap();
        rusqlite::Connection::open(&legacy_db)
            .unwrap()
            .execute("INSERT INTO threads VALUES ('t3', '', 'Oldest', 50)", [])
            .unwrap();

        let first_page = list_local_sessions(Some(ListLocalSessionsRequest {
            offset: 0,
            limit: 2,
        }));
        assert_eq!(first_page.payload.sessions.len(), 2);
        assert_eq!(first_page.payload.sessions[0].id, "t2");
        assert_eq!(first_page.payload.sessions[1].id, "t1");
        assert!(first_page.payload.has_more);
        assert_eq!(first_page.payload.total_count, 3);

        let second_page = list_local_sessions(Some(ListLocalSessionsRequest {
            offset: 2,
            limit: 2,
        }));
        restore_codex_home(previous_codex_home);

        assert_eq!(second_page.payload.sessions.len(), 1);
        assert_eq!(second_page.payload.sessions[0].id, "t3");
        assert!(!second_page.payload.has_more);
        assert_eq!(second_page.payload.total_count, 3);
    }

    #[test]
    fn list_local_sessions_ignores_relation_only_thread_reference_dbs() {
        let _codex_home_guard = lock_codex_home_for_test();
        let temp = tempfile::tempdir().unwrap();
        let previous_codex_home = std::env::var_os("CODEX_HOME");
        let codex_home = temp.path().join("codex-home");
        let sqlite_dir = codex_home.join("sqlite");
        std::fs::create_dir_all(&sqlite_dir).unwrap();
        let session_db = sqlite_dir.join("state_5.sqlite");
        let relation_db = sqlite_dir.join("codex-related.db");
        create_minimal_thread_db(&session_db, "t1", "Current Thread", 100);
        let relation = rusqlite::Connection::open(&relation_db).unwrap();
        relation
            .execute(
                "CREATE TABLE local_thread_catalog (thread_id TEXT PRIMARY KEY)",
                [],
            )
            .unwrap();
        relation
            .execute("INSERT INTO local_thread_catalog VALUES ('t1')", [])
            .unwrap();
        drop(relation);

        unsafe {
            std::env::set_var("CODEX_HOME", &codex_home);
        }
        let result = list_local_sessions(None);
        restore_codex_home(previous_codex_home);

        assert_eq!(result.status, "ok");
        assert_eq!(result.payload.sessions.len(), 1);
        assert_eq!(result.payload.sessions[0].id, "t1");
        assert_eq!(result.payload.sessions[0].title, "Current Thread");
    }

    #[test]
    fn delete_local_session_removes_duplicate_threads_from_all_candidate_dbs() {
        let _codex_home_guard = lock_codex_home_for_test();
        let temp = tempfile::tempdir().unwrap();
        let previous_codex_home = std::env::var_os("CODEX_HOME");
        let codex_home = temp.path().join("codex-home");
        let sqlite_dir = codex_home.join("sqlite");
        std::fs::create_dir_all(&sqlite_dir).unwrap();
        let current_db = sqlite_dir.join("state_5.sqlite");
        let legacy_db = codex_home.join("state_5.sqlite");
        create_minimal_thread_db(&current_db, "t1", "Current Copy", 100);
        create_minimal_thread_db(&legacy_db, "t1", "Legacy Copy", 200);

        unsafe {
            std::env::set_var("CODEX_HOME", &codex_home);
        }
        let result = delete_local_session(DeleteLocalSessionRequest {
            session_id: "t1".to_string(),
            title: "Legacy Copy".to_string(),
            db_path: Some(legacy_db.to_string_lossy().to_string()),
        });
        restore_codex_home(previous_codex_home);

        assert_eq!(result.status, "ok");
        assert_eq!(thread_count(&current_db, "t1"), 0);
        assert_eq!(thread_count(&legacy_db, "t1"), 0);
    }

    fn create_minimal_thread_db(path: &Path, id: &str, title: &str, updated_at_ms: i64) {
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at_ms INTEGER)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, '', ?2, ?3)",
            (id, title, updated_at_ms),
        )
        .unwrap();
    }

    fn thread_count(path: &Path, id: &str) -> i64 {
        let db = rusqlite::Connection::open(path).unwrap();
        db.query_row("SELECT COUNT(*) FROM threads WHERE id = ?1", [id], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap()
    }

    fn restore_codex_home(previous: Option<std::ffi::OsString>) {
        unsafe {
            if let Some(value) = previous {
                std::env::set_var("CODEX_HOME", value);
            } else {
                std::env::remove_var("CODEX_HOME");
            }
        }
    }

    #[test]
    fn apply_relay_profile_to_home_with_switch_rules_preserves_custom_provider_id() {
        let temp = tempfile::tempdir().unwrap();
        let profile = RelayProfile {
            relay_mode: chatgptx_core::settings::RelayMode::PureApi,
            protocol: chatgptx_core::settings::RelayProtocol::Responses,
            config_contents: "model_provider = \"ai\"\nmodel = \"gpt-image-2\"\n\n[model_providers.ai]\nname = \"ai\"\nwire_api = \"responses\"\nrequires_openai_auth = true\nbase_url = \"https://ahg.codes\"\n"
                .to_string(),
            auth_contents: "{}\n".to_string(),
            ..RelayProfile::default()
        };

        chatgptx_core::relay_config::apply_relay_profile_to_home_with_switch_rules(
            temp.path(),
            &profile,
            "",
        )
        .unwrap();

        let applied = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
        assert!(applied.contains("model_provider = \"ai\""));
        assert!(applied.contains("[model_providers.ai]"));
        assert!(!applied.contains("[model_providers.custom]"));
    }

    #[test]
    fn save_relay_file_in_home_only_allows_known_files() {
        let temp = tempfile::tempdir().unwrap();

        save_relay_file_in_home(temp.path(), "config", "model = \"gpt-5\"\n").unwrap();
        save_relay_file_in_home(temp.path(), "auth", "{}\n").unwrap();

        assert_eq!(
            std::fs::read_to_string(temp.path().join("config.toml")).unwrap(),
            "model = \"gpt-5\"\n"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("auth.json")).unwrap(),
            "{}\n"
        );
        assert!(save_relay_file_in_home(temp.path(), "../bad", "").is_err());
    }

    #[test]
    fn config_save_applies_official_deepseek_responses_compatibility() {
        let profile = RelayProfile {
            id: "custom-deepseek".to_string(),
            base_url: "https://api.deepseek.com/".to_string(),
            upstream_base_url: "https://api.deepseek.com/".to_string(),
            protocol: chatgptx_core::settings::RelayProtocol::Responses,
            ..RelayProfile::default()
        };
        let config = r#"[features]
unified_exec = true
code_mode_only = true

[features.code_mode]
enabled = true
"#;

        let prepared = prepare_relay_file_contents("config", config, &profile).unwrap();

        assert!(prepared.contains("unified_exec = true"));
        assert!(prepared.contains("code_mode_only = false"));
        assert!(prepared.contains("enabled = false"));
    }

    #[test]
    fn config_save_preserves_third_party_responses_and_deepseek_chat_completions() {
        let config = r#"[features]
code_mode_only = true

[features.code_mode]
enabled = true
"#;
        let third_party = RelayProfile {
            base_url: "https://relay.example/v1".to_string(),
            upstream_base_url: "https://relay.example/v1".to_string(),
            protocol: chatgptx_core::settings::RelayProtocol::Responses,
            ..RelayProfile::default()
        };
        let chat_completions = RelayProfile {
            base_url: "https://api.deepseek.com/".to_string(),
            upstream_base_url: "https://api.deepseek.com/".to_string(),
            protocol: chatgptx_core::settings::RelayProtocol::ChatCompletions,
            ..RelayProfile::default()
        };

        assert_eq!(
            prepare_relay_file_contents("config", config, &third_party).unwrap(),
            config
        );
        assert_eq!(
            prepare_relay_file_contents("config", config, &chat_completions).unwrap(),
            config
        );
    }

    #[test]
    /// #1972：用户误把 ChatGPTX 自己的 exe 选成了「Codex 应用路径」——文件选择器
    /// 只按 exe 扩展名过滤，拦不住。以前无效路径会原样存进 settings.json，而
    /// launcher 拿到显式无效 --app-path 又不回退自动探测，于是启动永久失败，
    /// 只能手改配置文件才能恢复。
    fn normalize_settings_before_save_drops_an_invalid_codex_app_path() {
        let chatgptx_own_exe = if cfg!(windows) {
            r"D:\ChatGPTX\chatgptx.exe"
        } else {
            "/Applications/ChatGPTX/chatgptx"
        };
        let settings = BackendSettings {
            codex_app_path: chatgptx_own_exe.to_string(),
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);

        // 清空而不是留着：留着就会被当成显式 --app-path 传下去，永久失败
        assert!(
            normalized.codex_app_path.is_empty(),
            "指向 ChatGPTX 自身的路径不该落库，实际是 {}",
            normalized.codex_app_path
        );
    }

    #[test]
    fn normalize_settings_before_save_keeps_an_empty_codex_app_path_empty() {
        let settings = BackendSettings {
            codex_app_path: "   ".to_string(),
            ..BackendSettings::default()
        };

        assert!(
            normalize_settings_before_save(settings)
                .codex_app_path
                .is_empty()
        );
    }

    #[test]
    fn backfill_request_preserves_identity_unless_explicitly_adopted() {
        let settings = serde_json::to_value(BackendSettings::default()).unwrap();
        let request: BackfillRelayProfileRequest = serde_json::from_value(json!({
            "settings": settings, "profileId": "profile-a"
        }))
        .unwrap();
        assert!(!request.adopt_provider_identity);
        let request: BackfillRelayProfileRequest = serde_json::from_value(json!({
            "settings": settings, "profileId": "profile-a", "adoptProviderIdentity": true
        }))
        .unwrap();
        assert!(request.adopt_provider_identity);
    }

    #[test]
    fn normalize_settings_before_save_preserves_profile_context_until_manual_extract() {
        let settings = BackendSettings {
            relay_common_config_contents: "[mcp_servers.context7]\ncommand = \"npx\"\n".to_string(),
            relay_profiles: vec![RelayProfile {
                use_common_config: false,
                config_contents: "model = \"gpt-5\"\n\n[mcp_servers.context7]\ncommand = \"npx\"\n"
                    .to_string(),
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);

        assert!(
            normalized.relay_profiles[0]
                .config_contents
                .contains("model = \"gpt-5\"")
        );
        assert!(
            normalized.relay_profiles[0]
                .config_contents
                .contains("[mcp_servers.context7]")
        );
        assert!(
            normalized
                .relay_context_config_contents
                .contains("[mcp_servers.context7]")
        );
        assert!(
            !normalized
                .relay_common_config_contents
                .contains("[mcp_servers")
        );
    }

    #[test]
    fn normalize_settings_before_save_ignores_legacy_launch_mode_and_preserves_provider_mode() {
        let settings: BackendSettings = serde_json::from_value(json!({
            "activeRelayId": "api", "launchMode": "relay",
            "relayProfiles": [{ "id": "api", "name": "API", "relayMode": "pureApi" }],
        }))
        .unwrap();
        let normalized = normalize_settings_before_save(settings);
        assert_eq!(
            normalized.relay_profiles[0].relay_mode,
            chatgptx_core::settings::RelayMode::PureApi
        );
        assert!(
            serde_json::to_value(&normalized)
                .unwrap()
                .get("launchMode")
                .is_none()
        );
    }

    #[test]
    fn list_tools_reports_codex_with_its_provider_state() {
        let _guard = lock_codex_home_for_test();
        let temp = tempfile::tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");
        let previous = chatgptx_core::paths::set_settings_path_for_tests(Some(settings_path));

        let settings = BackendSettings {
            active_relay_id: "supplier-a".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "supplier-a".to_string(),
                name: "供应商 A".to_string(),
                relay_mode: chatgptx_core::settings::RelayMode::PureApi,
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };
        SettingsStore::default().save(&settings).unwrap();

        let result = list_tools();
        chatgptx_core::paths::set_settings_path_for_tests(previous);

        assert_eq!(result.status, "ok");
        assert_eq!(result.payload.active_tool, "codex");

        let codex = result
            .payload
            .tools
            .iter()
            .find(|tool| tool.id == "codex")
            .expect("codex 必须在工具列表里");
        assert!(codex.switchable);
        assert!(codex.active);
        assert_eq!(codex.active_relay_name.as_deref(), Some("供应商 A"));
        assert_eq!(codex.relay_count, 1);
    }

    #[test]
    fn reset_image_overlay_settings_preserves_supplier_settings() {
        let _guard = lock_codex_home_for_test();
        let temp = tempfile::tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");
        let previous = chatgptx_core::paths::set_settings_path_for_tests(Some(settings_path));

        let settings = BackendSettings {
            codex_app_image_overlay_enabled: true,
            codex_app_image_overlay_path: "C:\\Users\\me\\Pictures\\overlay.png".to_string(),
            codex_app_image_overlay_opacity: 42,
            codex_app_image_overlay_fit_mode: "fill".to_string(),
            active_relay_id: "supplier-a".to_string(),
            relay_profiles: vec![RelayProfile {
                id: "supplier-a".to_string(),
                name: "供应商 A".to_string(),
                relay_mode: chatgptx_core::settings::RelayMode::PureApi,
                api_key: "sk-test".to_string(),
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };
        SettingsStore::default().save(&settings).unwrap();

        let result = reset_image_overlay_settings();
        chatgptx_core::paths::set_settings_path_for_tests(previous);

        assert_eq!(result.status, "ok");
        assert!(!result.payload.settings.codex_app_image_overlay_enabled);
        assert_eq!(result.payload.settings.codex_app_image_overlay_path, "");
        assert_eq!(result.payload.settings.codex_app_image_overlay_opacity, 35);
        assert_eq!(
            result.payload.settings.codex_app_image_overlay_fit_mode,
            "fit"
        );
        assert_eq!(result.payload.settings.active_relay_id, "supplier-a");
        assert_eq!(result.payload.settings.relay_profiles.len(), 1);
        assert_eq!(result.payload.settings.relay_profiles[0].id, "supplier-a");
        assert_eq!(result.payload.settings.relay_profiles[0].api_key, "sk-test");
    }

    #[test]
    fn normalize_settings_before_save_preserves_official_profile_auth() {
        let settings = BackendSettings {
            relay_profiles: vec![RelayProfile {
                relay_mode: chatgptx_core::settings::RelayMode::Official,
                official_mix_api_key: false,
                auth_contents: r#"{"auth_mode":"chatgpt","tokens":{"access_token":"edited"}}"#
                    .to_string(),
                config_contents: "model_provider = \"custom\"\n".to_string(),
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);

        let auth_json: serde_json::Value =
            serde_json::from_str(&normalized.relay_profiles[0].auth_contents).unwrap();
        assert_eq!(
            auth_json,
            serde_json::json!({
                "auth_mode": "chatgpt",
                "tokens": {
                    "access_token": "edited"
                }
            })
        );
        assert!(normalized.relay_profiles[0].config_contents.is_empty());
    }

    #[test]
    fn normalize_settings_before_save_strips_common_from_enabled_profile() {
        let settings = BackendSettings {
            relay_common_config_contents: r#"model_reasoning_effort = "high"

[features]
goals = true

[plugins."superpowers@openai-curated"]
enabled = true
"#
            .to_string(),
            relay_profiles: vec![RelayProfile {
                use_common_config: true,
                config_contents: r#"model = "gpt-5"
model_reasoning_effort = "high"

[features]
goals = true
model_reasoning_effort = "high"

[plugins."superpowers@openai-curated"]
enabled = true
"#
                .to_string(),
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);
        let config = &normalized.relay_profiles[0].config_contents;

        assert!(config.contains("model = \"gpt-5\""));
        assert!(!config.contains("model_reasoning_effort"));
        // `goals` is an explicit per-profile override and must survive
        // normalization even when it matches the common configuration.
        assert!(config.contains("[features]"));
        assert!(config.contains("goals = true"));
        assert!(!config.contains("[plugins.\"superpowers@openai-curated\"]"));
    }

    #[test]
    fn normalize_settings_before_save_preserves_explicit_false_goals_override() {
        let settings = BackendSettings {
            relay_common_config_contents: "[features]\ngoals = true\nfast_mode = true\n"
                .to_string(),
            relay_profiles: vec![RelayProfile {
                use_common_config: true,
                config_contents: "model = \"gpt-5\"\n[features]\ngoals = false\n".to_string(),
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);
        let config = &normalized.relay_profiles[0].config_contents;
        assert!(config.contains("goals = false"));
        assert!(!config.contains("fast_mode = true"));
    }

    #[test]
    fn normalize_settings_before_save_repairs_invalid_profile_common_duplication() {
        let settings = BackendSettings {
            relay_common_config_contents: r#"model_reasoning_effort = "high"

[marketplaces.openai-bundled]
last_updated = "2026-05-25T11:52:46Z"
"#
            .to_string(),
            relay_profiles: vec![RelayProfile {
                use_common_config: true,
                config_contents: r#"model = "gpt-5"
model_reasoning_effort = "high"

[marketplaces.openai-bundled]
last_updated = "2026-05-25T11:52:46Z"

[marketplaces.openai-bundled]
last_updated = "2026-05-25T11:52:46Z"
"#
                .to_string(),
                ..RelayProfile::default()
            }],
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);
        let config = &normalized.relay_profiles[0].config_contents;

        assert!(config.contains("model = \"gpt-5\""));
        assert!(!config.contains("model_reasoning_effort"));
        assert!(!config.contains("[marketplaces.openai-bundled]"));
    }

    #[test]
    fn normalize_settings_before_save_removes_model_catalog_from_common_config() {
        let settings = BackendSettings {
            relay_common_config_contents: r#"model_catalog_json = "C:\\Users\\Administrator\\.codex\\model-catalogs\\relay-a.json"
model_catalog_json = 'C:\Users\Administrator\.codex\model-catalogs\relay-b.json'
model_reasoning_effort = "high"
"#
            .to_string(),
            ..BackendSettings::default()
        };

        let normalized = normalize_settings_before_save(settings);

        assert!(
            !normalized
                .relay_common_config_contents
                .contains("model_catalog_json")
        );
        assert!(
            normalized
                .relay_common_config_contents
                .contains("model_reasoning_effort = \"high\"")
        );
    }

    #[test]
    fn context_entry_commands_update_settings_payload() {
        let settings = BackendSettings::default();
        let upsert = upsert_context_entry(ContextEntryRequest {
            settings: settings.clone(),
            kind: "mcp".to_string(),
            id: "context7".to_string(),
            toml_body: "command = \"npx\"\n".to_string(),
        });

        assert_eq!(upsert.status, "ok");
        assert!(
            upsert
                .payload
                .settings
                .relay_context_config_contents
                .contains("[mcp_servers.context7]")
        );

        let listed = list_context_entries(ContextSettingsRequest {
            settings: upsert.payload.settings.clone(),
        });
        assert_eq!(listed.payload.entries.mcp_servers[0].id, "context7");

        let deleted = delete_context_entry(ContextDeleteRequest {
            settings: upsert.payload.settings,
            kind: "mcp".to_string(),
            id: "context7".to_string(),
        });
        assert_eq!(deleted.status, "ok");
        assert!(
            !deleted
                .payload
                .settings
                .relay_context_config_contents
                .contains("[mcp_servers.context7]")
        );
    }


    #[test]
    fn open_external_url_rejects_non_http_urls() {
        let result = open_external_url("file:///C:/Windows/win.ini".to_string());

        assert_eq!(result.status, "failed");
        assert!(result.message.contains("只允许打开 http 或 https 链接"));
    }

    #[test]
    fn multi_instance_slug_is_safe_and_bounded() {
        assert_eq!(
            multi_instance_slug("Deep/Seek .. profile"),
            "Deep-Seek-profile"
        );
        assert_eq!(multi_instance_slug("///"), "instance");
        assert!(multi_instance_slug(&"x".repeat(200)).len() <= 64);
    }

    #[test]
    fn multi_instance_port_selection_finds_distinct_loopback_ports() {
        let first = select_multi_instance_port(0, "official");
        let second = select_multi_instance_port(first.saturating_add(1), "deepseek");
        assert!(first > 0);
        assert!(second > 0);
        assert_ne!(first, second);
    }

    #[test]
    fn multi_instance_manifest_roundtrips_runtime_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let instance = CodexMultiInstance {
            instance_id: "zhipu".to_string(),
            profile_id: "profile-zhipu".to_string(),
            profile_name: "Zhipu GLM".to_string(),
            pid: Some(1234),
            debug_port: 9456,
            helper_port: 9457,
            status: "starting".to_string(),
            running: true,
            started_at_ms: 99,
            is_main: false,
            app_path: Some("/Applications/Codex.app".to_string()),
            codex_home: Some("/tmp/codex-home".to_string()),
            sqlite_home: Some("/tmp/sqlite-home".to_string()),
            user_data_dir: Some("/tmp/user-data".to_string()),
            launch_result: None,
        };
        save_multi_instance_manifest(temp.path(), &instance).unwrap();
        assert_eq!(
            load_multi_instance_manifest(temp.path())
                .unwrap()
                .profile_name,
            "Zhipu GLM"
        );
        assert_eq!(
            load_multi_instance_manifest(temp.path())
                .unwrap()
                .debug_port,
            9456
        );
    }

    // 多开在 Windows 上曾经「窗口没出现、列表却显示运行中」：现在状态由
    // worker 落盘的启动结果 + 端口判活共同决定，两者都要有测试守住。
    #[test]
    fn multi_instance_status_reports_worker_failure_instead_of_running() {
        let failed = CodexMultiLaunchResult {
            status: "failed".to_string(),
            message: "调试端口 9456 未就绪".to_string(),
            finished_at_ms: 12,
            debug_port: Some(9456),
            pid: None,
        };
        let ready = CodexMultiLaunchResult {
            status: "ready".to_string(),
            message: "Codex 多开实例已就绪。".to_string(),
            finished_at_ms: 13,
            debug_port: Some(9456),
            pid: Some(4321),
        };

        let degraded = CodexMultiLaunchResult {
            status: "ready_degraded".to_string(),
            message: "实例已就绪但增强未注入".to_string(),
            finished_at_ms: 14,
            debug_port: Some(9456),
            pid: Some(4321),
        };

        assert_eq!(multi_instance_status(true, Some(&failed)), "running");
        assert_eq!(multi_instance_status(false, Some(&failed)), "failed");
        // 注入降级要如实显示，不能当成就绪也不能当成失败。
        assert_eq!(
            multi_instance_status(false, Some(&degraded)),
            "running_degraded"
        );
        assert_eq!(multi_instance_status(false, Some(&ready)), "stopped");
        assert_eq!(
            multi_instance_status(false, Some(&degraded)),
            "running_degraded"
        );
        // 没有结果文件（worker 还没写、或用户自己关了窗口）不算失败。
        assert_eq!(multi_instance_status(false, None), "stopped");
    }

    #[test]
    fn multi_launch_result_roundtrips_through_instance_directory() {
        let temp = tempfile::tempdir().unwrap();
        let result = CodexMultiLaunchResult {
            status: "failed".to_string(),
            message: "Codex 多开实例启动后调试端口 9456 未就绪，Codex 可能已退出。".to_string(),
            finished_at_ms: 1234,
            debug_port: Some(9456),
            pid: None,
        };

        assert!(load_multi_launch_result(temp.path()).is_none());
        write_multi_launch_result(&temp.path().join(MULTI_LAUNCH_RESULT_FILE), &result).unwrap();
        let loaded = load_multi_launch_result(temp.path()).unwrap();
        assert_eq!(loaded.status, "failed");
        assert_eq!(loaded.debug_port, Some(9456));
        assert_eq!(loaded.message, result.message);
    }

    #[test]
    fn multi_launch_result_message_prefers_launcher_failure_detail() {
        let ready = multi_launch_result_message(true, 9456, None);
        assert!(ready.contains("已就绪"));

        let without_detail = multi_launch_result_message(false, 9456, None);
        assert!(without_detail.contains("9456"));
        assert!(without_detail.contains("可能已退出"));

        let with_detail = multi_launch_result_message(
            false,
            9456,
            Some(&chatgptx_core::status::LaunchStatus {
                status: "failed".to_string(),
                message: "该进程没有程序包标识符".to_string(),
                ..Default::default()
            }),
        );
        assert!(with_detail.contains("该进程没有程序包标识符"));
        assert!(with_detail.contains("Microsoft Store"));
    }

    // 旧版 manifest 里没有 launchResult 字段，升级后必须仍能读出来（默认 None）。
    #[test]
    fn legacy_multi_instance_manifest_without_launch_result_still_loads() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = serde_json::json!({
            "instanceId": "zhipu",
            "profileId": "profile-zhipu",
            "profileName": "Zhipu GLM",
            "pid": 1234,
            "debugPort": 9456,
            "helperPort": 9457,
            "status": "running",
            "running": true,
            "startedAtMs": 99,
            "isMain": false
        });
        let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        chatgptx_core::settings::atomic_write(&multi_instance_manifest_path(temp.path()), &bytes)
            .unwrap();

        let instance = load_multi_instance_manifest(temp.path()).unwrap();
        assert!(instance.launch_result.is_none());
        assert_eq!(instance.debug_port, 9456);
    }

    #[cfg(unix)]
    #[test]
    fn multi_worker_process_match_requires_worker_marker_and_options_path() {
        let options = std::env::temp_dir().join("chatgptx-worker-options-test.json");
        let pid = std::process::id();
        assert!(!multi_worker_process_matches(pid, &options));
    }

    #[test]
    fn multi_instance_catalog_pointer_is_absolute() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("model-catalogs")).unwrap();
        std::fs::write(
            temp.path().join("model-catalogs/catalog.json"),
            "{\"models\":[]}",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "model_catalog_json = \"model-catalogs/catalog.json\"\n",
        )
        .unwrap();

        absolutize_multi_instance_model_catalog(temp.path()).unwrap();

        let config = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
        let doc = config.parse::<toml_edit::DocumentMut>().unwrap();
        let pointer = doc["model_catalog_json"].as_str().unwrap();
        assert!(Path::new(pointer).is_absolute(), "{pointer}");
        assert!(Path::new(pointer).is_file(), "{pointer}");
    }

    #[test]
    fn multi_instance_onboarding_state_is_seeded_without_account_data() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join(".codex-global-state.json"),
            r#"{"electron-persisted-atom-state":{"keep":true}}"#,
        )
        .unwrap();

        seed_multi_instance_onboarding_state(temp.path()).unwrap();

        let state: Value = serde_json::from_slice(
            &std::fs::read(temp.path().join(".codex-global-state.json")).unwrap(),
        )
        .unwrap();
        let atoms = &state["electron-persisted-atom-state"];
        assert_eq!(atoms["keep"], true);
        assert_eq!(atoms["electron:onboarding-projectless-completed"], true);
        assert!(atoms["last_completed_onboarding"].is_number());
        assert!(state.get("auth").is_none());
    }

    #[test]
    fn multi_instance_removes_mcp_entries_without_transport() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("config.toml"),
            "[mcp_servers.stitch.http_headers]\nkey = \"redacted\"\n\n[mcp_servers.node_repl]\ncommand = \"node\"\n",
        )
        .unwrap();

        remove_invalid_multi_instance_mcp_servers(temp.path()).unwrap();

        let config = std::fs::read_to_string(temp.path().join("config.toml")).unwrap();
        assert!(!config.contains("stitch"));
        assert!(config.contains("[mcp_servers.node_repl]"));
    }
}
