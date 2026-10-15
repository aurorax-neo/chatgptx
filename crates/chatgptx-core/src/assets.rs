use base64::Engine;
use serde_json::{Value, json};
use std::path::Path;

use crate::settings::BackendSettings;

const RENDERER_SCRIPT: &str = include_str!("../../../assets/inject/renderer-inject.js");
const WHALE_DEFAULT_IMAGE: &[u8] =
    include_bytes!("../../../assets/inject/upstream/whale-widget/DSniang1.png");
const WHALE_RUNTIME: &str =
    include_str!("../../../assets/inject/upstream/whale-widget/full-widget-codex.js");
const WHALE_MEDIA: &[(&str, &str, &[u8])] = &[
    ("DSniang1.png", "image/png", WHALE_DEFAULT_IMAGE),
    (
        "Ya1.mp3",
        "audio/mpeg",
        include_bytes!("../../../assets/inject/upstream/whale-widget/Ya1.mp3"),
    ),
    (
        "Ya2.mp3",
        "audio/mpeg",
        include_bytes!("../../../assets/inject/upstream/whale-widget/Ya2.mp3"),
    ),
    (
        "D1.mp3",
        "audio/mpeg",
        include_bytes!("../../../assets/inject/upstream/whale-widget/D1.mp3"),
    ),
    (
        "D2.mp3",
        "audio/mpeg",
        include_bytes!("../../../assets/inject/upstream/whale-widget/D2.mp3"),
    ),
    (
        "minecraft-exp-orb.wav",
        "audio/wav",
        include_bytes!("../../../assets/inject/upstream/whale-widget/minecraft-exp-orb.wav"),
    ),
    (
        "task-end-a.wav",
        "audio/wav",
        include_bytes!("../../../assets/inject/upstream/whale-widget/task-end-a.wav"),
    ),
    (
        "bubble-petpet.gif",
        "image/gif",
        include_bytes!("../../../assets/inject/upstream/whale-widget/bubble-petpet.gif"),
    ),
    (
        "bubble-money1.gif",
        "image/gif",
        include_bytes!("../../../assets/inject/upstream/whale-widget/bubble-money1.gif"),
    ),
    (
        "rua.gif",
        "image/gif",
        include_bytes!("../../../assets/inject/upstream/whale-widget/rua.gif"),
    ),
];

fn whale_assets_json() -> String {
    let values: serde_json::Map<String, Value> = WHALE_MEDIA
        .iter()
        .map(|(name, mime, bytes)| ((*name).to_string(), json!(image_data_uri(mime, bytes))))
        .collect();
    Value::Object(values).to_string()
}

fn whale_runtime_script() -> String {
    // 直接注入函数声明；完整引擎使用词法宿主，不要求 renderer 开放 unsafe-eval。
    format!(
        "window.__CHATGPTX_WHALE_ASSETS__ = {};\nwindow.__CHATGPTX_WHALE_ENGINE__ = function(window,document,fetch,localStorage,setTimeout,clearTimeout,setInterval,clearInterval,requestAnimationFrame,cancelAnimationFrame,MutationObserver,ResizeObserver,AudioContext,webkitAudioContext,Image,Audio) {{\n{}\n}};\n",
        whale_assets_json(),
        WHALE_RUNTIME
    )
}

