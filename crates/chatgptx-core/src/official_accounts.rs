//! 多官方账号登录、管理、配额查询、切换、与第三方 API 自由绑定及启动代理配置。
//!
//! 参考 cockpit-tools 的 Codex OAuth 与启动代理机制设计，并与 ChatGPTX 的
//! `SettingsStore` / `RelayProfile` 体系无缝衔接。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;

use crate::official_login::{OFFICIAL_OAUTH_CLIENT_ID, OFFICIAL_OAUTH_TOKEN_URL};
use crate::settings::{BackendSettings, RelayMode, RelayProfile, SettingsStore};

pub const OFFICIAL_OAUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub const OFFICIAL_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const DEFAULT_OAUTH_PORT: u16 = 1455;
const FALLBACK_OAUTH_PORT: u16 = 1457;
const OAUTH_TIMEOUT_SECS: u64 = 300;

/// 官方账号的 OAuth Tokens。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialTokens {
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

/// 官方账号 5h / 周配额与计划信息。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialQuota {
    /// 5 小时主窗口剩余百分比 (0-100)
    #[serde(default)]
    pub hourly_percentage: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_reset_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_window_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_window_present: Option<bool>,
    /// 周窗口剩余百分比 (0-100)
    #[serde(default)]
    pub weekly_percentage: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_reset_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_window_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_window_present: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_review_percentage: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_review_reset_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_review_window_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 单个官方账号记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialAccount {
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    pub tokens: OfficialTokens,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<OfficialQuota>,
    /// 自由绑定的第三方 API 供应商 ID（RelayProfile.id）；None 表示不绑定第三方 API（纯官方模式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_relay_profile_id: Option<String>,
    /// 账号级出口代理模式：`"inherit"`（跟随全局） / `"direct"`（强制直连） / `"custom"`（指定代理）
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    /// 账号绑定的专属代理 URL（当 proxy_mode == "custom" 时生效）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress_proxy_url: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_used: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

pub fn default_proxy_mode() -> String {
    "inherit".to_string()
}

/// 常用代理节点资源项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialProxyResource {
    pub id: String,
    pub name: String,
    pub proxy_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 持久化在 `official-accounts.json` 中的完整状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialAccountsState {
    #[serde(default)]
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub accounts: Vec<OfficialAccount>,
    /// 全局 Codex 启动代理模式：`"inherit"` / `"direct"` / `"custom"`
    #[serde(default = "default_proxy_mode")]
    pub global_proxy_mode: String,
    /// 全局 Codex 启动代理 URL
    #[serde(default)]
    pub global_proxy_url: String,
    /// 常用代理节点池
    #[serde(default)]
    pub proxy_resources: Vec<OfficialProxyResource>,
    /// 供应商级代理绑定表：relay_profile_id -> 代理模式或 URL（`"direct"` 表示直连，`http://...` 表示自定义代理）
    #[serde(default)]
    pub relay_profile_proxies: HashMap<String, String>,
    /// 供应商级官方账号绑定表：relay_profile_id -> official_account_id
    /// 支持第三方 API 供应商自由绑定任意官方账号（切换该供应商时同时把该官方账号写入 auth.json）
    #[serde(default)]
    pub relay_profile_bound_accounts: HashMap<String, String>,
}

impl Default for OfficialAccountsState {
    fn default() -> Self {
        Self {
            active_account_id: None,
            accounts: Vec::new(),
            global_proxy_mode: default_proxy_mode(),
            global_proxy_url: String::new(),
            proxy_resources: Vec::new(),
            relay_profile_proxies: HashMap::new(),
            relay_profile_bound_accounts: HashMap::new(),
        }
    }
}

/// 前端展示用的官方账号视图（脱敏 token 明文，但保留元数据与状态）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialAccountView {
    pub id: String,
    pub email: String,
    pub label: Option<String>,
    pub user_id: Option<String>,
    pub plan_type: Option<String>,
    pub account_id: Option<String>,
    pub organization_id: Option<String>,
    pub account_name: Option<String>,
    pub has_refresh_token: bool,
    pub quota: Option<OfficialQuota>,
    pub bound_relay_profile_id: Option<String>,
    pub proxy_mode: String,
    pub egress_proxy_url: Option<String>,
    pub created_at: i64,
    pub last_used: i64,
    pub tags: Vec<String>,
    pub is_active: bool,
}

impl OfficialAccount {
    pub fn to_view(&self, active_id: Option<&str>) -> OfficialAccountView {
        OfficialAccountView {
            id: self.id.clone(),
            email: self.email.clone(),
            label: self.label.clone(),
            user_id: self.user_id.clone(),
            plan_type: self.plan_type.clone(),
            account_id: self.account_id.clone(),
            organization_id: self.organization_id.clone(),
            account_name: self.account_name.clone(),
            has_refresh_token: self
                .tokens
                .refresh_token
                .as_ref()
                .is_some_and(|t| !t.trim().is_empty()),
            quota: self.quota.clone(),
            bound_relay_profile_id: self.bound_relay_profile_id.clone(),
            proxy_mode: normalize_proxy_mode(&self.proxy_mode),
            egress_proxy_url: self.egress_proxy_url.clone(),
            created_at: self.created_at,
            last_used: self.last_used,
            tags: self.tags.clone(),
            is_active: active_id == Some(self.id.as_str()),
        }
    }
}

/// 返回给前端的完整多账号与代理管理快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialAccountsSnapshot {
    pub active_account_id: Option<String>,
    pub live_account_email: Option<String>,
    pub live_account_id: Option<String>,
    pub accounts: Vec<OfficialAccountView>,
    pub global_proxy_mode: String,
    pub global_proxy_url: String,
    pub effective_launch_proxy: String,
    pub proxy_resources: Vec<OfficialProxyResource>,
    pub relay_profile_proxies: HashMap<String, String>,
    pub relay_profile_bound_accounts: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthLoginStartResponse {
    pub login_id: String,
    pub auth_url: String,
    pub callback_port: u16,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConnectivityResult {
    pub reachable: bool,
    pub proxy_url: String,
    pub latency_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub detail: String,
}

/// Codex 进程启动时的代理注入策略（与 cockpit-tools `CodexLaunchProxy` 对齐）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexLaunchProxy {
    /// 继承系统环境，不额外注入代理参数
    Inherit,
    /// 强制直连：注入 `--no-proxy-server` 并清空代理环境变量
    Direct,
    /// 显式代理：注入 `--proxy-server` / `--proxy-bypass-list` 与 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`
    Proxy(String),
}

impl CodexLaunchProxy {
    pub fn describe(&self) -> String {
        match self {
            Self::Inherit => "inherit".to_string(),
            Self::Direct => "direct".to_string(),
            Self::Proxy(url) => url.clone(),
        }
    }
}

pub fn normalize_proxy_mode(mode: &str) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "direct" => "direct".to_string(),
        "custom" => "custom".to_string(),
        _ => "inherit".to_string(),
    }
}

pub fn normalize_proxy_url_input(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("代理地址不能为空");
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let parsed = Url::parse(&with_scheme).map_err(|e| anyhow::anyhow!("代理地址格式无效：{e}"))?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https" | "socks5" | "socks5h") {
        anyhow::bail!("不支持的代理协议「{scheme}」，仅支持 http / https / socks5 / socks5h");
    }
    if parsed.host_str().unwrap_or_default().is_empty() {
        anyhow::bail!("代理地址缺少主机名");
    }
    Ok(with_scheme)
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 将 Unix 秒时间戳格式化为官方 Codex `AuthDotJson.last_refresh` 所需的 RFC3339 UTC 字符串（`YYYY-MM-DDTHH:MM:SSZ`）。
pub fn unix_seconds_to_rfc3339(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;

    // Civil-from-days (Howard Hinnant algorithm)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn jwt_expiration_seconds(token: &str) -> Option<i64> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let payload_b64 = token.trim().split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE
                .decode(payload_b64.as_bytes())
                .ok()
        })?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp").and_then(Value::as_i64)
}

pub fn is_access_token_expired_or_near_expiry(token: &str) -> bool {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return true;
    }
    let Some(exp) = jwt_expiration_seconds(trimmed) else {
        return false;
    };
    exp <= now_seconds() + 300
}

#[cfg(all(target_os = "macos", not(test)))]
const CODEX_KEYCHAIN_SERVICE: &str = "Codex Auth";

#[cfg(all(target_os = "macos", not(test)))]
fn build_codex_keychain_account(codex_home: &Path) -> String {
    let resolved = std::fs::canonicalize(codex_home).unwrap_or_else(|_| codex_home.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(resolved.to_string_lossy().as_bytes());
    let digest_hex = format!("{:x}", hasher.finalize());
    format!("cli|{}", &digest_hex[..16])
}

#[cfg(all(target_os = "macos", not(test)))]
fn read_codex_keychain_raw_json(codex_home: &Path) -> Option<String> {
    let account = build_codex_keychain_account(codex_home);
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            CODEX_KEYCHAIN_SERVICE,
            "-a",
            &account,
            "-w",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!secret.is_empty()).then_some(secret)
}

#[cfg(not(all(target_os = "macos", not(test))))]
fn read_codex_keychain_raw_json(_codex_home: &Path) -> Option<String> {
    None
}

#[cfg(all(target_os = "macos", not(test)))]
fn sync_codex_keychain_if_present(codex_home: &Path, auth_json: &str) {
    let account = build_codex_keychain_account(codex_home);
    let exists = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            CODEX_KEYCHAIN_SERVICE,
            "-a",
            &account,
        ])
        .output()
        .is_ok_and(|out| out.status.success());
    if !exists {
        return;
    }
    let compact = serde_json::from_str::<Value>(auth_json)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_else(|| auth_json.trim().to_string());
    let _ = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            CODEX_KEYCHAIN_SERVICE,
            "-a",
            &account,
            "-w",
            &compact,
        ])
        .output();
}

#[cfg(not(all(target_os = "macos", not(test))))]
fn sync_codex_keychain_if_present(_codex_home: &Path, _auth_json: &str) {}

