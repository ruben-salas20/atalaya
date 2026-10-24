//! `POST /api/entries/mark-all-as-read` contra un Miniflux falso que anota
//! las llamadas que recibe.

use std::sync::{Arc, Mutex};

use atalaya_server::{AppState, app, config::Config, papers::saved::FEED_URL};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tower::ServiceExt;

type Calls = Arc<Mutex<Vec<String>>>;

/// Responde la lista de feeds y, a todo lo demás, 204. Anota «MÉTODO ruta».
async fn fake_miniflux(State(calls): State<Calls>, req: Request<Body>) -> Response {
    let call = format!("{} {}", req.method(), req.uri().path());
    calls.lock().unwrap().push(call.clone());
    match call.as_str() {
        // El feed 3 es el de papers guardados: oculto, no debe marcarse en «Todos».
        "GET /v1/feeds" => Json(json!([
            {"id": 1, "feed_url": "https://a.example/rss"},
            {"id": 2, "feed_url": "https://b.example/rss"},
            {"id": 3, "feed_url": FEED_URL},
        ]))
        .into_response(),
        _ => StatusCode::NO_CONTENT.into_response(),
    }
}

/// Arranca el Miniflux falso en un puerto libre y devuelve su dirección.
async fn start_fake(calls: Calls) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let fake = Router::new().fallback(fake_miniflux).with_state(calls);
    tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });
    format!("http://{addr}")
}

fn service(miniflux_url: &str, dir: &std::path::Path) -> Router {
    app(AppState::new(Config {
        api_key: "secreta".into(),
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.to_owned(),
        miniflux_url: miniflux_url.into(),
        miniflux_api_key: "x".into(),
        miniflux_webhook_secret: String::new(),
        llama_url: "http://127.0.0.1:9".into(),
        llama_model: "x".into(),
    }))
}

async fn post(router: &Router, uri: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secreta")
        .body(Body::empty())
        .unwrap();
    router.clone().oneshot(req).await.unwrap().status()
}

fn calls_of(calls: &Calls) -> Vec<String> {
    calls.lock().unwrap().clone()
}

#[tokio::test]
async fn todos_marks_every_feed_but_the_papers_one() {
    let calls = Calls::default();
    let url = start_fake(calls.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());

    let status = post(&router, "/api/entries/mark-all-as-read").await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    let seen = calls_of(&calls);
    assert!(
        seen.contains(&"PUT /v1/feeds/1/mark-all-as-read".to_string()),
        "{seen:?}"
    );
    assert!(
        seen.contains(&"PUT /v1/feeds/2/mark-all-as-read".to_string()),
        "{seen:?}"
    );
    assert!(
        !seen.contains(&"PUT /v1/feeds/3/mark-all-as-read".to_string()),
        "{seen:?}"
    );
}

#[tokio::test]
async fn feed_id_marks_only_that_feed() {
    let calls = Calls::default();
    let url = start_fake(calls.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());

    let status = post(&router, "/api/entries/mark-all-as-read?feed_id=2").await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        calls_of(&calls),
        vec!["PUT /v1/feeds/2/mark-all-as-read".to_string()]
    );
}

#[tokio::test]
async fn category_id_marks_that_category() {
    let calls = Calls::default();
    let url = start_fake(calls.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());

    let status = post(&router, "/api/entries/mark-all-as-read?category_id=5").await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        calls_of(&calls),
        vec!["PUT /v1/categories/5/mark-all-as-read".to_string()]
    );
}

#[tokio::test]
async fn a_bad_id_is_a_bad_request_and_marks_nothing() {
    let calls = Calls::default();
    let url = start_fake(calls.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());

    let status = post(&router, "/api/entries/mark-all-as-read?feed_id=abc").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(calls_of(&calls).is_empty());
}
