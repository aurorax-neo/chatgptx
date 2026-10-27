//! models.dev 远程全网大模型元数据目录拉取、本地缓存与智能解析。
//!
//! 参考 pi-desktop 架构：使用 https://models.dev/api.json 作为权威的统一模型元数据源，
//! 涵盖市场上所有供应商（OpenAI、Anthropic、DeepSeek、Google、xAI、阿里、智谱等）的
//! context_window、max_output、reasoning、modalities 等属性。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";
pub const MODELS_DEV_CACHE_FILE: &str = "models_dev_catalog.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCatalogStatus {
    pub loaded: bool,
    pub has_cache: bool,
    pub cache_path: String,
    pub model_count: usize,
    pub provider_count: usize,
    pub updated_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCatalogFetchResult {
    pub ok: bool,
    pub model_count: usize,
    pub provider_count: usize,
    pub updated_at: u64,
    pub message: String,
}

#[derive(Clone)]
struct ModelsDevCache {
    models: Vec<Value>,
    index_by_slug: HashMap<String, Value>,
    provider_count: usize,
    updated_at: u64,
}

static GLOBAL_CACHE: OnceLock<Mutex<Option<ModelsDevCache>>> = OnceLock::new();

fn cache_lock() -> &'static Mutex<Option<ModelsDevCache>> {
    GLOBAL_CACHE.get_or_init(|| Mutex::new(None))
}

/// 获取本地 models.dev 缓存文件路径 (~/.chatgptx/models_dev_catalog.json)
pub fn models_dev_cache_path() -> PathBuf {
    crate::paths::default_app_state_dir().join(MODELS_DEV_CACHE_FILE)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 从 models.dev/api.json 原始响应解析归一化模型条目
pub fn parse_models_dev_json(root: &Value) -> (Vec<Value>, HashMap<String, Value>, usize) {
    let mut models = Vec::new();
    let mut index = HashMap::new();
    let mut provider_count = 0;

    let Some(obj) = root.as_object() else {
        return (models, index, 0);
    };

    for (provider_key, provider_val) in obj {
        let Some(p_obj) = provider_val.as_object() else {
            continue;
        };
        let provider_name = p_obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(provider_key);
        let Some(models_map) = p_obj.get("models").and_then(Value::as_object) else {
            continue;
        };

        provider_count += 1;
        for (m_key, m_val) in models_map {
            let Some(m_obj) = m_val.as_object() else {
                continue;
            };
            let id = m_obj.get("id").and_then(Value::as_str).unwrap_or(m_key);
            let name = m_obj.get("name").and_then(Value::as_str).unwrap_or(id);
            let desc = m_obj.get("description").and_then(Value::as_str).unwrap_or("");
            let limit = m_obj.get("limit").and_then(Value::as_object);
            let context_window = limit.and_then(|l| l.get("context")).and_then(Value::as_u64).unwrap_or(128_000);
            let max_output = limit.and_then(|l| l.get("output")).and_then(Value::as_u64).unwrap_or(8_192);
            let reasoning = m_obj.get("reasoning").and_then(Value::as_bool).unwrap_or(false);
            let reasoning_levels = if reasoning {
                json!([
                    { "effort": "low", "description": "Fast responses with lighter reasoning" },
                    { "effort": "medium", "description": "Balances speed and reasoning depth for everyday tasks" },
                    { "effort": "high", "description": "Greater reasoning depth for complex problems" },
                    { "effort": "xhigh", "description": "Extra high reasoning depth for complex problems" }
                ])
            } else {
                json!([])
            };

            let entry = json!({
                "slug": id,
                "display_name": name,
                "description": desc,
                "context_window": context_window,
                "max_context_window": context_window,
                "max_output_tokens": max_output,
                "provider": provider_key,
                "provider_name": provider_name,
                "supports_reasoning": reasoning,
                "supported_reasoning_levels": reasoning_levels,
                "default_reasoning_level": "medium",
                "shell_type": "unified_exec",
                "source": "models.dev"
            });

            let lower_id = id.to_ascii_lowercase();
            index.insert(lower_id, entry.clone());

            // 建立去掉供应商前缀的叶子映射，例如 "anthropic/claude-3-5-sonnet" -> 同时映射 "claude-3-5-sonnet"
            let leaf = id.rsplit('/').next().unwrap_or(id).to_ascii_lowercase();
            if leaf != id.to_ascii_lowercase() {
                index.entry(leaf).or_insert_with(|| entry.clone());
            }

            models.push(entry);
        }
    }

    (models, index, provider_count)
}

/// 获取远程 models.dev 当前缓存状态
pub fn get_models_dev_catalog_status() -> RemoteCatalogStatus {
    let path = models_dev_cache_path();
    let mut guard = cache_lock().lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        if let Some(loaded) = load_from_disk(&path) {
            *guard = Some(loaded);
        }
    }

    if let Some(ref cache) = *guard {
        RemoteCatalogStatus {
            loaded: true,
            has_cache: true,
            cache_path: path.display().to_string(),
            model_count: cache.models.len(),
            provider_count: cache.provider_count,
            updated_at: Some(cache.updated_at),
        }
    } else {
        RemoteCatalogStatus {
            loaded: false,
            has_cache: path.exists(),
            cache_path: path.display().to_string(),
            model_count: 0,
            provider_count: 0,
            updated_at: None,
        }
    }
}