fn write_auth_json_and_sync_store(codex_home: &Path, auth_json: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(codex_home)?;
    let auth_path = codex_home.join("auth.json");
    crate::settings::atomic_write(&auth_path, auth_json.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&auth_path, std::fs::Permissions::from_mode(0o600));
    }
    sync_codex_keychain_if_present(codex_home, auth_json);
    Ok(())
}
fn state_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn accounts_file_path() -> PathBuf {
    // 跟随 default_settings_path 所在目录，保证单元测试设置 set_settings_path_for_tests 时自动隔离
    if let Some(parent) = crate::paths::default_settings_path().parent() {
        return parent.join("official-accounts.json");
    }
    crate::paths::default_official_accounts_path()
}

pub fn load_official_accounts_state() -> OfficialAccountsState {
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    load_state_unlocked()
}

fn load_state_unlocked() -> OfficialAccountsState {
    let path = accounts_file_path();
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return OfficialAccountsState::default();
    };
    if contents.trim().is_empty() {
        return OfficialAccountsState::default();
    }
    serde_json::from_str::<OfficialAccountsState>(&contents).unwrap_or_default()
}

pub fn save_official_accounts_state(state: &OfficialAccountsState) -> anyhow::Result<()> {
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    save_state_unlocked(state)
}

fn save_state_unlocked(state: &OfficialAccountsState) -> anyhow::Result<()> {
    let path = accounts_file_path();
    let bytes = serde_json::to_vec_pretty(state).context("序列化官方账号状态失败")?;
    crate::settings::atomic_write(&path, &bytes)
}

/// 生成前端快照，并在首次加载时自动把本机当前 `~/.codex/auth.json` 中的官方登录（若有）同步进账号列表。
pub fn get_official_accounts_snapshot() -> OfficialAccountsSnapshot {
    let codex_home = crate::codex_home::default_codex_home_dir();
    let settings = SettingsStore::default().load().unwrap_or_default();
    get_official_accounts_snapshot_with(&codex_home, &settings)
}

pub fn get_official_accounts_snapshot_with(
    codex_home: &Path,
    settings: &BackendSettings,
) -> OfficialAccountsSnapshot {
    let state_file_exists = accounts_file_path().exists();
    let mut state = load_official_accounts_state();
    let live_info = read_live_chatgpt_account_from_home(codex_home);

    // 仅在状态文件首次创建前自动从本机 auth.json 导入一次；
    // 一旦状态文件已存在（包括用户主动删除了最后一个账号），不再自动把本机账号塞回列表。
    if !state_file_exists {
        if let Some(live) = &live_info {
            let mut imported = live.clone();
            state.active_account_id = Some(imported.id.clone());
            imported.last_used = now_seconds();
            state.accounts.push(imported);
        }
        let _ = save_official_accounts_state(&state);
    }
    let active_id = state.active_account_id.clone();
    let effective_proxy = resolve_codex_launch_proxy_from(&state, settings);
    let views = state
        .accounts
        .iter()
        .map(|acc| acc.to_view(active_id.as_deref()))
        .collect();

    OfficialAccountsSnapshot {
        active_account_id: active_id,
        live_account_email: live_info.as_ref().map(|a| a.email.clone()),
        live_account_id: live_info.as_ref().and_then(|a| a.account_id.clone()),
        accounts: views,
        global_proxy_mode: normalize_proxy_mode(&state.global_proxy_mode),
        global_proxy_url: state.global_proxy_url.clone(),
        effective_launch_proxy: effective_proxy.describe(),
        proxy_resources: state.proxy_resources.clone(),
        relay_profile_proxies: state.relay_profile_proxies.clone(),
        relay_profile_bound_accounts: state.relay_profile_bound_accounts.clone(),
    }
}

fn find_matching_account_index(
    accounts: &[OfficialAccount],
    candidate: &OfficialAccount,
) -> Option<usize> {
    accounts.iter().position(|existing| {
        if let (Some(a), Some(b)) = (
            existing.account_id.as_deref().filter(|s| !s.is_empty()),
            candidate.account_id.as_deref().filter(|s| !s.is_empty()),
        ) {
            if a == b {
                // 若同时有 user_id 则也比对 user_id（支持 Team 下多成员区分）
                if let (Some(u1), Some(u2)) = (
                    existing.user_id.as_deref().filter(|s| !s.is_empty()),
                    candidate.user_id.as_deref().filter(|s| !s.is_empty()),
                ) {
                    return u1 == u2;
                }
                return true;
            }
        }
        !existing.email.is_empty()
            && !candidate.email.is_empty()
            && existing.email.eq_ignore_ascii_case(&candidate.email)
            && existing.account_id == candidate.account_id
    })
}

/// 从 JWT Token 解析用户信息与 Auth Claims。
#[derive(Debug, Clone, Default)]
pub struct ParsedJwtInfo {
    pub email: Option<String>,
    pub user_id: Option<String>,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub organization_id: Option<String>,
    pub account_name: Option<String>,
}

