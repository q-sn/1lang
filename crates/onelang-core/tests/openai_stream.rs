//! Streams a canned SSE response (Groq-style, header split across chunks,
//! usage in x_groq) through the OpenAI-compatible provider.

use std::sync::{Arc, Mutex};

use onelang_core::engine::EngineOptions;
use onelang_core::providers::{self, ProviderConfig, ProviderKind};
use onelang_core::{DetectionMode, Engine, LanguagePair, TranslateEvent, TranslateRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve(chunks: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 65536];
        let _ = sock.read(&mut buf).await;
        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n";
        sock.write_all(head.as_bytes()).await.unwrap();
        for c in chunks {
            let frame = format!("{:x}\r\n{}\r\n", c.len(), c);
            sock.write_all(frame.as_bytes()).await.unwrap();
            sock.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        sock.write_all(b"0\r\n\r\n").await.unwrap();
    });
    format!("http://{addr}")
}

fn delta(s: &str) -> String {
    format!("data: {}\n\n", serde_json::json!({ "choices": [{ "delta": { "content": s } }] }))
}

#[tokio::test]
async fn streams_with_header_and_usage() {
    let chunks = vec![
        delta("[["),
        delta("en>r"),
        // An SSE event split across TCP chunks.
        delta("u]]\nПри").split_at(10).0.to_string(),
        delta("u]]\nПри").split_at(10).1.to_string(),
        delta("вет, мир"),
        format!(
            "data: {}\n\n",
            serde_json::json!({ "choices": [{ "delta": {} }], "x_groq": { "usage": { "prompt_tokens": 120, "completion_tokens": 7 } } })
        ),
        "data: [DONE]\n\n".to_string(),
    ];
    let base = serve(chunks).await;
    let mut cfg = ProviderConfig::new("groq", ProviderKind::Groq);
    cfg.base_url = Some(base);
    let p = providers::build(cfg, Some("test".into()), reqwest::Client::new()).unwrap();
    let engine = Engine::new(
        vec![p],
        EngineOptions {
            pair: LanguagePair { primary: "ru".into(), secondary: "en".into() },
            detection: DetectionMode::Llm,
            favorite_languages: vec![],
            custom_instructions: None,
            dictionary_for_words: false,
        },
    );
    let streamed = Arc::new(Mutex::new(String::new()));
    let s2 = streamed.clone();
    let res = engine
        .translate(TranslateRequest { text: "Hello, world".into(), ..Default::default() }, &move |e| {
            if let TranslateEvent::Delta { text } = e {
                s2.lock().unwrap().push_str(&text);
            }
        })
        .await
        .unwrap();
    assert_eq!(res.text, "Привет, мир");
    assert_eq!(*streamed.lock().unwrap(), "Привет, мир");
    assert_eq!(res.source_lang.as_deref(), Some("en"));
    assert_eq!(res.target_lang, "ru");
    assert_eq!(res.usage.input_tokens, 120);
    assert_eq!(res.usage.output_tokens, 7);
    assert!(res.cost_usd.unwrap() > 0.0);
}

#[tokio::test]
async fn multibyte_split_between_chunks() {
    let event = delta("Привет");
    let bytes = event.as_bytes();
    // Split inside the two-byte "П".
    let cut = event.find('П').unwrap() + 1;
    let (a, b) = (bytes[..cut].to_vec(), bytes[cut..].to_vec());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 65536];
        let _ = sock.read(&mut buf).await;
        sock.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n").await.unwrap();
        for part in [a, b, b"data: [DONE]\n\n".to_vec()] {
            sock.write_all(format!("{:x}\r\n", part.len()).as_bytes()).await.unwrap();
            sock.write_all(&part).await.unwrap();
            sock.write_all(b"\r\n").await.unwrap();
            sock.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        sock.write_all(b"0\r\n\r\n").await.unwrap();
    });
    let mut cfg = ProviderConfig::new("groq", ProviderKind::Groq);
    cfg.base_url = Some(format!("http://{addr}"));
    let p = providers::build(cfg, Some("k".into()), reqwest::Client::new()).unwrap();
    let engine = Engine::new(
        vec![p],
        EngineOptions {
            pair: LanguagePair { primary: "ru".into(), secondary: "en".into() },
            detection: DetectionMode::Local,
            favorite_languages: vec![],
            custom_instructions: None,
            dictionary_for_words: false,
        },
    );
    let res = engine
        .translate(
            TranslateRequest { text: "Hello there my friend, how are you".into(), ..Default::default() },
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(res.text, "Привет");
}
