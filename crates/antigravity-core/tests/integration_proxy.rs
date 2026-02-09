#![allow(unused_crate_dependencies)]
#![allow(clippy::tests_outside_test_module, reason = "integration tests live in tests/ dir")]
#![allow(clippy::expect_used, reason = "integration test — panics are the assertion mechanism")]

use antigravity_core::proxy::upstream::client::UpstreamClient;
use antigravity_types::models::config::UpstreamProxyConfig;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn gemini_success_body() -> serde_json::Value {
    serde_json::json!({
        "candidates": [{
            "content": {
                "parts": [{"text": "Hello from mock!"}],
                "role": "model"
            },
            "finishReason": "STOP"
        }]
    })
}

fn request_body() -> serde_json::Value {
    serde_json::json!({
        "contents": [{"parts": [{"text": "Hi"}], "role": "user"}]
    })
}

async fn setup_server() -> MockServer {
    MockServer::start().await
}

#[tokio::test]
async fn test_upstream_proxy_flow() {
    let server = setup_server().await;
    let mock_url = format!("{}/v1internal", server.uri());
    let client = UpstreamClient::new(UpstreamProxyConfig::default(), Some(vec![mock_url]));

    {
        let _guard = Mock::given(method("POST"))
            .and(path_regex(r"/v1internal:.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(gemini_success_body()))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let result = client
            .call_v1_internal(
                "streamGenerateContent",
                "fake-token",
                request_body(),
                Some("alt=sse"),
            )
            .await;

        assert!(result.is_ok(), "200 scenario: expected Ok, got: {:?}", result.err());
        let resp = result.expect("already checked");
        assert_eq!(resp.status(), 200, "200 scenario: wrong status");
    }

    {
        let _guard = Mock::given(method("POST"))
            .and(path_regex(r"/v1internal:.*"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount_as_scoped(&server)
            .await;

        let result =
            client.call_v1_internal("generateContent", "fake-token", request_body(), None).await;

        let resp = result.expect("500 scenario: client should return Ok(Response), not Err");
        assert_eq!(resp.status(), 500, "500 scenario: wrong status");
    }

    {
        let _guard = Mock::given(method("POST"))
            .and(path_regex(r"/v1internal:.*"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {
                    "code": 429,
                    "message": "Resource exhausted",
                    "status": "RESOURCE_EXHAUSTED"
                }
            })))
            .mount_as_scoped(&server)
            .await;

        let result =
            client.call_v1_internal("generateContent", "fake-token", request_body(), None).await;

        let resp = result.expect("429 scenario: client should return Ok(Response), not Err");
        assert_eq!(resp.status(), 429, "429 scenario: wrong status");
    }
}