pub fn parse_jwt_info(token: &str) -> ParsedJwtInfo {
    let Some(payload_b64) = token.split('.').nth(1) else {
        return ParsedJwtInfo::default();
    };
    let decoded = URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE
                .decode(payload_b64.as_bytes())
                .ok()
        });
    let Some(bytes) = decoded else {
        return ParsedJwtInfo::default();
    };
    let Ok(claims) = serde_json::from_slice::<Value>(&bytes) else {
        return ParsedJwtInfo::default();
    };

    let email = claims
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            claims
                .pointer("/https:~1~1api.openai.com~1profile/email")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
        });

    let auth_obj = claims.get("https://api.openai.com/auth");
    let account_id = auth_obj
        .and_then(|a| a.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let user_id = auth_obj
        .and_then(|a| {
            a.get("chatgpt_user_id")
                .or_else(|| a.get("user_id"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            claims
                .get("sub")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
        });
    let plan_type = auth_obj
        .and_then(|a| a.get("chatgpt_plan_type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let mut organization_id = None;
    let mut account_name = None;
    if let Some(orgs) = auth_obj
        .and_then(|a| a.get("organizations"))
        .and_then(Value::as_array)
    {
        let chosen = orgs
            .iter()
            .find(|o| o.get("is_default").and_then(Value::as_bool) == Some(true))
            .or_else(|| orgs.first());
        if let Some(org) = chosen {
            organization_id = org
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            account_name = org
                .get("title")
                .or_else(|| org.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
        }
    }

    ParsedJwtInfo {
        email,
        user_id,
        account_id,
        plan_type,
        organization_id,
        account_name,
    }
}

/// 从 `OfficialTokens` 构建或合并出 `OfficialAccount`。
pub fn build_account_from_tokens(
    tokens: OfficialTokens,
    explicit_account_id: Option<String>,
    label: Option<String>,
) -> anyhow::Result<OfficialAccount> {
    if tokens.access_token.trim().is_empty() && tokens.id_token.trim().is_empty() {
        anyhow::bail!("官方账号凭据缺少 access_token 和 id_token");
    }
    let id_info = parse_jwt_info(&tokens.id_token);
    let access_info = parse_jwt_info(&tokens.access_token);

    let email = id_info
        .email
        .or(access_info.email)
        .unwrap_or_else(|| "chatgpt-user@openai.com".to_string());
    let account_id = explicit_account_id
        .filter(|s| !s.trim().is_empty())
        .or(id_info.account_id)
        .or(access_info.account_id);
    let user_id = id_info.user_id.or(access_info.user_id);
    let plan_type = id_info.plan_type.or(access_info.plan_type);
    let organization_id = id_info.organization_id.or(access_info.organization_id);
    let account_name = id_info.account_name.or(access_info.account_name);

    let id = generate_stable_account_storage_id(&email, account_id.as_deref(), user_id.as_deref());
    let now = now_seconds();

    Ok(OfficialAccount {
        id,
        email,
        label: label.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        user_id,
        plan_type,
        account_id,
        organization_id,
        account_name,
        tokens,
        quota: None,
        bound_relay_profile_id: None,
        proxy_mode: default_proxy_mode(),
        egress_proxy_url: None,
        created_at: now,
        last_used: now,
        tags: Vec::new(),
    })
}

fn generate_stable_account_storage_id(
    email: &str,
    account_id: Option<&str>,
    user_id: Option<&str>,
) -> String {
    let mut seed = email.trim().to_ascii_lowercase();
    if let Some(acc) = account_id.map(str::trim).filter(|s| !s.is_empty()) {
        seed.push('|');
        seed.push_str(acc);
    }
    if let Some(uid) = user_id.map(str::trim).filter(|s| !s.is_empty()) {
        seed.push('|');
        seed.push_str(uid);
    }
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    let short_hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("codex_acct_{short_hex}")
}

/// 插入或更新官方账号，保留已有的绑定配置、标签、备注与配额缓存。
pub fn upsert_official_account(incoming: OfficialAccount, set_active: bool) -> anyhow::Result<OfficialAccount> {
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    let now = now_seconds();

    let saved = if let Some(idx) = find_matching_account_index(&state.accounts, &incoming) {
        let existing = &mut state.accounts[idx];
        existing.email = incoming.email;
        if incoming.label.is_some() {
            existing.label = incoming.label;
        }
        if incoming.user_id.is_some() {
            existing.user_id = incoming.user_id;
        }
        if incoming.plan_type.is_some() {
            existing.plan_type = incoming.plan_type;
        }
        if incoming.account_id.is_some() {
            existing.account_id = incoming.account_id;
        }
        if incoming.organization_id.is_some() {
            existing.organization_id = incoming.organization_id;
        }
        if incoming.account_name.is_some() {
            existing.account_name = incoming.account_name;
        }
        existing.tokens = incoming.tokens;
        if incoming.quota.is_some() {
            existing.quota = incoming.quota;
        }
        existing.last_used = now;
        existing.clone()
    } else {
        let mut acct = incoming;
        if acct.created_at == 0 {
            acct.created_at = now;
        }
        acct.last_used = now;
        state.accounts.push(acct.clone());
        acct
    };

    if set_active || state.active_account_id.is_none() {
        state.active_account_id = Some(saved.id.clone());
    }
    save_state_unlocked(&state)?;
    Ok(saved)
}

/// 从指定 `CODEX_HOME`（`auth.json` 及 macOS Keychain）读取当前官方账号（如果是 ChatGPT OAuth 登录态）。
pub fn read_live_chatgpt_account_from_home(codex_home: &Path) -> Option<OfficialAccount> {
    let auth_path = codex_home.join("auth.json");
    let file_acct = std::fs::read_to_string(auth_path)
        .ok()
        .and_then(|contents| parse_account_from_auth_json_str(&contents, None).ok());
    let keychain_acct = read_codex_keychain_raw_json(codex_home)
        .and_then(|contents| parse_account_from_auth_json_str(&contents, None).ok());

    match (file_acct, keychain_acct) {
        (Some(f), Some(k)) => {
            // 如果两者是同一个账号，取 access_token 过期时间更晚（更新一代）的那份
            let f_exp = jwt_expiration_seconds(&f.tokens.access_token).unwrap_or(0);
            let k_exp = jwt_expiration_seconds(&k.tokens.access_token).unwrap_or(0);
            if k_exp > f_exp {
                Some(k)
            } else {
                Some(f)
            }
        }
        (Some(f), None) => Some(f),
        (None, Some(k)) => Some(k),
        (None, None) => None,
    }
}

/// 在切号前将当前 `CODEX_HOME`（`auth.json` / Keychain）中已由 Codex 客户端轮换的最新 Token 回写到账号库，
/// 防止单次 `refresh_token` 被丢弃导致下次切回该账号时提示重新登录。
pub fn sync_live_account_tokens_to_store(codex_home: &Path) {
    let Some(live) = read_live_chatgpt_account_from_home(codex_home) else {
        return;
    };
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    let Some(idx) = find_matching_account_index(&state.accounts, &live) else {
        return;
    };
    let existing = &mut state.accounts[idx];
    let mut changed = false;
    let live_exp = jwt_expiration_seconds(&live.tokens.access_token).unwrap_or(0);
    let existing_exp = jwt_expiration_seconds(&existing.tokens.access_token).unwrap_or(0);
    if !live.tokens.access_token.trim().is_empty()
        && live.tokens.access_token != existing.tokens.access_token
        && live_exp >= existing_exp
    {
        existing.tokens.access_token = live.tokens.access_token;
        changed = true;
    }
    if !live.tokens.id_token.trim().is_empty()
        && live.tokens.id_token != existing.tokens.id_token
        && live_exp >= existing_exp
    {
        existing.tokens.id_token = live.tokens.id_token;
        changed = true;
    }
    if let Some(rt) = live
        .tokens
        .refresh_token
        .filter(|s| !s.trim().is_empty())
    {
        if existing.tokens.refresh_token.as_deref() != Some(rt.as_str()) && live_exp >= existing_exp
        {
            existing.tokens.refresh_token = Some(rt);
            changed = true;
        }
    }
    if changed {
        let _ = save_state_unlocked(&state);
    }
}
/// 从 `auth.json` 格式或包含 `tokens` / `access_token` 的 JSON 文本解析官方账号。
pub fn parse_account_from_auth_json_str(
    raw: &str,
    label: Option<String>,
) -> anyhow::Result<OfficialAccount> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("导入内容不能为空");
    }

    // 如果直接是一段 JWT (eyJ...)，当作 access_token 导入
    if trimmed.starts_with("eyJ") && trimmed.split('.').count() >= 3 && !trimmed.contains('{') {
        let tokens = OfficialTokens {
            id_token: trimmed.to_string(),
            access_token: trimmed.to_string(),
            refresh_token: None,
        };
        return build_account_from_tokens(tokens, None, label);
    }

    let value: Value = serde_json::from_str(trimmed).context("JSON 解析失败")?;
    parse_account_from_json_value(&value, label)
}

pub fn parse_account_from_json_value(
    value: &Value,
    label: Option<String>,
) -> anyhow::Result<OfficialAccount> {
    // 兼容两种结构：
    // 1. Codex auth.json: { "auth_mode": "chatgpt", "tokens": { "id_token": ..., "access_token": ..., "refresh_token": ..., "account_id": ... } }
    // 2. 直接导出或 cockpit-tools 账号对象: { "email": ..., "tokens": { ... }, "account_id": ... } 或顶层就是 tokens
    let tokens_node = value.get("tokens").unwrap_or(value);
    let access_token = tokens_node
        .get("access_token")
        .or_else(|| tokens_node.get("accessToken"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let id_token = tokens_node
        .get("id_token")
        .or_else(|| tokens_node.get("idToken"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| access_token.clone());
    let refresh_token = tokens_node
        .get("refresh_token")
        .or_else(|| tokens_node.get("refreshToken"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let explicit_account_id = tokens_node
        .get("account_id")
        .or_else(|| tokens_node.get("accountId"))
        .or_else(|| value.get("account_id"))
        .or_else(|| value.get("accountId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let explicit_label = label.or_else(|| {
        value
            .get("label")
            .or_else(|| value.get("accountName"))
            .or_else(|| value.get("account_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    });

    let mut account = build_account_from_tokens(
        OfficialTokens {
            id_token,
            access_token,
            refresh_token,
        },
        explicit_account_id,
        explicit_label,
    )?;

    if let Some(bound) = value
        .get("boundRelayProfileId")
        .or_else(|| value.get("bound_relay_profile_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        account.bound_relay_profile_id = Some(bound.to_string());
    }
    if let Some(proxy_url) = value
        .get("egressProxyUrl")
        .or_else(|| value.get("egress_proxy_url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        account.egress_proxy_url = Some(proxy_url.to_string());
        account.proxy_mode = "custom".to_string();
    }
    if let Some(mode) = value
        .get("proxyMode")
        .or_else(|| value.get("proxy_mode"))
        .and_then(Value::as_str)
    {
        account.proxy_mode = normalize_proxy_mode(mode);
    }

    Ok(account)
}

/// 将指定官方账号序列化为 Codex `auth.json` 的 JSON 对象（与官方 `AuthDotJson` 及 `cockpit-tools` 严格对齐）。
pub fn build_auth_json_for_account(
    account: &OfficialAccount,
    existing_auth_contents: Option<&str>,
    preserve_api_key: bool,
) -> anyhow::Result<String> {
    let mut root = existing_auth_contents
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    // 清理可能干扰官方客户端识别的旧字段
    for stale_key in [
        "type",
        "email",
        "account_email",
        "account_name",
        "account_id",
        "chatgpt_account_id",
        "user_id",
        "personal_access_token",
        "agent_identity",
        "base_url",
        "api_base_url",
    ] {
        root.remove(stale_key);
    }

    if !preserve_api_key {
        root.insert("OPENAI_API_KEY".to_string(), Value::Null);
    } else if !root
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
    {
        root.insert("OPENAI_API_KEY".to_string(), Value::Null);
    }

    let id_token = if account.tokens.id_token.trim().is_empty() {
        account.tokens.access_token.trim()
    } else {
        account.tokens.id_token.trim()
    };
    let refresh_token = account
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let resolved_account_id = account
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| parse_jwt_info(id_token).account_id)
        .or_else(|| parse_jwt_info(&account.tokens.access_token).account_id);

    let mut tokens_map = serde_json::Map::new();
    tokens_map.insert("id_token".to_string(), json!(id_token));
    tokens_map.insert(
        "access_token".to_string(),
        json!(account.tokens.access_token.trim()),
    );
    // Codex CLI 的 AuthDotJson 解析要求 refresh_token 字段始终存在
    tokens_map.insert("refresh_token".to_string(), json!(refresh_token));
    if let Some(acc_id) = resolved_account_id {
        tokens_map.insert("account_id".to_string(), json!(acc_id));
    }

    root.insert("auth_mode".to_string(), json!("chatgpt"));
    root.insert("tokens".to_string(), Value::Object(tokens_map));
    // 官方 Codex `AuthDotJson.last_refresh` 是 Option<DateTime<Utc>>，必须是 RFC3339 字符串！
    root.insert(
        "last_refresh".to_string(),
        json!(unix_seconds_to_rfc3339(now_seconds())),
    );

    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&Value::Object(root))?
    ))
}

/// 从本机 `~/.codex/auth.json`（或 macOS Keychain）导入官方账号并保存。
pub fn import_official_account_from_local_auth(
    codex_home: &Path,
    label: Option<String>,
) -> anyhow::Result<OfficialAccount> {
    if let Some(mut account) = read_live_chatgpt_account_from_home(codex_home) {
        if let Some(lbl) = label.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            account.label = Some(lbl);
        }
        return upsert_official_account(account, true);
    }
    let auth_path = codex_home.join("auth.json");
    let contents = std::fs::read_to_string(&auth_path)
        .with_context(|| format!("未找到或无法读取 {}", auth_path.display()))?;
    let account = parse_account_from_auth_json_str(&contents, label)
        .context("当前本机不包含有效的 ChatGPT 官方 OAuth 登录凭据")?;
    upsert_official_account(account, true)
}

/// 通过 JSON（支持单个账号对象、`auth.json`、或账号数组）或 Refresh Token 导入官方账号。
pub async fn import_official_accounts_from_input(
    input: &str,
    label: Option<String>,
) -> anyhow::Result<Vec<OfficialAccount>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("导入内容不能为空");
    }

    // 1. 如果是 JSON 数组
    if trimmed.starts_with('[') {
        let items: Vec<Value> = serde_json::from_str(trimmed).context("解析账号 JSON 数组失败")?;
        let mut imported = Vec::new();
        for item in items {
            let acct = parse_account_from_json_value(&item, label.clone())?;
            let saved = upsert_official_account(acct, imported.is_empty())?;
            imported.push(saved);
        }
        if imported.is_empty() {
            anyhow::bail!("JSON 数组中没有可导入的账号");
        }
        return Ok(imported);
    }

    // 2. 如果是 JSON 对象
    if trimmed.starts_with('{') {
        let value: Value = serde_json::from_str(trimmed).context("解析 JSON 失败")?;
        if let Some(arr) = value.get("accounts").and_then(Value::as_array) {
            let mut imported = Vec::new();
            for item in arr {
                let acct = parse_account_from_json_value(item, label.clone())?;
                let saved = upsert_official_account(acct, imported.is_empty())?;
                imported.push(saved);
            }
            if !imported.is_empty() {
                return Ok(imported);
            }
        }
        let acct = parse_account_from_json_value(&value, label)?;
        let saved = upsert_official_account(acct, true)?;
        return Ok(vec![saved]);
    }

    // 3. 如果是 JWT (eyJ...)
    if trimmed.starts_with("eyJ") && trimmed.split('.').count() >= 3 {
        let acct = parse_account_from_auth_json_str(trimmed, label)?;
        let saved = upsert_official_account(acct, true)?;
        return Ok(vec![saved]);
    }

    // 4. 否则按 Refresh Token 尝试向 OpenAI OAuth 端点兑换完整 Token
    let tokens = exchange_refresh_token_for_official_tokens(trimmed, None).await?;
    let acct = build_account_from_tokens(tokens, None, label)?;
    let saved = upsert_official_account(acct, true)?;
    Ok(vec![saved])
}

/// 导出指定或全部官方账号为 JSON 字符串（含完整 tokens，供跨设备迁移备份）。
pub fn export_official_accounts_json(account_ids: Option<&[String]>) -> anyhow::Result<String> {
    let state = load_official_accounts_state();
    let list: Vec<&OfficialAccount> = match account_ids {
        Some(ids) if !ids.is_empty() => state
            .accounts
            .iter()
            .filter(|a| ids.iter().any(|id| id == &a.id))
            .collect(),
        _ => state.accounts.iter().collect(),
    };
    serde_json::to_string_pretty(&list).context("导出官方账号 JSON 失败")
}

/// 删除官方账号，并清理绑定关系。支持删除最后一个官方账号且不会自动从 auth.json 重新恢复。
pub fn delete_official_account(account_id: &str) -> anyhow::Result<()> {
    let target = account_id.trim();
    if target.is_empty() {
        anyhow::bail!("账号 ID 不能为空");
    }
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    let before = state.accounts.len();
    state
        .accounts
        .retain(|a| a.id != target && !a.email.eq_ignore_ascii_case(target));
    if state.accounts.len() == before {
        anyhow::bail!("未找到指定的官方账号：{target}");
    }
    if state
        .active_account_id
        .as_deref()
        .is_none_or(|active| !state.accounts.iter().any(|a| a.id == active))
    {
        state.active_account_id = state.accounts.first().map(|a| a.id.clone());
    }
    state
        .relay_profile_bound_accounts
        .retain(|_, bound_id| state.accounts.iter().any(|a| &a.id == bound_id));
    save_state_unlocked(&state)
}

/// 更新官方账号的备注名、第三方 API 绑定与代理配置。
pub fn update_official_account_meta(
    account_id: &str,
    label: Option<String>,
    bound_relay_profile_id: Option<String>,
    proxy_mode: Option<String>,
    egress_proxy_url: Option<String>,
) -> anyhow::Result<OfficialAccount> {
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    let account = state
        .accounts
        .iter_mut()
        .find(|a| a.id == account_id)
        .ok_or_else(|| anyhow::anyhow!("未找到官方账号：{account_id}"))?;

    if let Some(lbl) = label {
        let trimmed = lbl.trim().to_string();
        account.label = (!trimmed.is_empty()).then_some(trimmed);
    }
    account.bound_relay_profile_id = bound_relay_profile_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(mode) = proxy_mode {
        account.proxy_mode = normalize_proxy_mode(&mode);
    }
    if let Some(url) = egress_proxy_url {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            account.egress_proxy_url = None;
        } else {
            account.egress_proxy_url = Some(normalize_proxy_url_input(trimmed)?);
        }
    }

    let updated = account.clone();
    save_state_unlocked(&state)?;
    Ok(updated)
}

/// 设置第三方 API 供应商与官方账号的绑定关系（双向可查）。
pub fn bind_relay_profile_official_account(
    relay_profile_id: &str,
    official_account_id: Option<&str>,
) -> anyhow::Result<()> {
    let relay_id = relay_profile_id.trim();
    if relay_id.is_empty() {
        anyhow::bail!("供应商 ID 不能为空");
    }
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    match official_account_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(acct_id) => {
            if !state.accounts.iter().any(|a| a.id == acct_id) {
                anyhow::bail!("未找到要绑定的官方账号：{acct_id}");
            }
            state
                .relay_profile_bound_accounts
                .insert(relay_id.to_string(), acct_id.to_string());
        }
        None => {
            state.relay_profile_bound_accounts.remove(relay_id);
        }
    }
    save_state_unlocked(&state)
}

/// 设置供应商级启动代理（`None` 或 `"inherit"` 表示跟随全局，`"direct"` 表示直连，URL 表示自定义代理）。
pub fn set_relay_profile_launch_proxy(
    relay_profile_id: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<()> {
    let relay_id = relay_profile_id.trim();
    if relay_id.is_empty() {
        anyhow::bail!("供应商 ID 不能为空");
    }
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    match proxy_spec.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("inherit") => {
            state.relay_profile_proxies.remove(relay_id);
        }
        Some("direct") => {
            state
                .relay_profile_proxies
                .insert(relay_id.to_string(), "direct".to_string());
        }
        Some(raw_url) => {
            let normalized = normalize_proxy_url_input(raw_url)?;
            state
                .relay_profile_proxies
                .insert(relay_id.to_string(), normalized);
        }
    }
    save_state_unlocked(&state)
}

/// 更新全局启动代理配置与常用代理节点池。
pub fn update_global_launch_proxy_settings(
    global_proxy_mode: &str,
    global_proxy_url: &str,
    proxy_resources: Option<Vec<OfficialProxyResource>>,
) -> anyhow::Result<OfficialAccountsState> {
    let mode = normalize_proxy_mode(global_proxy_mode);
    let url = if global_proxy_url.trim().is_empty() {
        String::new()
    } else {
        normalize_proxy_url_input(global_proxy_url)?
    };
    if mode == "custom" && url.is_empty() {
        anyhow::bail!("开启自定义全局代理时，代理地址不能为空");
    }

    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    state.global_proxy_mode = mode;
    state.global_proxy_url = url;
    if let Some(resources) = proxy_resources {
        let mut normalized_resources = Vec::new();
        for mut item in resources {
            item.name = item.name.trim().to_string();
            if item.proxy_url.trim().is_empty() {
                continue;
            }
            item.proxy_url = normalize_proxy_url_input(&item.proxy_url)?;
            if item.id.trim().is_empty() {
                item.id = format!("proxy_{}", uuid::Uuid::new_v4().simple());
            }
            if item.name.is_empty() {
                item.name = item.proxy_url.clone();
            }
            normalized_resources.push(item);
        }
        state.proxy_resources = normalized_resources;
    }
    save_state_unlocked(&state)?;
    Ok(state)
}

/// 切换激活的官方账号，并按绑定关系同步写入 `~/.codex/auth.json` 和 `~/.codex/config.toml`。
///
/// 规则：
/// 1. 如果该官方账号绑定了某个第三方 API 供应商 (`bound_relay_profile_id`)：
///    - 将 `settings.active_relay_id` 切换为该第三方供应商并生成 `config.toml`；
///    - 同时将该官方账号的 OAuth `tokens` 合并写入 `~/.codex/auth.json`（保留 API Key 若需要），实现官方身份 + 第三方 API 组合。
/// 2. 如果该官方账号未绑定第三方供应商（纯官方模式）：
///    - 若当前激活的是纯 API 供应商，可自动切回官方模式 profile（若存在）或直接清理第三方路由，将官方账号 `tokens` 写入 `~/.codex/auth.json`。
/// 3. 同时同步更新所有官方登录模式 `RelayProfile` 快照中的 `auth_contents`，保证聚合代理或后续切换不丢登录态。
/// 切号前预检：若目标账号 `access_token` 已过期（或将在 5 分钟内过期）且持有 `refresh_token`，
/// 先向 OpenAI OAuth 端点刷新 Token 并持久化回账号库，确保写入 `auth.json` / Keychain 的凭据始终有效。
pub async fn prepare_official_account_for_switch(account_id: &str) -> anyhow::Result<OfficialAccount> {
    let account = {
        let state = load_official_accounts_state();
        state
            .accounts
            .into_iter()
            .find(|a| a.id == account_id)
            .ok_or_else(|| anyhow::anyhow!("未找到官方账号：{account_id}"))?
    };

    if !is_access_token_expired_or_near_expiry(&account.tokens.access_token) {
        return Ok(account);
    }

    let Some(refresh_tok) = account
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(account);
    };

    let proxy = effective_proxy_for_account(Some(&account));
    if let Ok(new_tokens) =
        exchange_refresh_token_for_official_tokens(refresh_tok, proxy.as_deref()).await
    {
        persist_account_tokens(account_id, &new_tokens);
        let state = load_official_accounts_state();
        if let Some(updated) = state.accounts.into_iter().find(|a| a.id == account_id) {
            return Ok(updated);
        }
    }
    Ok(account)
}

pub fn switch_official_account(
    account_id: &str,
    codex_home: &Path,
    store: &SettingsStore,
) -> anyhow::Result<BackendSettings> {
    // 1. 覆盖前先把当前 live（auth.json / Keychain）中已轮换的最新 Token 保存回账号库
    sync_live_account_tokens_to_store(codex_home);

    let mut state = load_official_accounts_state();
    let account_idx = state
        .accounts
        .iter()
        .position(|a| a.id == account_id)
        .ok_or_else(|| anyhow::anyhow!("未找到官方账号：{account_id}"))?;

    state.accounts[account_idx].last_used = now_seconds();
    let account = state.accounts[account_idx].clone();
    state.active_account_id = Some(account.id.clone());
    save_official_accounts_state(&state)?;

    let mut settings = store.load()?;
    let existing_live_auth = std::fs::read_to_string(codex_home.join("auth.json")).ok();

    if let Some(bound_relay_id) = account
        .bound_relay_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(relay_idx) = settings
            .relay_profiles
            .iter()
            .position(|p| p.id == bound_relay_id)
        {
            settings.active_relay_id = bound_relay_id.to_string();
            let preserve_api_key = settings.relay_profiles[relay_idx].relay_mode != RelayMode::Official
                || settings.relay_profiles[relay_idx].official_mix_api_key
                || !settings.relay_profiles[relay_idx].api_key.trim().is_empty();
            let base_auth = if !settings.relay_profiles[relay_idx].auth_contents.trim().is_empty() {
                Some(settings.relay_profiles[relay_idx].auth_contents.clone())
            } else {
                existing_live_auth.clone()
            };
            let auth_json = build_auth_json_for_account(
                &account,
                base_auth.as_deref(),
                preserve_api_key,
            )?;
            settings.relay_profiles[relay_idx].auth_contents = auth_json;

            let profile_clone = settings.relay_profiles[relay_idx].clone();
            let common = settings.relay_common_config_contents.clone();
            crate::relay_config::apply_relay_profile_to_home_with_switch_rules(
                codex_home,
                &profile_clone,
                &common,
            )?;
            let final_live = std::fs::read_to_string(codex_home.join("auth.json"))
                .ok()
                .or(base_auth);
            let merged_auth =
                build_auth_json_for_account(&account, final_live.as_deref(), preserve_api_key)?;
            write_auth_json_and_sync_store(codex_home, &merged_auth)?;
            store.save(&settings)?;
            return Ok(settings);
        }
    }

    // 未绑定特定第三方 API 供应商（纯官方模式）：
    // 将 active_relay_id 设为 "__official__"（使供应商配置页不显示使用中），
    // 彻底清理 config.toml 中残留的第三方 model / model_provider / openai_base_url / model_catalog_json，
    // 并将纯官方 OAuth 凭据写入 auth.json 与 Keychain。
    let official_auth_json =
        build_auth_json_for_account(&account, existing_live_auth.as_deref(), false)?;
    settings.active_relay_id = "__official__".to_string();

    let clean_official = RelayProfile {
        id: "__official__".to_string(),
        name: "官方登录".to_string(),
        relay_mode: RelayMode::Official,
        official_mix_api_key: false,
        auth_contents: official_auth_json.clone(),
        use_common_config: true,
        ..RelayProfile::default()
    };
    let common = settings.relay_common_config_contents.clone();
    let _ = crate::relay_config::apply_relay_profile_to_home_with_switch_rules(
        codex_home,
        &clean_official,
        &common,
    );
    // 额外确保 config.toml 根部残留的第三方 model / openai_base_url / model_catalog_json 被彻底移除
    let config_path = codex_home.join("config.toml");
    if let Ok(existing_cfg) = std::fs::read_to_string(&config_path) {
        if let Ok(mut doc) = existing_cfg.parse::<toml_edit::DocumentMut>() {
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
            let _ = crate::settings::atomic_write(&config_path, doc.to_string().as_bytes());
        }
    }

    write_auth_json_and_sync_store(codex_home, &official_auth_json)?;
    store.save(&settings)?;
    Ok(settings)
}

/// 当用户在「供应商配置」页单独切换到某个纯 API 供应商时调用：
/// 1. 先将当前 live 的官方 OAuth tokens（若已轮换）回写到账号库；
/// 2. 将 `OfficialAccountsState.active_account_id` 清空为 `None`（使官方账号页不再显示「使用中」）；
/// 3. 从 `CODEX_HOME/auth.json` 与 Keychain 中移除官方 OAuth `tokens`，只保留纯 API 的 `OPENAI_API_KEY`。
pub fn deactivate_official_account_for_pure_api(
    codex_home: &Path,
    api_key: &str,
) -> anyhow::Result<()> {
    sync_live_account_tokens_to_store(codex_home);
    {
        let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut state = load_state_unlocked();
        if state.active_account_id.is_some() {
            state.active_account_id = None;
            let _ = save_state_unlocked(&state);
        }
    }
    let trimmed_key = api_key.trim();
    if !trimmed_key.is_empty() {
        let pure_auth = serde_json::to_string_pretty(&json!({
            "OPENAI_API_KEY": trimmed_key,
        }))?;
        write_auth_json_and_sync_store(codex_home, &format!("{pure_auth}\n"))?;
    }
    Ok(())
}

/// 当在供应商页面切换或应用某个 `RelayProfile` 时，如果该供应商绑定了某个官方账号（或某个官方账号绑定了该供应商），
/// 将对应官方账号的 OAuth `tokens` 同步写入 `CODEX_HOME/auth.json` 与 Keychain。
pub fn sync_bound_official_account_on_relay_apply(
    codex_home: &Path,
    profile: &RelayProfile,
) -> anyhow::Result<Option<String>> {
    let mut state = load_official_accounts_state();
    let bound_account_id = state
        .relay_profile_bound_accounts
        .get(&profile.id)
        .cloned()
        .or_else(|| {
            state
                .accounts
                .iter()
                .find(|a| a.bound_relay_profile_id.as_deref() == Some(profile.id.as_str()))
                .map(|a| a.id.clone())
        });

    let Some(account_id) = bound_account_id else {
        return Ok(None);
    };

    let Some(account_idx) = state.accounts.iter().position(|a| a.id == account_id) else {
        return Ok(None);
    };

    state.accounts[account_idx].last_used = now_seconds();
    state.active_account_id = Some(account_id.clone());
    let account = state.accounts[account_idx].clone();
    let _ = save_official_accounts_state(&state);

    let auth_path = codex_home.join("auth.json");
    let existing_live = std::fs::read_to_string(&auth_path).ok();
    let preserve_api_key =
        profile.relay_mode != RelayMode::Official || profile.official_mix_api_key;
    let merged = build_auth_json_for_account(&account, existing_live.as_deref(), preserve_api_key)?;
    write_auth_json_and_sync_store(codex_home, &merged)?;
    Ok(Some(account.email))
}
// OAuth PKCE 登录流程（参考 cockpit-tools `codex_oauth.rs`）
// ============================================================================

#[derive(Debug)]
struct ActiveOAuthSession {
    login_id: String,
    port: u16,
    redirect_uri: String,
    code_verifier: String,
    state_token: String,
    code_rx: std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Receiver<String>>>>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

fn oauth_session_slot() -> &'static Mutex<Option<ActiveOAuthSession>> {
    static SLOT: OnceLock<Mutex<Option<ActiveOAuthSession>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn generate_pkce_verifier() -> String {
    let u1 = uuid::Uuid::new_v4().simple().to_string();
    let u2 = uuid::Uuid::new_v4().simple().to_string();
    format!("{u1}{u2}")
}

fn generate_pkce_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    URL_SAFE_NO_PAD.encode(digest)
}

fn build_oauth_authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let scope = "openid profile email offline_access";
    let params = [
        ("response_type", "code"),
        ("client_id", OFFICIAL_OAUTH_CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", scope),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", "codex_cli_rs"),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{OFFICIAL_OAUTH_AUTHORIZE_URL}?{query}")
}

fn urlencoding_encode(input: &str) -> String {
    url::form_urlencoded::byte_serialize(input.as_bytes()).collect()
}

/// 启动本地 OAuth PKCE 登录流程：绑定 127.0.0.1:1455（或备选端口）并返回授权链接。
pub async fn start_official_oauth_login() -> anyhow::Result<OAuthLoginStartResponse> {
    // 取消之前未完成的会话
    cancel_official_oauth_login();

    let mut listener_opt = None;
    for candidate_port in [DEFAULT_OAUTH_PORT, FALLBACK_OAUTH_PORT, 0] {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", candidate_port)).await {
            listener_opt = Some(listener);
            break;
        }
    }
    let listener = listener_opt.context("无法绑定本地 OAuth 回调端口")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{port}/auth/callback");

    let login_id = uuid::Uuid::new_v4().to_string();
    let code_verifier = generate_pkce_verifier();
    let code_challenge = generate_pkce_challenge(&code_verifier);
    let state_token = uuid::Uuid::new_v4().simple().to_string();
    let auth_url = build_oauth_authorize_url(&redirect_uri, &code_challenge, &state_token);

    let (code_tx, code_rx) = oneshot::channel::<String>();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    let expected_state = state_token.clone();

    tokio::spawn(async move {
        let mut code_tx = Some(code_tx);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(OAUTH_TIMEOUT_SECS);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    break;
                }
                _ = tokio::time::sleep_until(deadline) => {
                    break;
                }
                accept_res = listener.accept() => {
                    let Ok((mut stream, _)) = accept_res else {
                        continue;
                    };
                    let mut buf = vec![0u8; 4096];
                    let Ok(n) = stream.read(&mut buf).await else {
                        continue;
                    };
                    if n == 0 {
                        continue;
                    }
                    let req_str = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req_str.lines().next().unwrap_or_default();
                    let path_part = first_line.split_whitespace().nth(1).unwrap_or("/");
                    if !path_part.starts_with("/auth/callback") {
                        let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found";
                        let _ = stream.write_all(resp.as_bytes()).await;
                        continue;
                    }
                    let full_url = format!("http://localhost:{port}{path_part}");
                    if let Ok(parsed) = Url::parse(&full_url) {
                        let pairs: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
                        let state_ok = pairs.get("state").map(|s| s == &expected_state).unwrap_or(false);
                        if let Some(code) = pairs.get("code").filter(|c| !c.is_empty() && state_ok) {
                            let html = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>登录成功</title><style>body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#0f172a;color:#f8fafc}.card{background:#1e293b;padding:32px 40px;border-radius:16px;text-align:center;box-shadow:0 10px 30px rgba(0,0,0,.4)}h2{margin:0 0 12px;color:#38bdf8}p{margin:0;color:#94a3b8;font-size:14px}</style></head><body><div class="card"><h2>ChatGPT 官方账号授权成功</h2><p>已同步回 ChatGPTX，您可以关闭此浏览器标签页。</p></div><script>setTimeout(()=>window.close(),1500)</script></body></html>"#;
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                html.len(),
                                html
                            );
                            let _ = stream.write_all(resp.as_bytes()).await;
                            let _ = stream.flush().await;
                            if let Some(tx) = code_tx.take() {
                                let _ = tx.send(code.clone());
                            }
                            break;
                        }
                    }
                    let html = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body><h3>授权参数无效或 state 不匹配，请返回 ChatGPTX 重试。</h3></body></html>";
                    let resp = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                }
            }
        }
    });

    let session = ActiveOAuthSession {
        login_id: login_id.clone(),
        port,
        redirect_uri: redirect_uri.clone(),
        code_verifier,
        state_token,
        code_rx: std::sync::Arc::new(tokio::sync::Mutex::new(Some(code_rx))),
        cancel_tx: Some(cancel_tx),
    };
    *oauth_session_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(session);

    Ok(OAuthLoginStartResponse {
        login_id,
        auth_url,
        callback_port: port,
        redirect_uri,
    })
}

