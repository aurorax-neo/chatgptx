pub fn proxied_client(user_agent: &str) -> anyhow::Result<reqwest::Client> {
    proxied_client_with_timeout(user_agent, std::time::Duration::from_secs(35))
}

pub fn proxied_client_with_timeout(
    user_agent: &str,
    timeout: std::time::Duration,
) -> anyhow::Result<reqwest::Client> {
    let ua = if user_agent.trim().is_empty() {
        format!("ChatGPTX/{}", env!("CARGO_PKG_VERSION"))
    } else {
        user_agent.trim().to_string()
    };
    let mut builder = reqwest::Client::builder().user_agent(ua).timeout(timeout);
    let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
    if !crate::proxy::has_proxy_environment(&env_map) {
        let configured_proxy = {
            let state = crate::official_accounts::load_official_accounts_state();
            match state.global_proxy_mode.trim() {
                "direct" => Some("direct".to_string()),
                "custom" if !state.global_proxy_url.trim().is_empty() => {
                    Some(state.global_proxy_url.trim().to_string())
                }
                _ => crate::proxy::detect_system_proxy(),
            }
        };
        if let Some(proxy_spec) = configured_proxy {
            if proxy_spec.eq_ignore_ascii_case("direct") {
                builder = builder.no_proxy();
            } else if let Ok(proxy) = reqwest::Proxy::all(&proxy_spec) {
                let proxy = proxy.no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
                builder = builder.proxy(proxy);
            }
        }
    }
    Ok(builder.build()?)
}

/// VLM 专用 HTTP client（带超时）。
/// 不复用通用 proxied_client，避免 VLM 服务无响应时永久阻塞整个代理。
pub fn vlm_http_client() -> anyhow::Result<reqwest::Client> {
    vlm_http_client_with_timeout(
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(30),
    )
}

pub(crate) fn vlm_http_client_with_timeout(
    connect: std::time::Duration,
    total: std::time::Duration,
) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("ChatGPTX-VLM/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(connect)
        .timeout(total)
        .build()?)
}
