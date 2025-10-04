use actix::{Actor, AsyncContext, StreamHandler};
use actix_web_actors::ws;
use tokio::sync::broadcast::{error::TryRecvError, Receiver};

use crate::models::MatchedOrder;

pub struct MyWebSocket {
    rx: Receiver<MatchedOrder>,
}

impl MyWebSocket {
    pub fn new(rx: Receiver<MatchedOrder>) -> Self {
        MyWebSocket { rx }
    }

    fn drain_messages(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        loop {
            match self.rx.try_recv() {
                Ok(matched_order) => {
                    if let Ok(order_info) = serde_json::to_string(&matched_order) {
                        ctx.text(order_info);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => break,
            }
        }
    }
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(std::time::Duration::from_millis(200), |actor, ctx| {
            actor.drain_messages(ctx);
        });
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        println!("WS: {:?}", msg);
        self.drain_messages(ctx);
    }
}