fn load_from_disk(path: &PathBuf) -> Option<ModelsDevCache> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let json_val: Value = serde_json::from_str(&content).ok()?;
    let meta = std::fs::metadata(path).ok()?;
    let updated_at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or_else(now_ms);

    let (models, index_by_slug, provider_count) = parse_models_dev_json(&json_val);
    if models.is_empty() {
        return None;
    }

    Some(ModelsDevCache {
        models,
        index_by_slug,
        provider_count,
        updated_at,
    })
}

/// 联网拉取最新的 https://models.dev/api.json 并缓存至本地
pub async fn fetch_and_cache_models_dev_catalog() -> anyhow::Result<RemoteCatalogFetchResult> {
    let client = crate::http_client::proxied_client_with_timeout(
        "ChatGPTX",
        Duration::from_secs(25),
    )?;

    let res = client.get(MODELS_DEV_URL).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("请求 models.dev 失败，HTTP 状态码: {}", res.status());
    }

    let raw_text = res.text().await?;
    let parsed_json: Value = serde_json::from_str(&raw_text)?;

    let path = models_dev_cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &raw_text)?;

    let (models, index_by_slug, provider_count) = parse_models_dev_json(&parsed_json);
    let model_count = models.len();
    let updated_at = now_ms();

    {
        let mut guard = cache_lock().lock().unwrap_or_else(|p| p.into_inner());
        *guard = Some(ModelsDevCache {
            models,
            index_by_slug,
            provider_count,
            updated_at,
        });
    }

    // 同时清空已有的内置解析缓存，促使下次匹配使用最新拉取到的 models.dev 数据
    crate::model_suffix::clear_builtin_metadata_cache();

    Ok(RemoteCatalogFetchResult {
        ok: true,
        model_count,
        provider_count,
        updated_at,
        message: format!(
            "成功同步 models.dev 元数据：涵盖 {} 个厂商，共计 {} 个模型。",
            provider_count, model_count
        ),
    })
}

/// 按模型名检索 models.dev 目录（支持忽略大小写、剥离前缀、剥离日期后缀等）
pub fn query_models_dev_catalog(slug: &str) -> Option<Value> {
    let mut guard = cache_lock().lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        let path = models_dev_cache_path();
        if let Some(loaded) = load_from_disk(&path) {
            *guard = Some(loaded);
        }
    }
    let cache = guard.as_ref()?;

    let raw = slug.trim().to_ascii_lowercase();
    if let Some(hit) = cache.index_by_slug.get(&raw) {
        return Some(hit.clone());
    }

    let leaf = raw.rsplit('/').next().unwrap_or(&raw);
    if let Some(hit) = cache.index_by_slug.get(leaf) {
        return Some(hit.clone());
    }

    // 剥离日期后缀
    let stripped = strip_date_suffix(leaf);
    if stripped != leaf {
        if let Some(hit) = cache.index_by_slug.get(stripped) {
            return Some(hit.clone());
        }
    }

    // 点杠互换
    let dashed = leaf.replace('.', "-");
    if dashed != leaf {
        if let Some(hit) = cache.index_by_slug.get(&dashed) {
            return Some(hit.clone());
        }
    }

    None
}

/// 获取 models.dev 缓存中所有的模型条目（用于全局索引扩展）
pub fn all_cached_models_dev_models() -> Vec<Value> {
    let mut guard = cache_lock().lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        let path = models_dev_cache_path();
        if let Some(loaded) = load_from_disk(&path) {
            *guard = Some(loaded);
        }
    }
    guard
        .as_ref()
        .map(|c| c.models.clone())
        .unwrap_or_default()
}

fn strip_date_suffix(s: &str) -> &str {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len >= 11 {
        let sep = bytes[len - 6];
        let sep2 = bytes[len - 3];
        if (sep == b'-' || sep == b'_' || sep == b'.') && sep == sep2 {
            let prefix_sep = bytes[len - 11];
            if prefix_sep == b'-' || prefix_sep == b'_' || prefix_sep == b'.' {
                let y = &s[len - 10..len - 6];
                let m = &s[len - 5..len - 3];
                let d = &s[len - 2..];
                if y.chars().all(|c| c.is_ascii_digit())
                    && m.chars().all(|c| c.is_ascii_digit())
                    && d.chars().all(|c| c.is_ascii_digit())
                {
                    return &s[..len - 11];
                }
            }
        }
    }
    if len >= 9 && (bytes[len - 9] == b'-' || bytes[len - 9] == b'_') {
        let digits = &s[len - 8..];
        if digits.starts_with("20") && digits.chars().all(|c| c.is_ascii_digit()) {
            return &s[..len - 9];
        }
    }
    s
}