/// 等待浏览器回调完成 OAuth 登录并自动保存账号。
pub async fn wait_for_official_oauth_callback(
    login_id: &str,
    label: Option<String>,
    set_active: bool,
) -> anyhow::Result<OfficialAccount> {
    let (rx_arc, redirect_uri, code_verifier) = {
        let guard = oauth_session_slot()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let session = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("当前没有正在进行的 OAuth 登录会话"))?;
        if !login_id.trim().is_empty() && session.login_id != login_id {
            anyhow::bail!("OAuth 登录会话已变更，请重新发起登录");
        }
        (
            session.code_rx.clone(),
            session.redirect_uri.clone(),
            session.code_verifier.clone(),
        )
    };

    let rx = {
        let mut rx_guard = rx_arc.lock().await;
        rx_guard
            .take()
            .ok_or_else(|| anyhow::anyhow!("该 OAuth 会话已在等待回调中"))?
    };

    let code = tokio::time::timeout(Duration::from_secs(OAUTH_TIMEOUT_SECS), rx)
        .await
        .context("等待浏览器 OAuth 授权回调超时（5 分钟）")?
        .context("OAuth 授权会话已取消")?;

    let tokens = exchange_code_for_official_tokens(&code, &code_verifier, &redirect_uri).await?;
    cancel_official_oauth_login();

    let account = build_account_from_tokens(tokens, None, label)?;
    let saved = upsert_official_account(account, set_active)?;
    Ok(saved)
}

