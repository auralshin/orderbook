use std::sync::mpsc::{self, Receiver};

use actix::{Actor, StreamHandler};
use actix_web_actors::ws;

use crate::models::MatchedOrder;
use actix::AsyncContext;
use std::sync::{Arc, Mutex};
pub struct MyWebSocket {
    rx: Arc<Mutex<Receiver<MatchedOrder>>>,
}

impl MyWebSocket {
    pub fn new(rx: Arc<Mutex<Receiver<MatchedOrder>>>) -> Self {
        MyWebSocket { rx }
    }
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(std::time::Duration::from_secs(1), |actor, ctx| {
            if let Ok(rx_lock) = actor.rx.lock() {
                while let Ok(matched_order) = rx_lock.try_recv() {
                    let order_info = serde_json::to_string(&matched_order).unwrap();
                    ctx.text(order_info);
                }
            }
        });
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        println!("WS: {:?}", msg);
        if let Ok(rx_lock) = self.rx.lock() {
            while let Ok(matched_order) = rx_lock.try_recv() {
                let order_info = serde_json::to_string(&matched_order).unwrap();
                print!("Sending: {}", order_info);
                ctx.text(order_info);
            }
        }
    }
}
