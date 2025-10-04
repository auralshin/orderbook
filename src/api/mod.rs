use crate::models::{MatchedOrder, Order};
use crate::order_book::OrderBook;
use crate::websocket::MyWebSocket;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use serde::Deserialize;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Deserialize)]
struct TradingPairPath {
    trading_pair: String,
}

#[derive(Deserialize)]
struct TradingPairOrderPath {
    trading_pair: String,
    order_id: String,
}

pub fn config(cfg: &mut web::ServiceConfig, rx: Arc<Mutex<Receiver<MatchedOrder>>>) {
    cfg.app_data(web::Data::new(rx.clone()));
    cfg.service(web::resource("/ws/{trading_pair}").route(web::get().to(websocket_handler)));
    cfg.service(web::resource("/healthcheck").route(web::get().to(health_check)));
    cfg.service(
        web::scope("/{trading_pair}")
            .service(
                web::resource("/orders")
                    .route(web::post().to(create_order))
                    .route(web::get().to(get_orders)),
            )
            .service(web::resource("/orders/{order_id}").route(web::delete().to(cancel_order)))
            .service(web::resource("/asks").route(web::get().to(get_all_asks)))
            .service(web::resource("/bids").route(web::get().to(get_all_bids)))
            .service(web::resource("/best_bid").route(web::get().to(best_bid)))
            .service(web::resource("/best_ask").route(web::get().to(best_ask))),
    );
}

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("Server is up and running!")
}

async fn create_order(
    path: web::Path<TradingPairPath>,
    order: web::Json<Order>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let order = order.into_inner();
    let trading_pair = path.into_inner().trading_pair;
    let mut order_book = order_book.lock().unwrap(); // Lock the shared OrderBook
    let order_book = order_book.add_order(
        trading_pair.as_str(),
        order,
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );
    println!("{:?}", order_book);
    HttpResponse::Ok().body(format!("{:?}", order_book))
}

async fn get_all_asks(
    path: web::Path<TradingPairPath>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let trading_pair = path.into_inner().trading_pair;
    let order_book = order_book.lock().unwrap();
    let order_book = order_book.get_all_asks(trading_pair.as_str());
    HttpResponse::Ok().json(order_book)
}

async fn get_all_bids(
    path: web::Path<TradingPairPath>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let trading_pair = path.into_inner().trading_pair;
    let order_book = order_book.lock().unwrap();
    let order_book = order_book.get_all_bids(trading_pair.as_str());
    HttpResponse::Ok().json(order_book)
}

async fn get_orders(
    path: web::Path<TradingPairPath>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let trading_pair = path.into_inner().trading_pair;
    let order_book = order_book.lock().unwrap();
    let order_book = order_book.get_orders(trading_pair.as_str());
    HttpResponse::Ok().json(order_book)
}

async fn cancel_order(
    path: web::Path<TradingPairOrderPath>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let params = path.into_inner();
    let mut order_book = order_book.lock().unwrap();
    match order_book.cancel_order(params.trading_pair.as_str(), &params.order_id) {
        Some(order) => HttpResponse::Ok().json(order),
        None => {
            HttpResponse::NotFound().body(format!("Order with ID {} not found", params.order_id))
        }
    }
}

async fn best_bid(
    path: web::Path<TradingPairPath>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let trading_pair = path.into_inner().trading_pair;
    let order_book = order_book.lock().unwrap();
    let best_bid = order_book.get_best_bid(trading_pair.as_str()).copied();
    HttpResponse::Ok().json(best_bid)
}

async fn best_ask(
    path: web::Path<TradingPairPath>,
    order_book: web::Data<Arc<Mutex<OrderBook>>>,
) -> HttpResponse {
    let trading_pair = path.into_inner().trading_pair;
    let order_book = order_book.lock().unwrap();
    let best_ask = order_book.get_best_ask(trading_pair.as_str()).copied();
    HttpResponse::Ok().json(best_ask)
}

async fn websocket_handler(
    path: web::Path<TradingPairPath>,
    req: HttpRequest,
    stream: web::Payload,
    rx: web::Data<Arc<Mutex<Receiver<MatchedOrder>>>>,
) -> Result<HttpResponse, actix_web::Error> {
    let trading_pair = path.into_inner();
    println!(
        "Websocket connection requested for {}",
        trading_pair.trading_pair
    );
    ws::start(MyWebSocket::new(rx.get_ref().clone()), &req, stream)
}
