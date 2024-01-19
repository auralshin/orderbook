mod api;
mod order;
mod order_book;
mod websocket;
use actix_cors::Cors;
use actix_web::{http, App, HttpServer};
use order::MatchedOrder;
use order_book::OrderBook;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (tx, rx) = mpsc::channel::<MatchedOrder>();
    let order_book = Arc::new(Mutex::new(OrderBook::new(tx)));
    let rx = Arc::new(Mutex::new(rx));
    HttpServer::new(move || {
        let order_book = Arc::clone(&order_book);
        let rx_clone = Arc::clone(&rx);
        let cors = Cors::default()
            .allowed_origin("http://localhost:3000") // Add your frontend url here
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .max_age(3600);

        App::new()
            .wrap(cors)
            .data(order_book) // Share the OrderBook state with the app
            .configure(|cfg| api::config(cfg, rx_clone)) // Configure your API routes
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
