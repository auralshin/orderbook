use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub enum BidOrAsk {
    Bid,
    Ask,
}

#[derive(Debug, PartialEq, Clone, Copy, Deserialize, Serialize)]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Hash, Clone, Copy, Deserialize, Serialize)]
pub struct Price {
    integral: u64,
    fractional: u64,
    scalar: u64,
}

impl Price {
    pub fn new(price: f64) -> Price {
        let scalar = 100000;
        let integral = price as u64;
        let fractional = ((price % 1.0) * scalar as f64) as u64;
        Price {
            scalar,
            integral,
            fractional,
        }
    }
    pub fn integral(&self) -> u64 {
        self.integral
    }

    pub fn fractional(&self) -> u64 {
        self.fractional
    }

    pub fn scalar(&self) -> u64 {
        self.scalar
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Order {
    pub id: u64,
    pub order_type: OrderType,
    pub trading_pair: String,
    pub amount: f64,
    pub price: Option<Price>,
    pub timestamp: u64,
    pub bid_or_ask: BidOrAsk,
}

impl Order {
    pub fn new(
        id: u64,
        order_type: OrderType,
        trading_pair: String,
        amount: f64,
        price: Option<Price>,
        timestamp: u64,
        bid_or_ask: BidOrAsk,
    ) -> Self {
        Self {
            id,
            order_type,
            trading_pair,
            amount,
            price,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            bid_or_ask,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MatchedOrder {
    pub id: u64,
    pub matched_with_id: u64,
    pub order_type: OrderType,
    pub price: Price,
    pub amount: f64,
    pub bid_or_ask: BidOrAsk,
}