/// 手动提交浏览器回调链接（`http://localhost:1455/auth/callback?code=...&state=...`）或直接提交授权 `code` 完成登录。
pub async fn submit_official_oauth_callback_url(
    callback_url_or_code: &str,
    label: Option<String>,
    set_active: bool,
) -> anyhow::Result<OfficialAccount> {
    let trimmed = callback_url_or_code.trim();
    if trimmed.is_empty() {
        anyhow::bail!("回调链接或授权码不能为空");
    }

    let (redirect_uri, code_verifier, expected_state) = {
        let guard = oauth_session_slot()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let session = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("请先点击「发起浏览器 OAuth 登录」生成会话后再粘贴回调地址"))?;
        (
            session.redirect_uri.clone(),
            session.code_verifier.clone(),
            session.state_token.clone(),
        )
    };

    let code = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let parsed = Url::parse(trimmed).context("回调 URL 格式不正确")?;
        let pairs: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
        if let Some(err) = pairs.get("error") {
            let desc = pairs.get("error_description").cloned().unwrap_or_default();
            anyhow::bail!("OAuth 授权返回错误：{err} {desc}");
        }
        if let Some(state) = pairs.get("state") {
            if state != &expected_state {
                anyhow::bail!("回调 URL 中的 state 与当前登录会话不匹配，请使用最新生成的授权链接");
            }
        }
        pairs
            .get("code")
            .cloned()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("回调 URL 中未找到 code 参数"))?
    } else {
        trimmed.to_string()
    };

    let tokens = exchange_code_for_official_tokens(&code, &code_verifier, &redirect_uri).await?;
    cancel_official_oauth_login();

    let account = build_account_from_tokens(tokens, None, label)?;
    let saved = upsert_official_account(account, set_active)?;
    Ok(saved)
}

