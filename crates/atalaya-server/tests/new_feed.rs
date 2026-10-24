//! `POST /api/feeds` contra un Miniflux falso que tarda en crear el feed y
//! tiene más de una página de artículos (#34).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use atalaya_server::{AppState, app, config::Config};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tower::ServiceExt;

type Calls = Arc<Mutex<Vec<String>>>;

const FEED_ID: i64 = 7;
const ENTRIES: usize = 250;

/// Tarda en crear el feed, como Miniflux al descargar cada artículo, y sirve
/// sus artículos por páginas. Anota «MÉTODO ruta?offset».
async fn fake_miniflux(
    State(calls): State<Calls>,
    Query(query): Query<HashMap<String, String>>,
    req: Request<Body>,
) -> Response {
    let path = req.uri().path().to_owned();
    let offset = number(&query, "offset", 0);
    let limit = number(&query, "limit", 100);
    calls
        .lock()
        .unwrap()
        .push(format!("{} {path}?{offset}", req.method()));
    match (req.method().as_str(), path.as_str()) {
        ("POST", "/v1/feeds") => {
            tokio::time::sleep(Duration::from_millis(300)).await;
            (StatusCode::CREATED, Json(json!({"feed_id": FEED_ID}))).into_response()
        }
        ("GET", "/v1/feeds/7/entries") => {
            let entries: Vec<_> = (offset..ENTRIES.min(offset + limit))
                .map(|i| json!({"id": i + 1, "feed_id": FEED_ID, "title": "Hello", "content": ""}))
                .collect();
            Json(json!({"total": ENTRIES, "entries": entries})).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

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

#[tokio::test]
async fn a_new_feed_reaches_the_queue_even_if_the_app_gives_up() {
    let calls = Calls::default();
    let url = start_fake(calls.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/feeds")
        .header(header::AUTHORIZATION, "Bearer secreta")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"feed_url": "https://a.example/rss", "category_id": 1, "crawler": false})
                .to_string(),
        ))
        .unwrap();
    // La app corta antes de que Miniflux responda: el handler se suelta.
    let cut = tokio::time::timeout(Duration::from_millis(50), router.oneshot(req)).await;
    assert!(cut.is_err(), "la petición debía cortarse");

    let wanted = [
        "GET /v1/feeds/7/entries?0",
        "GET /v1/feeds/7/entries?100",
        "GET /v1/feeds/7/entries?200",
    ];
    for _ in 0..50 {
        let seen = calls.lock().unwrap().clone();
        if wanted.iter().all(|w| seen.contains(&w.to_string())) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "no se leyeron todas las páginas: {:?}",
        calls.lock().unwrap()
    );
}

fn number(query: &HashMap<String, String>, key: &str, default: usize) -> usize {
    query
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