const PET_REAL_MOUSE_SCRIPT: &str = include_str!("../../../assets/inject/pet-real-mouse-inject.js");
const API_QUOTA_GATE_SCRIPT: &str = include_str!("../../../assets/inject/api-quota-gate.js");
const STEPWISE_SCRIPT: &str = concat!(
    "(() => {\n",
    include_str!("../../../assets/inject/floating-panel/runtime/state.js"),
    include_str!("../../../assets/inject/floating-panel/core/appearance-runtime.js"),
    include_str!("../../../assets/inject/floating-panel/runtime/dom.js"),
    include_str!("../../../assets/inject/floating-panel/runtime/bridge-client.js"),
    include_str!("../../../assets/inject/floating-panel/runtime/answer-context.js"),
    include_str!("../../../assets/inject/floating-panel/stepwise/suggestions.js"),
    include_str!("../../../assets/inject/floating-panel/stepwise/generation.js"),
    include_str!("../../../assets/inject/floating-panel/runtime/lifecycle.js"),
    include_str!("../../../assets/inject/floating-panel/runtime/settings.js"),
    include_str!("../../../assets/inject/floating-panel/core/appearance.js"),
    include_str!("../../../assets/inject/floating-panel/core/host.js"),
    include_str!("../../../assets/inject/floating-panel/core/geometry.js"),
    include_str!("../../../assets/inject/floating-panel/core/interaction.js"),
    include_str!("../../../assets/inject/floating-panel/core/views.js"),
    include_str!("../../../assets/inject/floating-panel/outline/parser.js"),
    include_str!("../../../assets/inject/floating-panel/outline/navigation.js"),
    include_str!("../../../assets/inject/floating-panel/outline/feature.js"),
    include_str!("../../../assets/inject/floating-panel/outline/view.js"),
    include_str!("../../../assets/inject/floating-panel/core/scroll-state.js"),
    include_str!("../../../assets/inject/floating-panel-inject.js"),
    "\n})();\n",
);
pub const DIAGNOSTIC_BUILD_ID: &str = "diag-20260518-1";

pub fn renderer_script() -> &'static str {
    RENDERER_SCRIPT
}

pub fn stepwise_script() -> &'static str {
    STEPWISE_SCRIPT
}

pub fn pet_real_mouse_script() -> &'static str {
    PET_REAL_MOUSE_SCRIPT
}

pub fn api_quota_gate_script() -> &'static str {
    API_QUOTA_GATE_SCRIPT
}

