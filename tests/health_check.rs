use actix_web::{test, App};
use orderbook::api;
use tokio::sync::broadcast;

#[actix_web::test]
async fn test_health_check() {
    let (tx, _rx) = broadcast::channel::<orderbook::models::MatchedOrder>(64);

    let app = test::init_service(App::new().configure(|cfg| api::config(cfg, tx.clone()))).await;

    let req = test::TestRequest::get().uri("/healthcheck").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
}
