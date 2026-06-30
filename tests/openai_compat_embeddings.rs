//! OpenAI-compatible embeddings integration test.
//!
//! Spins up an in-process mock HTTP server that mimics the OpenAI `/v1/embeddings`
//! contract — the SAME contract 9Router exposes at `POST /v1/embeddings` — and
//! drives a real `VoyageClient` (provider = OpenAI) end-to-end against it. This
//! is the automated smoke test for routing context-engine embeddings through a
//! local 9Router gateway (base URL `http://localhost:20128/v1`).
//!
//! What it verifies:
//!   1. URL construction: a base of `http://127.0.0.1:<port>/v1` resolves to
//!      `…/v1/embeddings` (no double `/embeddings`), matching 9Router's route.
//!   2. Request shape is OpenAI-compatible: `Authorization: Bearer <key>`,
//!      JSON body carries `model` + `input`, and `input_type` is OMITTED (Voyage
//!      -only field that OpenAI-compatible servers reject).
//!   3. Response parsing: `{ "data": [ { "embedding": [...] } ] }` is decoded
//!      into the per-input vectors, in order.
//!
//! No network egress, no secrets — the "API key" is a dummy the mock echoes back
//! for assertion. The server is a hand-rolled tokio TCP listener so the test
//! pulls in no extra dev-dependency beyond what the crate already uses.

use std::sync::{Arc, Mutex};

use context_engine_rs::embedding::InputType;
use context_engine_rs::embedding::voyage::{Provider, VoyageClient, embedding_url};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// What the mock captured from the single request it served.
#[derive(Default, Clone)]
struct Captured {
    path: String,
    authorization: Option<String>,
    body: String,
}

/// Start a one-shot OpenAI-compatible embeddings mock on an ephemeral port.
///
/// Returns the bound base URL (`http://127.0.0.1:<port>/v1`) and a handle the
/// caller can lock after the request to inspect what arrived. The server serves
/// exactly `expected_requests` requests, replying to each with a fixed two-vector
/// embeddings payload, then exits.
async fn start_mock(expected_requests: usize) -> (String, Arc<Mutex<Captured>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://127.0.0.1:{}/v1", addr.port());
    let captured = Arc::new(Mutex::new(Captured::default()));
    let captured_srv = Arc::clone(&captured);

    tokio::spawn(async move {
        for _ in 0..expected_requests {
            let (mut sock, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => return,
            };

            // Read the request head + body. Embedding requests are small; a single
            // read of up to 64 KiB captures the whole thing in test conditions.
            let mut buf = vec![0u8; 65536];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]).to_string();

            // Split head/body on the blank line.
            let (head, body) = match raw.split_once("\r\n\r\n") {
                Some((h, b)) => (h, b),
                None => (raw.as_str(), ""),
            };

            let mut path = String::new();
            let mut authorization = None;
            for (i, line) in head.lines().enumerate() {
                if i == 0 {
                    // Request line: METHOD SP PATH SP VERSION
                    if let Some(p) = line.split_whitespace().nth(1) {
                        path = p.to_string();
                    }
                } else if let Some(v) = line.strip_prefix("Authorization: ") {
                    authorization = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("authorization: ") {
                    authorization = Some(v.trim().to_string());
                }
            }

            {
                let mut c = captured_srv.lock().unwrap();
                c.path = path;
                c.authorization = authorization;
                c.body = body.to_string();
            }

            // Fixed OpenAI-style response: two 3-dim vectors.
            let payload = r#"{"object":"list","data":[{"object":"embedding","index":0,"embedding":[0.1,0.2,0.3]},{"object":"embedding","index":1,"embedding":[0.4,0.5,0.6]}],"model":"mock","usage":{"prompt_tokens":4,"total_tokens":4}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = sock.write_all(response.as_bytes()).await;
            let _ = sock.flush().await;
        }
    });

    (base, captured)
}

#[test]
fn base_url_resolves_to_9router_embeddings_path() {
    // The exact 9Router base the docs recommend → exactly one `/embeddings`.
    assert_eq!(
        embedding_url(Provider::OpenAI, Some("http://localhost:20128/v1")),
        "http://localhost:20128/v1/embeddings"
    );
    // Trailing slash is tolerated.
    assert_eq!(
        embedding_url(Provider::OpenAI, Some("http://localhost:20128/v1/")),
        "http://localhost:20128/v1/embeddings"
    );
    // Pasting the full endpoint must NOT double the suffix.
    assert_eq!(
        embedding_url(Provider::OpenAI, Some("http://localhost:20128/v1/embeddings")),
        "http://localhost:20128/v1/embeddings"
    );
}

#[tokio::test]
async fn openai_compatible_embed_roundtrip_against_mock() {
    let (base, captured) = start_mock(1).await;

    let client = VoyageClient::new_for_provider(
        Provider::OpenAI,
        "emb/nomic-embed-text".to_string(),
        vec!["dummy-9router-key".to_string()],
        Some(&base),
        None,
    )
    .expect("build OpenAI-compatible embedding client");

    let inputs = vec!["fn main() {}".to_string(), "let x = 1;".to_string()];
    let embeddings = client
        .embed(&inputs, InputType::Document)
        .await
        .expect("embed against mock 9Router");

    // Response parsed into two vectors, in order.
    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[0], vec![0.1, 0.2, 0.3]);
    assert_eq!(embeddings[1], vec![0.4, 0.5, 0.6]);

    let c = captured.lock().unwrap();

    // 1. URL: base `…/v1` hit `…/v1/embeddings` (no double suffix).
    assert_eq!(c.path, "/v1/embeddings", "request path");

    // 2. Bearer auth carries the configured key.
    assert_eq!(
        c.authorization.as_deref(),
        Some("Bearer dummy-9router-key"),
        "Authorization header"
    );

    // 3. Body is OpenAI-compatible: has `model` + `input`, OMITS Voyage-only
    //    `input_type` (which OpenAI-compatible servers reject).
    let body: serde_json::Value = serde_json::from_str(&c.body).expect("request body is JSON");
    assert_eq!(body["model"], "emb/nomic-embed-text", "model field");
    assert!(body["input"].is_array(), "input must be an array");
    assert_eq!(body["input"][0], "fn main() {}");
    assert_eq!(body["input"][1], "let x = 1;");
    assert!(
        body.get("input_type").is_none(),
        "OpenAI-compatible body must omit input_type, got: {body}"
    );
    assert!(
        body.get("dimensions").is_none(),
        "dimensions must be omitted when unset"
    );
}