const PET_V2_SPRITE_DETECTION_SCRIPT: &str = r#"
  const isV2Sprite = async (mascot) => {
    if (!mascot) return false;
    if (Array.from(mascot.querySelectorAll("img")).some((image) =>
      image.naturalWidth === 1536 && image.naturalHeight === 2288
    )) return true;
    for (const element of [mascot, ...mascot.querySelectorAll("*")]) {
      const background = getComputedStyle(element).backgroundImage || "";
      const match = background.match(/url\(["']?([^"')]+)/i);
      if (!match) continue;
      const source = match[1];
      const cacheKey = "__chatgptxPetV2SpriteProbe";
      let probe = window[cacheKey];
      if (!probe || probe.source !== source) {
        probe = { source, valid: false, pending: true };
        probe.promise = (async () => {
          try {
            const image = new Image();
            image.src = source;
            await image.decode();
            return image.naturalWidth === 1536 && image.naturalHeight === 2288;
          } catch {
            return false;
          }
        })().then((valid) => {
          probe.valid = valid;
          probe.pending = false;
          return valid;
        });
        window[cacheKey] = probe;
      }
      const wasPending = probe.pending;
      const valid = wasPending ? await probe.promise : probe.valid;
      if (wasPending) {
        const currentBackground = getComputedStyle(element).backgroundImage || "";
        const currentMatch = currentBackground.match(/url\(["']?([^"')]+)/i);
        if (currentMatch?.[1] !== source) continue;
      }
      if (window[cacheKey] === probe && valid) return true;
    }
    return false;
  };
"#;

pub fn pet_real_mouse_capability_probe_script() -> String {
    let mut script = String::from(
        r#"
(async () => {
  const mascot = document.querySelector('[data-avatar-mascot="true"]');
"#,
    );
    script.push_str(PET_V2_SPRITE_DETECTION_SCRIPT);
    script.push_str(
        r#"
  if (!await isV2Sprite(mascot)) return false;
  const urls = [
    ...Array.from(document.scripts || []).map((script) => script.src),
    ...Array.from(document.querySelectorAll("link[href]") || []).map((link) => link.href),
    ...performance.getEntriesByType("resource").map((entry) => entry.name),
  ].filter((url) => url && url.includes("/assets/") && url.split("?")[0].endsWith(".js"));
  let dispatcherUrl = urls.find((url) => url.includes("vscode-api-"));
  if (!dispatcherUrl) {
    for (const url of urls) {
      try {
        const source = await fetch(url).then((response) => response.ok ? response.text() : "");
        const match = source.match(/["'](\.\/(?:assets\/)?vscode-api-[^"']+\.js)["']/);
        if (match) {
          dispatcherUrl = new URL(match[1], url).href;
          break;
        }
      } catch {
      }
    }
  }
  if (!dispatcherUrl) return false;
  try {
    const module = await import(dispatcherUrl);
    return Object.values(module || {}).some((value) => value
      && typeof value.dispatchHostMessage === "function"
      && typeof value.subscribe === "function");
  } catch {
    return false;
  }
})()
"#,
    );
    script
}

pub fn pet_real_mouse_update_script(x: i32, y: i32) -> String {
    let mut script = String::from(
        r#"(async () => {
  const mascot = document.querySelector('[data-avatar-mascot="true"]');
"#,
    );
    script.push_str(PET_V2_SPRITE_DETECTION_SCRIPT);
    script.push_str(&format!(
        r#"
  return await isV2Sprite(mascot)
    && window.__chatgptxPetRealMouseLook?.updateScreenPoint?.({{ x: {x}, y: {y} }}) === true;
}})()"#
    ));
    script
}

pub fn pet_real_mouse_stop_script() -> &'static str {
    "window.__chatgptxPetRealMouseLook?.stop?.();"
}

pub fn injection_script(helper_port: u16) -> String {
    injection_script_with_settings(helper_port, &BackendSettings::default())
}

pub fn hide_official_usage_alert_config(settings: &BackendSettings) -> bool {
    let profile = settings.active_relay_profile();
    profile.relay_mode == crate::settings::RelayMode::Official && profile.official_mix_api_key
}

pub fn force_chinese_locale_config(settings: &BackendSettings) -> Value {
    json!({ "enabled": settings.codex_app_force_chinese_locale, "locale": "zh-CN" })
}

pub fn injection_script_with_settings(helper_port: u16, settings: &BackendSettings) -> String {
    let helper_url = format!("http://127.0.0.1:{helper_port}");
    let instance_label = std::env::var("CHATGPTX_INSTANCE_LABEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("CHATGPTX_INSTANCE_ID")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
    let instance_identity = instance_label
        .as_deref()
        .and_then(|value| serde_json::to_string(value).ok())
        .map(|value| format!("window.__CHATGPTX_INSTANCE_LABEL__ = {value};\n"))
        .unwrap_or_default();
    let image_overlay = image_overlay_config(helper_port, settings);
    let paste_fix = paste_fix_enabled_config(settings);
    let force_chinese_locale = force_chinese_locale_config(settings);
    let hide_official_usage_alert = hide_official_usage_alert_config(settings);
    let stepwise_runtime =
        if settings.codex_app_stepwise_enabled || settings.codex_app_answer_outline_enabled {
            stepwise_script()
        } else {
            ""
        };
    format!(
        "{}window.__CODEX_SESSION_DELETE_HELPER__ = {};\nwindow.__CHATGPTX_VERSION__ = {};\nwindow.__CHATGPTX_BUILD__ = {};\nwindow.__CHATGPTX_IMAGE_OVERLAY__ = {};\nwindow.__CHATGPTX_PASTE_FIX__ = {};\nwindow.__CHATGPTX_FORCE_CHINESE_LOCALE__ = {};\nwindow.__CHATGPTX_HIDE_OFFICIAL_USAGE_ALERT__ = {};\n{}\n{}",
        instance_identity,
        serde_json::to_string(&helper_url).expect("helper URL should serialize"),
        serde_json::to_string(crate::version::VERSION).expect("version should serialize"),
        serde_json::to_string(DIAGNOSTIC_BUILD_ID).expect("build id should serialize"),
        serde_json::to_string(&image_overlay).expect("image overlay config should serialize"),
        serde_json::to_string(&paste_fix).expect("paste fix config should serialize"),
        serde_json::to_string(&force_chinese_locale)
            .expect("force Chinese locale config should serialize"),
        serde_json::to_string(&hide_official_usage_alert)
            .expect("usage alert config should serialize"),
        format!(
            "window.__CHATGPTX_WHALE_IMAGE__ = {};\n{}\n{}\n{}",
            serde_json::to_string(&image_data_uri("image/png", WHALE_DEFAULT_IMAGE))
                .expect("whale image should serialize"),
            whale_runtime_script(),
            include_str!("../../../assets/inject/composer-readiness.js"),
            renderer_script()
        ),
        stepwise_runtime,
    )
}

pub fn image_overlay_config(helper_port: u16, settings: &BackendSettings) -> Value {
    let has_path = !settings.codex_app_image_overlay_path.trim().is_empty();
    let enabled = settings.codex_app_image_overlay_enabled && has_path;
    let data_url = if enabled {
        image_file_data_uri(Path::new(settings.codex_app_image_overlay_path.trim()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    json!({
        "enabled": enabled && !data_url.is_empty(),
        "opacity": f64::from(settings.codex_app_image_overlay_opacity.clamp(1, 100)) / 100.0,
        "fitMode": settings.codex_app_image_overlay_fit_mode.as_str(),
        "dataUrl": data_url,
        "imageUrl": if enabled && !data_url.is_empty() {
            format!("http://127.0.0.1:{helper_port}/overlay/image")
        } else {
            String::new()
        },
    })
}

pub fn paste_fix_enabled_config(settings: &BackendSettings) -> Value {
    json!({ "enabled": settings.codex_app_paste_fix })
}

fn image_data_uri(mime_type: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime_type};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn image_file_data_uri(path: &Path) -> Option<String> {
    let mime_type = image_content_type(path)?;
    let bytes = std::fs::read(path).ok()?;
    Some(image_data_uri(mime_type, &bytes))
}

fn image_content_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        Some("gif") => Some("image/gif"),
        Some("bmp") => Some("image/bmp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_does_not_project_retired_plugin_cache() {
        let script = injection_script_with_settings(57321, &BackendSettings::default());
        assert!(!script.contains("window.__CHATGPTX_PLUGIN_MARKETPLACES__ ="));
    }

    #[test]
    fn whale_character_is_embedded_for_offline_and_live_enable() {
        let script = injection_script(57321);
        let prefix = "window.__CHATGPTX_WHALE_IMAGE__ = ";
        let value = script.split_once(prefix).unwrap().1.lines().next().unwrap();
        let uri: String = serde_json::from_str(value.trim_end_matches(';')).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(uri.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        assert_eq!(bytes, WHALE_DEFAULT_IMAGE);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn whale_media_preserves_original_audio_and_animation_bytes() {
        let values: Value = serde_json::from_str(&whale_assets_json()).unwrap();
        assert_eq!(values.as_object().unwrap().len(), 10);
        for (name, mime, expected) in WHALE_MEDIA {
            let uri = values[*name].as_str().unwrap();
            let prefix = format!("data:{mime};base64,");
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(uri.strip_prefix(&prefix).unwrap())
                .unwrap();
            assert_eq!(&decoded, expected);
        }
        let script = whale_runtime_script();
        assert!(script.contains("window.__CHATGPTX_WHALE_ENGINE__ = function("));
        assert!(!script.contains("new Function("));
    }

    #[test]
    fn image_overlay_config_includes_fit_mode() {
        let settings = BackendSettings {
            codex_app_image_overlay_fit_mode: "fill".to_string(),
            ..BackendSettings::default()
        };
        let config = image_overlay_config(57321, &settings);

        assert_eq!(config["fitMode"].as_str(), Some("fill"));
    }

    #[test]
    fn image_overlay_config_clears_endpoint_when_asset_is_unavailable() {
        let settings = BackendSettings {
            codex_app_image_overlay_enabled: true,
            codex_app_image_overlay_path: "C:\\missing\\overlay.png".to_string(),
            ..BackendSettings::default()
        };
        let config = image_overlay_config(57321, &settings);

        assert_eq!(config["enabled"], false);
        assert_eq!(config["dataUrl"], "");
        assert_eq!(config["imageUrl"], "");
    }
}