/// 取消当前未完成的 OAuth 登录监听。
pub fn cancel_official_oauth_login() {
    let mut guard = oauth_session_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(mut session) = guard.take() {
        if let Some(cancel) = session.cancel_tx.take() {
            let _ = cancel.send(());
        }
    }
}

fn build_http_client_with_optional_proxy(
    proxy_url: Option<&str>,
    timeout_secs: u64,
) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .user_agent(format!("ChatGPTX/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(timeout_secs));

    if let Some(raw) = proxy_url.map(str::trim).filter(|s| !s.is_empty()) {
        if raw.eq_ignore_ascii_case("direct") {
            builder = builder.no_proxy();
        } else {
            let normalized = normalize_proxy_url_input(raw)?;
            let proxy = reqwest::Proxy::all(&normalized)
                .with_context(|| format!("无效的代理地址 {normalized}"))?
                .no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
            builder = builder.proxy(proxy);
        }
    }
    builder.build().context("创建 HTTP 客户端失败")
}

fn effective_proxy_for_account(account: Option<&OfficialAccount>) -> Option<String> {
    let state = load_official_accounts_state();
    if let Some(acct) = account {
        match acct.proxy_mode.trim() {
            "direct" => return Some("direct".to_string()),
            "custom" => {
                if let Some(url) = acct
                    .egress_proxy_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    return Some(url.to_string());
                }
            }
            _ => {}
        }
    }
    match state.global_proxy_mode.trim() {
        "direct" => Some("direct".to_string()),
        "custom" if !state.global_proxy_url.trim().is_empty() => {
            Some(state.global_proxy_url.trim().to_string())
        }
        _ => {
            let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
            if crate::proxy::has_proxy_environment(&env_map) {
                None
            } else {
                crate::proxy::detect_system_proxy()
            }
        }
    }
}

async fn exchange_code_for_official_tokens(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<OfficialTokens> {
    let proxy = effective_proxy_for_account(None);
    let client = build_http_client_with_optional_proxy(proxy.as_deref(), 30)?;
    let resp = client
        .post(OFFICIAL_OAUTH_TOKEN_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", OFFICIAL_OAUTH_CLIENT_ID),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await
        .context("请求 OpenAI OAuth Token 端点失败")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("OAuth Token 兑换失败 ({status})：{body}");
    }
    parse_oauth_token_response(&body, None)
}

pub async fn exchange_refresh_token_for_official_tokens(
    refresh_token: &str,
    proxy_override: Option<&str>,
) -> anyhow::Result<OfficialTokens> {
    let fallback_proxy = effective_proxy_for_account(None);
    let proxy = proxy_override.or(fallback_proxy.as_deref());
    let client = build_http_client_with_optional_proxy(proxy, 30)?;
    let resp = client
        .post(OFFICIAL_OAUTH_TOKEN_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", OFFICIAL_OAUTH_CLIENT_ID),
            ("refresh_token", refresh_token.trim()),
            ("scope", "openid profile email"),
        ])
        .send()
        .await
        .context("请求刷新官方账号 Token 失败")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("刷新官方账号 Token 失败 ({status})：{body}");
    }
    parse_oauth_token_response(&body, Some(refresh_token.trim()))
}

fn parse_oauth_token_response(
    body: &str,
    previous_refresh_token: Option<&str>,
) -> anyhow::Result<OfficialTokens> {
    let value: Value = serde_json::from_str(body).context("解析 OAuth Token 响应 JSON 失败")?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("OAuth 响应缺少 access_token"))?
        .to_string();
    let id_token = value
        .get("id_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| access_token.clone());
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| previous_refresh_token.map(ToString::to_string));

    Ok(OfficialTokens {
        id_token,
        access_token,
        refresh_token,
    })
}

// ============================================================================
// 官方账号配额查询 (`https://chatgpt.com/backend-api/wham/usage`)
// ============================================================================

/// 刷新单个官方账号的 5h / 周配额，并在遇到 401/403 时自动使用 refresh_token 刷新后重试。
pub async fn refresh_official_account_quota(account_id: &str) -> anyhow::Result<OfficialQuota> {
    let account = {
        let state = load_official_accounts_state();
        state
            .accounts
            .into_iter()
            .find(|a| a.id == account_id)
            .ok_or_else(|| anyhow::anyhow!("未找到官方账号：{account_id}"))?
    };

    let proxy = effective_proxy_for_account(Some(&account));
    let client = build_http_client_with_optional_proxy(proxy.as_deref(), 20)?;

    let mut current_tokens = account.tokens.clone();
    let mut first_resp = send_wham_usage_request(
        &client,
        &current_tokens.access_token,
        account.account_id.as_deref(),
    )
    .await;

    let should_refresh = match &first_resp {
        Ok((status, _)) => {
            *status == reqwest::StatusCode::UNAUTHORIZED
                || *status == reqwest::StatusCode::FORBIDDEN
        }
        Err(_) => false,
    };

    if should_refresh {
        if let Some(refresh_tok) = current_tokens
            .refresh_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            if let Ok(new_tokens) =
                exchange_refresh_token_for_official_tokens(refresh_tok, proxy.as_deref()).await
            {
                current_tokens = new_tokens.clone();
                persist_account_tokens(account_id, &new_tokens);
                first_resp = send_wham_usage_request(
                    &client,
                    &current_tokens.access_token,
                    account.account_id.as_deref(),
                )
                .await;
            }
        }
    }

    let (status, body) = match first_resp {
        Ok(res) => res,
        Err(err) => {
            let failed_quota = OfficialQuota {
                updated_at: now_seconds(),
                error: Some(err.to_string()),
                ..OfficialQuota::default()
            };
            persist_account_quota(account_id, &failed_quota);
            return Err(err);
        }
    };

    if !status.is_success() {
        let msg = format!("配额查询返回 HTTP {status}");
        let failed_quota = OfficialQuota {
            updated_at: now_seconds(),
            error: Some(msg.clone()),
            ..OfficialQuota::default()
        };
        persist_account_quota(account_id, &failed_quota);
        anyhow::bail!("{msg}：{body}");
    }

    let quota = parse_wham_usage_payload(&body)?;
    persist_account_quota(account_id, &quota);
    Ok(quota)
}

