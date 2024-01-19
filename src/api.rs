use crate::order::{self, MatchedOrder, Order};
use crate::order_book::OrderBook;
use crate::websocket::MyWebSocket;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::SystemTime; // Add the missing import statement

// Define your API configuration function
pub fn config(cfg: &mut web::ServiceConfig, rx: Arc<Mutex<Receiver<MatchedOrder>>>) {
    cfg.service(web::resource("/ws/").route(web::get().to(
        move |r: HttpRequest, stream: web::Payload| {
            let rx_clone = rx.clone(); // Clone the Arc for each request
            async move { ws::start(MyWebSocket::new(rx_clone), &r, stream) }
        },
    )));
    cfg.service(web::resource("/healthcheck").route(web::get().to(health_check)));
    cfg.service(
        web::resource("/orders")
            .route(web::post().to(create_order))
            .route(web::get().to(get_orders)),
    );
    cfg.service(web::resource("/asks").route(web::get().to(get_all_asks)));
    cfg.service(web::resource("/bids").route(web::get().to(get_all_bids)));
}

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("Server is up and running!")
}

async fn create_order(
    order: web::Json<Order>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let order = order.into_inner();
    let mut order_book = order_book.lock().unwrap(); // Lock the shared OrderBook
    let order_book = order_book.add_order(
        order,
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );
    println!("{:?}", order_book);
    HttpResponse::Ok().body(format!("{:?}", order_book))
}

async fn get_all_asks(order_book: web::Data<Arc<Mutex<OrderBook>>>) -> HttpResponse {
    let order_book = order_book.lock().unwrap();
    let order_book = order_book.get_all_asks();
    HttpResponse::Ok().json(order_book)
}

async fn get_all_bids(order_book: web::Data<Arc<Mutex<OrderBook>>>) -> HttpResponse {
    let order_book = order_book.lock().unwrap();
    let order_book = order_book.get_all_bids();
    HttpResponse::Ok().json(order_book)
}

async fn get_orders(order_book: web::Data<Arc<Mutex<OrderBook>>>) -> HttpResponse {
    let order_book = order_book.lock().unwrap();
    let order_book = order_book.get_orders();
    HttpResponse::Ok().json(order_book)
}
