use actix_cors::Cors;
use actix_web::{http, web, App, HttpServer};
use models::MatchedOrder;
use order_book::OrderBook;
use orderbook::{api, models, order_book};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (tx, _rx) = broadcast::channel::<MatchedOrder>(64);
    let order_book = Arc::new(Mutex::new(OrderBook::new(tx.clone())));
    HttpServer::new(move || {
        let order_book = Arc::clone(&order_book);
        let tx_clone = tx.clone();
        let cors = Cors::default()
            .allowed_origin("http://localhost:3000") // Add your frontend url here
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(order_book)) // Share the OrderBook state with the app
            .configure(|cfg| api::config(cfg, tx_clone.clone())) // Configure your API routes
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