async fn send_wham_usage_request(
    client: &reqwest::Client,
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> anyhow::Result<(reqwest::StatusCode, String)> {
    let mut req = client
        .get(OFFICIAL_USAGE_URL)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token.trim()),
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::REFERER, "https://chatgpt.com/")
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36",
        );

    if let Some(acc_id) = chatgpt_account_id.map(str::trim).filter(|s| !s.is_empty()) {
        req = req.header("ChatGPT-Account-Id", acc_id);
    }

    let resp = req.send().await.context("请求 ChatGPT 配额接口失败")?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text))
}

pub fn parse_wham_usage_payload(body: &str) -> anyhow::Result<OfficialQuota> {
    let value: Value = serde_json::from_str(body).context("解析配额响应 JSON 失败")?;
    let plan_type = value
        .get("plan_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let rate_limit = value.get("rate_limit");
    let primary = rate_limit.and_then(|r| r.get("primary_window"));
    let secondary = rate_limit.and_then(|r| r.get("secondary_window"));

    let (hourly_pct, hourly_reset, hourly_mins, hourly_present) = extract_window_metrics(primary);
    let (weekly_pct, weekly_reset, weekly_mins, weekly_present) = extract_window_metrics(secondary);

    let cr_limit = value.get("code_review_rate_limit");
    let cr_window = cr_limit
        .and_then(|r| r.get("primary_window"))
        .filter(|v| v.is_object())
        .or_else(|| {
            cr_limit
                .and_then(|r| r.get("secondary_window"))
                .filter(|v| v.is_object())
        });
    let (cr_pct, cr_reset, cr_mins, cr_present) = extract_window_metrics(cr_window);

    Ok(OfficialQuota {
        hourly_percentage: hourly_pct,
        hourly_reset_time: hourly_reset,
        hourly_window_minutes: hourly_mins,
        hourly_window_present: Some(hourly_present),
        weekly_percentage: weekly_pct,
        weekly_reset_time: weekly_reset,
        weekly_window_minutes: weekly_mins,
        weekly_window_present: Some(weekly_present),
        code_review_percentage: cr_present.then_some(cr_pct),
        code_review_reset_time: cr_present.then_some(cr_reset).flatten(),
        code_review_window_minutes: cr_present.then_some(cr_mins).flatten(),
        raw_data: Some(value),
        plan_type,
        updated_at: now_seconds(),
        error: None,
    })
}

fn extract_window_metrics(window: Option<&Value>) -> (i32, Option<i64>, Option<i64>, bool) {
    let Some(win) = window.filter(|v| v.is_object()) else {
        return (100, None, None, false);
    };
    let used_percent = win
        .get("used_percent")
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|i| i as f64))
        })
        .unwrap_or(0.0);
    let remaining = (100.0 - used_percent).round().clamp(0.0, 100.0) as i32;
    let reset_at = win.get("reset_at").and_then(Value::as_i64).or_else(|| {
        win.get("reset_after_seconds")
            .and_then(Value::as_i64)
            .map(|secs| now_seconds() + secs)
    });
    let window_minutes = win
        .get("limit_window_seconds")
        .and_then(Value::as_i64)
        .filter(|s| *s > 0)
        .map(|s| (s + 59) / 60);
    (remaining, reset_at, window_minutes, true)
}

fn persist_account_tokens(account_id: &str, tokens: &OfficialTokens) {
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    if let Some(acct) = state.accounts.iter_mut().find(|a| a.id == account_id) {
        acct.tokens = tokens.clone();
        let _ = save_state_unlocked(&state);
    }
}

fn persist_account_quota(account_id: &str, quota: &OfficialQuota) {
    let _guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state_unlocked();
    if let Some(acct) = state.accounts.iter_mut().find(|a| a.id == account_id) {
        if let Some(plan) = &quota.plan_type {
            acct.plan_type = Some(plan.clone());
        }
        acct.quota = Some(quota.clone());
        let _ = save_state_unlocked(&state);
    }
}

// ============================================================================
// Codex 启动代理注入解析与连通性测试（参考 cockpit-tools `process_codex_launch_proxy.rs`）
// ============================================================================

/// 根据当前激活的供应商、绑定的官方账号与全局代理配置，计算启动 Codex 时应注入的代理策略。
///
/// 优先级（由高到低）：
/// 1. 当前激活供应商（`active_relay_id`）绑定的专属代理（`relay_profile_proxies`）
/// 2. 当前激活官方账号（或供应商绑定的官方账号）配置的专属代理（`proxy_mode` / `egress_proxy_url`）
/// 3. 全局 Codex 启动代理（`global_proxy_mode` / `global_proxy_url`）
/// 4. 默认 `CodexLaunchProxy::Inherit`
pub fn resolve_codex_launch_proxy(settings: &BackendSettings) -> CodexLaunchProxy {
    let state = load_official_accounts_state();
    resolve_codex_launch_proxy_from(&state, settings)
}

pub fn resolve_codex_launch_proxy_from(
    state: &OfficialAccountsState,
    settings: &BackendSettings,
) -> CodexLaunchProxy {
    let active_relay_id = settings.active_relay_id.trim();

    // 1. 供应商级代理配置
    if !active_relay_id.is_empty() {
        if let Some(spec) = state
            .relay_profile_proxies
            .get(active_relay_id)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            if spec.eq_ignore_ascii_case("direct") {
                return CodexLaunchProxy::Direct;
            }
            if let Ok(normalized) = normalize_proxy_url_input(spec) {
                return CodexLaunchProxy::Proxy(normalized);
            }
        }
    }

    // 2. 官方账号级代理配置（优先取当前供应商绑定的官方账号，其次取当前激活的官方账号）
    let target_account_id = state
        .relay_profile_bound_accounts
        .get(active_relay_id)
        .cloned()
        .or_else(|| state.active_account_id.clone());

    if let Some(acct_id) = target_account_id {
        if let Some(acct) = state.accounts.iter().find(|a| a.id == acct_id) {
            match acct.proxy_mode.trim() {
                "direct" => return CodexLaunchProxy::Direct,
                "custom" => {
                    if let Some(url) = acct
                        .egress_proxy_url
                        .as_deref()
                        .and_then(|u| normalize_proxy_url_input(u).ok())
                    {
                        return CodexLaunchProxy::Proxy(url);
                    }
                }
                _ => {}
            }
        }
    }

    // 3. 全局代理配置
    match state.global_proxy_mode.trim() {
        "direct" => CodexLaunchProxy::Direct,
        "custom" => {
            if let Ok(url) = normalize_proxy_url_input(&state.global_proxy_url) {
                CodexLaunchProxy::Proxy(url)
            } else {
                CodexLaunchProxy::Inherit
            }
        }
        _ => CodexLaunchProxy::Inherit,
    }
}

/// 生成要注入给 Electron (Codex App) 的命令行参数。
///
/// 注意：
/// - `socks5h://` 转换为 Electron/Chromium 识别的 `socks5://`
/// - 若代理 URL 含有用户名/密码，不放入命令行参数（避免进程列表泄露且 Chromium 不支持命令行内嵌凭据），仅走环境变量注入
/// - 自动注入 `--proxy-bypass-list=localhost;127.0.0.1;[::1];<local>` 防止本地协议代理与 CDP 端口被劫持
pub fn build_electron_proxy_args(launch_proxy: &CodexLaunchProxy) -> Vec<String> {
    match launch_proxy {
        CodexLaunchProxy::Inherit => Vec::new(),
        CodexLaunchProxy::Direct => vec!["--no-proxy-server".to_string()],
        CodexLaunchProxy::Proxy(raw_url) => {
            let Ok(parsed) = Url::parse(raw_url) else {
                return Vec::new();
            };
            if !parsed.username().is_empty() || parsed.password().is_some() {
                // 含认证信息时仅走环境变量，不注入命令行参数
                return Vec::new();
            }
            let Some(host) = parsed.host_str() else {
                return Vec::new();
            };
            let scheme = match parsed.scheme().to_ascii_lowercase().as_str() {
                "socks5h" => "socks5".to_string(),
                other => other.to_string(),
            };
            let host_port = match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            };
            vec![
                format!("--proxy-server={scheme}://{host_port}"),
                "--proxy-bypass-list=localhost;127.0.0.1;[::1];<local>".to_string(),
            ]
        }
    }
}

/// 生成要注入给 Codex 子进程的环境变量（返回要设置的键值对与要移除的键名）。
pub fn build_process_proxy_env(
    launch_proxy: &CodexLaunchProxy,
) -> (Vec<(String, String)>, Vec<String>) {
    let proxy_keys = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ];
    let no_proxy_keys = ["NO_PROXY", "no_proxy"];

    match launch_proxy {
        CodexLaunchProxy::Inherit => (Vec::new(), Vec::new()),
        CodexLaunchProxy::Direct => {
            let mut set_vars = Vec::new();
            for key in no_proxy_keys {
                set_vars.push((key.to_string(), "*".to_string()));
            }
            let remove_vars = proxy_keys.iter().map(|k| k.to_string()).collect();
            (set_vars, remove_vars)
        }
        CodexLaunchProxy::Proxy(proxy_url) => {
            let mut set_vars = Vec::new();
            for key in proxy_keys {
                set_vars.push((key.to_string(), proxy_url.clone()));
            }
            for key in no_proxy_keys {
                set_vars.push((
                    key.to_string(),
                    "localhost,127.0.0.1,::1".to_string(),
                ));
            }
            (set_vars, Vec::new())
        }
    }
}

