//! `GET /api/entries?q=…` contra un Miniflux falso con 250 artículos. Las
//! pares tienen «Modelos» en el título; las impares, no.

use std::collections::HashMap;

use atalaya_server::{AppState, app, config::Config};
use axum::{
    Json, Router,
    body::Body,
    extract::Query,
    http::{Request, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOTAL: u64 = 250;

/// Un artículo por id, del 1 al 250. Cuanto más alto el id, más nuevo.
fn entry(id: u64) -> Value {
    let title = if id.is_multiple_of(2) {
        format!("Modelos de lenguaje {id}")
    } else {
        format!("Otra cosa {id}")
    };
    let published_at = format!("2024-01-01T{:02}:{:02}:00Z", id / 60, id % 60);
    json!({
        "id": id, "feed_id": 1, "status": "unread", "title": title,
        "url": format!("https://a.example/{id}"), "published_at": published_at,
        "author": "", "content": "<p>texto</p>", "starred": false,
        "feed": {"title": "Feed A"}
    })
}

/// Responde las páginas que pide el servidor, según `order`, `offset` y `limit`.
async fn fake_miniflux(Query(params): Query<HashMap<String, String>>) -> Json<Value> {
    if params.get("order").map(String::as_str) != Some("id") {
        return Json(json!({"total": 0, "entries": []}));
    }
    let offset: u64 = params["offset"].parse().unwrap();
    let limit: u64 = params["limit"].parse().unwrap();
    let entries: Vec<Value> = (1..=TOTAL)
        .skip(offset as usize)
        .take(limit as usize)
        .map(entry)
        .collect();
    Json(json!({"total": TOTAL, "entries": entries}))
}

async fn fake_feeds() -> Json<Value> {
    Json(json!([]))
}

async fn start_fake() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let fake = Router::new()
        .route("/v1/entries", axum::routing::get(fake_miniflux))
        .route("/v1/feeds", axum::routing::get(fake_feeds));
    tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });
    format!("http://{addr}")
}

async fn get_ids(router: &Router, uri: &str) -> (u64, Vec<u64>) {
    let req = Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secreta")
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), 200, "{uri}");
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let page: Value = serde_json::from_slice(&bytes).unwrap();
    let ids = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_u64().unwrap())
        .collect();
    (page["total"].as_u64().unwrap(), ids)
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
async fn search_covers_the_whole_view_and_pages_the_matches() {
    let url = start_fake().await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());

    // Las pares son 125 y van del más nuevo al más viejo: 250, 248, 246…
    let (total, ids) = get_ids(&router, "/api/entries?q=MODELOS&limit=3").await;
    assert_eq!(total, 125);
    assert_eq!(ids, [250, 248, 246]);

    // La última página: el artículo 2, el más viejo que encaja.
    let (total, ids) = get_ids(&router, "/api/entries?q=modelos&limit=3&offset=124").await;
    assert_eq!(total, 125);
    assert_eq!(ids, [2]);

    // Sin coincidencias, total 0 y lista vacía.
    let (total, ids) = get_ids(&router, "/api/entries?q=inexistente").await;
    assert_eq!((total, ids.len()), (0, 0));
}

#[tokio::test]
async fn blank_search_is_a_normal_list() {
    let url = start_fake().await;
    let dir = tempfile::tempdir().unwrap();
    let router = service(&url, dir.path());
    // Sin `q` útil, la respuesta sale de la petición normal (orden por fecha),
    // que el falso no atiende: total 0.
    let (total, ids) = get_ids(&router, "/api/entries?q=%20%20").await;
    assert_eq!((total, ids.len()), (0, 0));
}
