use atalaya_server::{AppState, app, config::Config};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use tower::ServiceExt;

const BODY: &str = r#"{"event_type":"new_entries","feed":{"id":7,"title":"The Verge"},
"entries":[{"id":101,"feed_id":7,"title":"The first broken iPhone Duos are here",
"content":"<p>Repair shops have started to receive the first broken units of the new phone.</p>",
"status":"unread"}]}"#;

fn state(secret: &str, dir: &std::path::Path) -> AppState {
    AppState::new(Config {
        api_key: "secreta".into(),
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.to_owned(),
        miniflux_url: "http://127.0.0.1:9".into(),
        miniflux_api_key: "x".into(),
        miniflux_webhook_secret: secret.into(),
        llama_url: "http://127.0.0.1:9".into(),
        llama_model: "x".into(),
    })
}

fn sign(secret: &str, body: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn webhook(signature: &str) -> Request<Body> {
    Request::post("/webhook/miniflux")
        .header("Content-Type", "application/json")
        .header("X-Miniflux-Event-Type", "new_entries")
        .header("X-Miniflux-Signature", signature)
        .body(Body::from(BODY))
        .unwrap()
}

#[tokio::test]
async fn webhook_rejects_without_secret() {
    let dir = tempfile::tempdir().unwrap();
    let resp = app(state("", dir.path()))
        .oneshot(webhook(&sign("", BODY)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn webhook_rejects_bad_signature() {
    let dir = tempfile::tempdir().unwrap();
    let resp = app(state("secreto", dir.path()))
        .oneshot(webhook(&sign("otro", BODY)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_records_new_entries() {
    let dir = tempfile::tempdir().unwrap();
    let state = state("secreto", dir.path());
    let resp = app(state.clone())
        .oneshot(webhook(&sign("secreto", BODY)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // El idioma se guarda en segundo plano, tras responder.
    for _ in 0..50 {
        let titles = state.translations.titles(&[101]).await;
        if let Some(info) = titles.get(&101) {
            assert_eq!(info.lang.as_deref(), Some("en"));
            assert_eq!(info.title, None, "sin llama-swap no hay traducción");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("el webhook no guardó el artículo");
}
