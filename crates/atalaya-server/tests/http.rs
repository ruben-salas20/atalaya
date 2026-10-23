use atalaya_server::{AppState, app, config::Config};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    app(AppState::new(Config {
        api_key: "secreta".into(),
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: std::env::temp_dir(),
        miniflux_url: "http://127.0.0.1:9".into(),
        miniflux_api_key: "x".into(),
        miniflux_webhook_secret: String::new(),
        llama_url: "http://127.0.0.1:9".into(),
        llama_model: "x".into(),
    }))
}

async fn status(req: Request<Body>) -> StatusCode {
    test_app().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn health_needs_no_key() {
    let req = Request::get("/health").body(Body::empty()).unwrap();
    assert_eq!(status(req).await, StatusCode::OK);
}

#[tokio::test]
async fn api_rejects_missing_or_wrong_key() {
    let req = Request::get("/api/status").body(Body::empty()).unwrap();
    assert_eq!(status(req).await, StatusCode::UNAUTHORIZED);

    let req = Request::get("/api/status")
        .header(header::AUTHORIZATION, "Bearer otra")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status(req).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_accepts_right_key() {
    let req = Request::get("/api/status")
        .header(header::AUTHORIZATION, "Bearer secreta")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status(req).await, StatusCode::OK);
}
