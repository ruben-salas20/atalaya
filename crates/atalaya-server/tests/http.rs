use atalaya_server::{AppState, app, config::Config};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

/// La carpeta temporal vive lo que la prueba: al soltarla se borra la base.
fn test_app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let router = app(AppState::new(Config {
        api_key: "secreta".into(),
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: dir.path().to_owned(),
        miniflux_url: "http://127.0.0.1:9".into(),
        miniflux_api_key: "x".into(),
        miniflux_webhook_secret: String::new(),
        llama_url: "http://127.0.0.1:9".into(),
        llama_model: "x".into(),
    }));
    (router, dir)
}

async fn status(req: Request<Body>) -> StatusCode {
    let (router, _dir) = test_app();
    router.oneshot(req).await.unwrap().status()
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

#[tokio::test]
async fn paper_routes_need_the_key() {
    for uri in [
        "/api/papers/translate?q=modelos",
        "/api/papers/search?q=modelos",
    ] {
        let req = Request::get(uri).body(Body::empty()).unwrap();
        assert_eq!(status(req).await, StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn empty_paper_search_is_a_bad_request() {
    let req = Request::get("/api/papers/search?q=%20")
        .header(header::AUTHORIZATION, "Bearer secreta")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status(req).await, StatusCode::BAD_REQUEST);
}

/// Respuesta completa: código y mensaje del `ApiError`.
async fn error_of(req: Request<Body>) -> (StatusCode, String) {
    let (router, _dir) = test_app();
    let res = router.oneshot(req).await.unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json["error"].as_str().unwrap().to_owned())
}

/// #45: los errores que genera axum llegan en español.
#[tokio::test]
async fn axum_errors_in_spanish() {
    let post = |body: &'static str, json: bool| {
        let mut req = Request::post("/api/feeds").header(header::AUTHORIZATION, "Bearer secreta");
        if json {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
        req.body(Body::from(body)).unwrap()
    };

    let (code, msg) = error_of(post("{\"feed_url\":", true)).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert!(msg.starts_with("El cuerpo no es JSON válido"), "{msg}");

    let (code, msg) = error_of(post("{}", true)).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(msg.contains("falta el campo `feed_url`"), "{msg}");

    let (code, msg) = error_of(post("{}", false)).await;
    assert_eq!(code, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(msg, "Falta la cabecera Content-Type: application/json");

    let req = Request::delete("/api/feeds/abc")
        .header(header::AUTHORIZATION, "Bearer secreta")
        .body(Body::empty())
        .unwrap();
    let (code, msg) = error_of(req).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert!(
        msg.starts_with("La ruta no es válida: no se puede leer `abc`"),
        "{msg}"
    );

    let req = Request::get("/no-existe").body(Body::empty()).unwrap();
    assert_eq!(
        error_of(req).await,
        (StatusCode::NOT_FOUND, "Esa ruta no existe".to_owned())
    );
}
