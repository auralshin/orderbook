use actix_web::{test, App};
use orderbook::api;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[actix_web::test]
async fn test_health_check() {
    let (_tx, rx) = mpsc::channel();
    let rx = Arc::new(Mutex::new(rx));

    let app = test::init_service(App::new().configure(|cfg| api::config(cfg, rx.clone()))).await;

    let req = test::TestRequest::get().uri("/healthcheck").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
}