/// 测试代理节点的连通性与延迟。
pub async fn test_proxy_connectivity(proxy_url: &str) -> anyhow::Result<ProxyConnectivityResult> {
    let normalized = if proxy_url.trim().eq_ignore_ascii_case("direct") {
        "direct".to_string()
    } else {
        normalize_proxy_url_input(proxy_url)?
    };

    let client = build_http_client_with_optional_proxy(Some(&normalized), 10)?;
    let started = std::time::Instant::now();
    let response = client
        .get("https://chatgpt.com/backend-api/codex")
        .send()
        .await;

    match response {
        Ok(resp) => {
            let elapsed = started.elapsed().as_millis() as u64;
            let code = resp.status().as_u16();
            Ok(ProxyConnectivityResult {
                reachable: true,
                proxy_url: normalized,
                latency_ms: Some(elapsed),
                status_code: Some(code),
                detail: format!("连接成功 (HTTP {code}，耗时 {elapsed} ms)"),
            })
        }
        Err(err) => Ok(ProxyConnectivityResult {
            reachable: false,
            proxy_url: normalized,
            latency_ms: None,
            status_code: None,
            detail: format!("连接失败：{err}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fake_jwt(payload: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{body}.")
    }

    #[test]
    fn parses_jwt_claims_and_builds_account() {
        let jwt = make_fake_jwt(json!({
            "email": "tester@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct_12345",
                "chatgpt_user_id": "user_67890",
                "chatgpt_plan_type": "plus",
                "organizations": [
                    { "id": "org_1", "title": "Personal Workspace", "is_default": true }
                ]
            }
        }));
        let tokens = OfficialTokens {
            id_token: jwt.clone(),
            access_token: jwt,
            refresh_token: Some("rt_secret".to_string()),
        };
        let acct = build_account_from_tokens(tokens, None, Some("主力 Plus".to_string())).unwrap();
        assert_eq!(acct.email, "tester@example.com");
        assert_eq!(acct.account_id.as_deref(), Some("acct_12345"));
        assert_eq!(acct.user_id.as_deref(), Some("user_67890"));
        assert_eq!(acct.plan_type.as_deref(), Some("plus"));
        assert_eq!(acct.organization_id.as_deref(), Some("org_1"));
        assert_eq!(acct.account_name.as_deref(), Some("Personal Workspace"));
        assert_eq!(acct.label.as_deref(), Some("主力 Plus"));
    }

    #[test]
    fn parses_wham_usage_response_correctly() {
        let raw = r#"{
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 35,
                    "limit_window_seconds": 18000,
                    "reset_at": 1760000000
                },
                "secondary_window": {
                    "used_percent": 12.4,
                    "limit_window_seconds": 604800,
                    "reset_at": 1760500000
                }
            }
        }"#;
        let quota = parse_wham_usage_payload(raw).unwrap();
        assert_eq!(quota.plan_type.as_deref(), Some("pro"));
        assert_eq!(quota.hourly_percentage, 65);
        assert_eq!(quota.hourly_window_minutes, Some(300));
        assert_eq!(quota.weekly_percentage, 88);
        assert_eq!(quota.weekly_window_minutes, Some(10080));
    }

    #[test]
    fn builds_electron_proxy_arguments_and_env() {
        let proxy = CodexLaunchProxy::Proxy("socks5h://127.0.0.1:7890".to_string());
        let args = build_electron_proxy_args(&proxy);
        assert_eq!(
            args,
            vec![
                "--proxy-server=socks5://127.0.0.1:7890".to_string(),
                "--proxy-bypass-list=localhost;127.0.0.1;[::1];<local>".to_string()
            ]
        );
        let (set_env, remove_env) = build_process_proxy_env(&proxy);
        assert!(remove_env.is_empty());
        assert!(set_env.iter().any(|(k, v)| k == "HTTPS_PROXY" && v == "socks5h://127.0.0.1:7890"));
        assert!(set_env.iter().any(|(k, v)| k == "NO_PROXY" && v.contains("127.0.0.1")));

        // 含用户名密码的代理不拼入命令行参数，仅通过环境变量传递
        let auth_proxy = CodexLaunchProxy::Proxy("http://user:pass@127.0.0.1:7890".to_string());
        assert!(build_electron_proxy_args(&auth_proxy).is_empty());

        let direct = CodexLaunchProxy::Direct;
        assert_eq!(build_electron_proxy_args(&direct), vec!["--no-proxy-server".to_string()]);
    }

    #[test]
    fn resolves_proxy_priority_between_relay_account_and_global() {
        let mut state = OfficialAccountsState {
            active_account_id: Some("acct-1".to_string()),
            accounts: vec![OfficialAccount {
                id: "acct-1".to_string(),
                email: "a@example.com".to_string(),
                label: None,
                user_id: None,
                plan_type: None,
                account_id: Some("acct_1".to_string()),
                organization_id: None,
                account_name: None,
                tokens: OfficialTokens::default(),
                quota: None,
                bound_relay_profile_id: None,
                proxy_mode: "custom".to_string(),
                egress_proxy_url: Some("http://127.0.0.1:8888".to_string()),
                created_at: 0,
                last_used: 0,
                tags: Vec::new(),
            }],
            global_proxy_mode: "custom".to_string(),
            global_proxy_url: "http://127.0.0.1:7890".to_string(),
            proxy_resources: Vec::new(),
            relay_profile_proxies: HashMap::new(),
            relay_profile_bound_accounts: HashMap::new(),
        };
        let mut settings = BackendSettings::default();
        settings.active_relay_id = "relay-1".to_string();

        // 账号级代理优先于全局代理
        assert_eq!(
            resolve_codex_launch_proxy_from(&state, &settings),
            CodexLaunchProxy::Proxy("http://127.0.0.1:8888".to_string())
        );

        // 供应商级代理优先于账号级与全局代理
        state
            .relay_profile_proxies
            .insert("relay-1".to_string(), "socks5://127.0.0.1:1080".to_string());
        assert_eq!(
            resolve_codex_launch_proxy_from(&state, &settings),
            CodexLaunchProxy::Proxy("socks5://127.0.0.1:1080".to_string())
        );
    }

    #[test]
    fn switch_official_account_with_bound_third_party_relay_writes_auth_and_config() {
        let _guard = crate::paths::settings_path_test_guard();
        let temp = tempfile::tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");
        let codex_home = temp.path().join("codex-home");
        std::fs::create_dir_all(&codex_home).unwrap();
        let prev = crate::paths::set_settings_path_for_tests(Some(settings_path.clone()));

        let store = SettingsStore::new(settings_path);
        let mut settings = BackendSettings::default();
        settings.relay_profiles = vec![
            RelayProfile {
                id: "official-default".to_string(),
                name: "官方默认".to_string(),
                relay_mode: RelayMode::Official,
                ..RelayProfile::default()
            },
            RelayProfile {
                id: "third-party-1".to_string(),
                name: "第三方中转 A".to_string(),
                relay_mode: RelayMode::PureApi,
                base_url: "https://api.thirdparty.test/v1".to_string(),
                upstream_base_url: "https://api.thirdparty.test/v1".to_string(),
                api_key: "sk-third-party-key".to_string(),
                model: "gpt-5.4".to_string(),
                ..RelayProfile::default()
            },
        ];
        settings.active_relay_id = "official-default".to_string();
        store.save(&settings).unwrap();

        let jwt = make_fake_jwt(json!({
            "email": "multi@openai.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct_multi_99",
                "chatgpt_plan_type": "pro"
            }
        }));
        let mut account = build_account_from_tokens(
            OfficialTokens {
                id_token: jwt.clone(),
                access_token: jwt,
                refresh_token: Some("rt_multi_99".to_string()),
            },
            None,
            Some("绑定中转的官方号".to_string()),
        )
        .unwrap();
        account.bound_relay_profile_id = Some("third-party-1".to_string());
        let saved_acc = upsert_official_account(account, false).unwrap();

        let updated_settings = switch_official_account(&saved_acc.id, &codex_home, &store).unwrap();
        crate::paths::set_settings_path_for_tests(prev);

        assert_eq!(updated_settings.active_relay_id, "third-party-1");
        let auth_text = std::fs::read_to_string(codex_home.join("auth.json")).unwrap();
        let config_text = std::fs::read_to_string(codex_home.join("config.toml")).unwrap();
        assert!(auth_text.contains("\"acct_multi_99\""), "auth.json: {auth_text}");
        assert!(auth_text.contains("\"sk-third-party-key\""), "auth.json: {auth_text}");
        assert!(config_text.contains("https://api.thirdparty.test/v1"), "config.toml: {config_text}");
    }

    #[test]
    fn deleting_last_official_account_does_not_reimport_from_live_auth() {
        let _guard = crate::paths::settings_path_test_guard();
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_home).unwrap();
        let settings_path = temp.path().join("settings.json");
        let prev = crate::paths::set_settings_path_for_tests(Some(settings_path));
        let jwt = make_fake_jwt(json!({
            "email": "last@openai.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct_last_1",
                "chatgpt_plan_type": "free"
            }
        }));
        let auth_json = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": jwt,
                "access_token": jwt,
                "refresh_token": "rt_last_1",
                "account_id": "acct_last_1"
            }
        });
        std::fs::write(
            codex_home.join("auth.json"),
            serde_json::to_vec_pretty(&auth_json).unwrap(),
        )
        .unwrap();

        let settings = BackendSettings::default();
        let initial_snap = get_official_accounts_snapshot_with(&codex_home, &settings);
        assert_eq!(initial_snap.accounts.len(), 1);
        let only_id = initial_snap.accounts[0].id.clone();

        delete_official_account(&only_id).unwrap();
        let after_delete_snap = get_official_accounts_snapshot_with(&codex_home, &settings);
        crate::paths::set_settings_path_for_tests(prev);

        assert!(
            after_delete_snap.accounts.is_empty(),
            "deleting the last official account must keep the account list empty"
        );
    }
}
