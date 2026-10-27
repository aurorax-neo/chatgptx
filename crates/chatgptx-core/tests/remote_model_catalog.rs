use chatgptx_core::remote_model_catalog::parse_models_dev_json;
use serde_json::json;

#[test]
fn parses_models_dev_json_structure() {
    let sample = json!({
        "openai": {
            "name": "OpenAI",
            "api": "https://api.openai.com/v1",
            "models": {
                "gpt-4o": {
                    "id": "gpt-4o",
                    "name": "GPT-4o",
                    "limit": { "context": 128000, "output": 16384 },
                    "reasoning": false
                },
                "o1": {
                    "id": "o1",
                    "name": "o1",
                    "limit": { "context": 200000, "output": 100000 },
                    "reasoning": true
                }
            }
        },
        "anthropic": {
            "name": "Anthropic",
            "api": "https://api.anthropic.com/v1",
            "models": {
                "claude-3-5-sonnet-20241022": {
                    "id": "claude-3-5-sonnet-20241022",
                    "name": "Claude 3.5 Sonnet",
                    "limit": { "context": 200000, "output": 8192 },
                    "reasoning": true
                }
            }
        }
    });

    let (models, index, provider_count) = parse_models_dev_json(&sample);
    assert_eq!(provider_count, 2);
    assert_eq!(models.len(), 3);

    assert!(index.contains_key("gpt-4o"));
    assert!(index.contains_key("o1"));
    assert!(index.contains_key("claude-3-5-sonnet-20241022"));

    let gpt4o = index.get("gpt-4o").unwrap();
    assert_eq!(gpt4o["context_window"], 128_000);
    assert_eq!(gpt4o["provider"], "openai");

    let o1 = index.get("o1").unwrap();
    assert_eq!(o1["context_window"], 200_000);
    assert_eq!(o1["supports_reasoning"], true);
}
use chatgptx_core::remote_model_catalog::{
    fetch_and_cache_models_dev_catalog, get_models_dev_catalog_status, query_models_dev_catalog,
};

#[tokio::test]
async fn test_fetch_and_cache_live_models_dev() {
    let res = fetch_and_cache_models_dev_catalog().await.expect("联网同步 models.dev 成功");
    assert!(res.ok);
    assert!(res.model_count > 1000, "模型数量应当大于 1000");
    assert!(res.provider_count > 50, "厂商数量应当大于 50");

    let status = get_models_dev_catalog_status();
    assert!(status.has_cache);
    assert!(status.loaded);
    assert!(status.model_count > 1000);

    // 测试真实模型查询
    let gpt4o = query_models_dev_catalog("gpt-4o").expect("应能查询到 gpt-4o");
    assert_eq!(gpt4o["context_window"], 128_000);

    let claude = query_models_dev_catalog("claude-sonnet-4-5-20250929").expect("应能查询到 claude");
    assert_eq!(claude["context_window"], 200_000);
}
