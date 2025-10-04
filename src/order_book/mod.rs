use crate::models::{BidOrAsk, MatchedOrder, Order, OrderType, Price, Tif};
use serde::Deserialize;
use serde::Serialize;

use std::collections::{BTreeMap, HashMap, VecDeque};
use tokio::sync::broadcast::Sender;

mod price;

fn can_trade_with(a: &Order, b: &Order) -> bool {
    if let (Some(ida), Some(idb)) = (a.owner_id.as_ref(), b.owner_id.as_ref()) {
        ida != idb
    } else {
        true
    }
}

// Self-Trade Prevention (STP): when both orders have an `owner_id` set and
// they are equal, taker orders must not match against resting orders with the
// same owner. This helper centralizes that logic so behavior is consistent
// across market and limit matching and easier to adjust in the future.

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SideBook {
    pub bids: BTreeMap<Price, VecDeque<Order>>,
    pub asks: BTreeMap<Price, VecDeque<Order>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderBook {
    pub books: HashMap<String, SideBook>,
    #[serde(skip_serializing, skip_deserializing)]
    notifier: Option<Sender<MatchedOrder>>,
}

impl Default for OrderBook {
    fn default() -> Self {
        Self {
            books: HashMap::new(),
            notifier: None,
        }
    }
}

impl OrderBook {
    // ...existing code...

    fn ensure_book(&mut self, trading_pair: &str) -> &mut SideBook {
        self.books
            .entry(trading_pair.to_string())
            .or_insert_with(SideBook::default)
    }

    fn get_book(&self, trading_pair: &str) -> Option<&SideBook> {
        self.books.get(trading_pair)
    }

    pub fn new(notifier: Sender<MatchedOrder>) -> Self {
        Self {
            books: HashMap::new(),
            notifier: Some(notifier),
        }
    }

    pub fn add_order(&mut self, trading_pair: &str, mut order: Order, timestamp: u64) -> &Self {
        order.timestamp = timestamp;
        order.trading_pair = trading_pair.to_string();

        let is_market_order = order.order_type == OrderType::Market;
        let bid_or_ask = order.bid_or_ask.clone();

        // For FOK orders we should only attempt the dry-run if the book exists.
        // Avoid creating an empty book just to run the check.
        if !is_market_order && order.tif == Some(Tif::Fok) {
            if self.get_book(trading_pair).is_none() {
                return self;
            }
            if !self.can_fully_fill_limit(trading_pair, &order) {
                return self;
            }
        }

        self.ensure_book(trading_pair);

        let matched_orders = if is_market_order {
            self.match_market_order(trading_pair, order.clone())
        } else {
            self.match_limit_order(trading_pair, order.clone())
        };

        if !matched_orders.is_empty() {
            let total_matched: f64 = matched_orders.iter().map(|o| o.amount).sum();
            order.amount -= total_matched;
        }

        if order.amount > 0.0 && order.tif != Some(Tif::Ioc) {
            if let Some(price) = order.price.clone() {
                let side_book = self.ensure_book(trading_pair);
                let book = match bid_or_ask {
                    BidOrAsk::Bid => &mut side_book.bids,
                    BidOrAsk::Ask => &mut side_book.asks,
                };
                let entry = book.entry(price).or_insert_with(VecDeque::new);
                entry.push_back(order);
            }
        }

        self
    }

    pub fn cancel_order(&mut self, trading_pair: &str, order_id: &str) -> Option<Order> {
        let mut removed_price = None;
        let mut removed_order = None;

        if let Some(side_book) = self.books.get_mut(trading_pair) {
            for (price, orders) in side_book.bids.iter_mut() {
                if let Some(pos) = orders.iter().position(|o| o.id.to_string() == order_id) {
                    removed_order = orders.remove(pos);
                    if orders.is_empty() {
                        removed_price = Some(*price);
                    }
                    break;
                }
            }
            if let Some(price) = removed_price {
                side_book.bids.remove(&price);
                removed_price = None;
            }

            if removed_order.is_none() {
                for (price, orders) in side_book.asks.iter_mut() {
                    if let Some(pos) = orders.iter().position(|o| o.id.to_string() == order_id) {
                        removed_order = orders.remove(pos);
                        if orders.is_empty() {
                            removed_price = Some(*price);
                        }
                        break;
                    }
                }
                if let Some(price) = removed_price {
                    side_book.asks.remove(&price);
                }
            }
        }

        if self
            .books
            .get(trading_pair)
            .map(|book| book.bids.is_empty() && book.asks.is_empty())
            .unwrap_or(false)
        {
            self.books.remove(trading_pair);
        }

        removed_order
    }

    pub fn get_all_bids(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, q) in side_book.bids.iter() {
                for o in q.iter() {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_all_asks(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, q) in side_book.asks.iter() {
                for o in q.iter() {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_orders(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, q) in side_book.bids.iter() {
                for o in q.iter() {
                    orders.push(o.clone());
                }
            }
            for (_, q) in side_book.asks.iter() {
                for o in q.iter() {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_order_by_id(&self, trading_pair: &str, id: u64) -> Option<&Order> {
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, orders) in side_book.bids.iter() {
                for order in orders.iter() {
                    if order.id == id {
                        return Some(order);
                    }
                }
            }
            for (_, orders) in side_book.asks.iter() {
                for order in orders.iter() {
                    if order.id == id {
                        return Some(order);
                    }
                }
            }
        }
        None
    }

    pub fn get_market_orders_to_match(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, q) in side_book.bids.iter() {
                for o in q.iter() {
                    if o.order_type == OrderType::Market {
                        orders.push(o.clone());
                    }
                }
            }
            for (_, q) in side_book.asks.iter() {
                for o in q.iter() {
                    if o.order_type == OrderType::Market {
                        orders.push(o.clone());
                    }
                }
            }
        }
        orders
    }

    pub fn get_limit_orders_to_match(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, q) in side_book.bids.iter() {
                for o in q.iter() {
                    if o.order_type == OrderType::Limit {
                        orders.push(o.clone());
                    }
                }
            }
            for (_, q) in side_book.asks.iter() {
                for o in q.iter() {
                    if o.order_type == OrderType::Limit {
                        orders.push(o.clone());
                    }
                }
            }
        }
        orders
    }

    pub fn get_best_bid(&self, trading_pair: &str) -> Option<&Price> {
        self.get_book(trading_pair)
            .and_then(|book| book.bids.keys().next_back())
    }

    pub fn get_best_ask(&self, trading_pair: &str) -> Option<&Price> {
        self.get_book(trading_pair)
            .and_then(|book| book.asks.keys().next())
    }

    fn can_fully_fill_limit(&self, trading_pair: &str, limit_order: &Order) -> bool {
        let mut remaining = limit_order.amount;
        let order_price = match limit_order.price {
            Some(ref p) => p.clone(),
            None => return false,
        };

        let Some(book) = self.books.get(trading_pair) else {
            return false;
        };

        let book_side = match limit_order.bid_or_ask {
            BidOrAsk::Bid => &book.asks,
            BidOrAsk::Ask => &book.bids,
        };

        match limit_order.bid_or_ask {
            BidOrAsk::Bid => {
                for (price, orders) in book_side.iter() {
                    if *price > order_price {
                        break;
                    }
                    for o in orders.iter() {
                        if !can_trade_with(limit_order, o) {
                            continue;
                        }
                        if o.amount >= remaining {
                            return true;
                        }
                        remaining -= o.amount;
                    }
                }
            }
            BidOrAsk::Ask => {
                for (price, orders) in book_side.iter().rev() {
                    if *price < order_price {
                        break;
                    }
                    for o in orders.iter() {
                        if !can_trade_with(limit_order, o) {
                            continue;
                        }
                        if o.amount >= remaining {
                            return true;
                        }
                        remaining -= o.amount;
                    }
                }
            }
        }

        false
    }

    pub fn match_market_order(
        &mut self,
        trading_pair: &str,
        market_order: Order,
    ) -> Vec<MatchedOrder> {
        let mut matched_orders = Vec::new();
        let mut remaining_amount = market_order.amount;

        let Some(book) = self.books.get_mut(trading_pair) else {
            return matched_orders;
        };

        let book_side = match market_order.bid_or_ask {
            BidOrAsk::Bid => &mut book.asks,
            BidOrAsk::Ask => &mut book.bids,
        };

        let mut prices: Vec<Price> = book_side.keys().cloned().collect();
        if matches!(market_order.bid_or_ask, BidOrAsk::Ask) {
            prices.reverse();
        }

        let mut to_remove = Vec::new();

        'levels: for price in prices.iter() {
            if remaining_amount <= 0.0 {
                break;
            }

            if let Some(orders) = book_side.get_mut(price) {
                loop {
                    let pos = orders
                        .iter()
                        .position(|ord| can_trade_with(&market_order, ord));

                    if let Some(p) = pos {
                        let mut order = orders.remove(p).expect("valid index");
                        let id = order.id;

                        let filled_amount = if order.amount <= remaining_amount {
                            remaining_amount -= order.amount;
                            order.amount
                        } else {
                            let filled = remaining_amount;
                            remaining_amount = 0.0;
                            order.amount -= filled;
                            orders.insert(p, order);
                            filled
                        };

                        matched_orders.push(MatchedOrder {
                            id: market_order.id,
                            matched_with_id: id,
                            order_type: market_order.order_type.clone(),
                            price: *price,
                            amount: filled_amount,
                            bid_or_ask: market_order.bid_or_ask.clone(),
                            trading_pair: trading_pair.to_string(),
                        });

                        if remaining_amount <= 0.0 {
                            break 'levels;
                        }

                        continue;
                    } else {
                        break;
                    }
                }

                if orders.is_empty() {
                    to_remove.push(*price);
                }
            }
        }

        for p in to_remove {
            book_side.remove(&p);
        }

        if let Some(tx) = self.notifier.as_ref() {
            for m in &matched_orders {
                let _ = tx.send(m.clone());
            }
        }

        matched_orders
    }

    pub fn match_limit_order(
        &mut self,
        trading_pair: &str,
        limit_order: Order,
    ) -> Vec<MatchedOrder> {
        let mut matched_orders = Vec::new();

        let order_price = limit_order
            .price
            .expect("Limit orders must specify a price");

        let Some(book) = self.books.get_mut(trading_pair) else {
            return matched_orders;
        };

        let book_side = match limit_order.bid_or_ask {
            BidOrAsk::Bid => &mut book.asks,
            BidOrAsk::Ask => &mut book.bids,
        };

        let mut remaining_amount = limit_order.amount;

        match limit_order.bid_or_ask {
            BidOrAsk::Bid => {
                let mut to_remove = Vec::new();
                let mut encountered_eligible = false;
                let mut matched_in_eligible = false;

                for (price, orders) in book_side.iter_mut() {
                    let eligible_here = *price <= order_price;
                    if eligible_here {
                        encountered_eligible = true;
                    } else {
                        // If we hit a non-eligible price and either we haven't seen any eligible
                        // prices yet, or we've matched at an eligible price already, then stop.
                        if !encountered_eligible || matched_in_eligible {
                            break;
                        }
                        // Otherwise, we encountered eligible prices but didn't match (STP); continue scanning.
                    }

                    loop {
                        let pos = orders.iter().position(|o| can_trade_with(&limit_order, o));
                        if let Some(p) = pos {
                            let mut order = orders.remove(p).expect("valid index");
                            let id = order.id;
                            let filled_amount = if order.amount <= remaining_amount {
                                remaining_amount -= order.amount;
                                order.amount
                            } else {
                                let filled = remaining_amount;
                                remaining_amount = 0.0;
                                order.amount -= filled;
                                orders.insert(p, order);
                                filled
                            };

                            matched_orders.push(MatchedOrder {
                                id: limit_order.id,
                                matched_with_id: id,
                                order_type: limit_order.order_type.clone(),
                                price: *price,
                                amount: filled_amount,
                                bid_or_ask: limit_order.bid_or_ask.clone(),
                                trading_pair: trading_pair.to_string(),
                            });

                            if eligible_here {
                                matched_in_eligible = true;
                            }

                            if remaining_amount <= 0.0 {
                                break;
                            }

                            continue;
                        } else {
                            break;
                        }
                    }

                    if orders.is_empty() {
                        to_remove.push(*price);
                    }

                    if remaining_amount <= 0.0 {
                        break;
                    }
                }

                for p in to_remove {
                    book_side.remove(&p);
                }
            }
            BidOrAsk::Ask => {
                let mut to_remove = Vec::new();
                let mut encountered_eligible = false;
                let mut matched_in_eligible = false;

                for (price, orders) in book_side.iter_mut().rev() {
                    let eligible_here = *price >= order_price;
                    if eligible_here {
                        encountered_eligible = true;
                    } else {
                        if !encountered_eligible || matched_in_eligible {
                            break;
                        }
                    }

                    loop {
                        let pos = orders.iter().position(|o| can_trade_with(&limit_order, o));
                        if let Some(p) = pos {
                            let mut order = orders.remove(p).expect("valid index");
                            let id = order.id;
                            let filled_amount = if order.amount <= remaining_amount {
                                remaining_amount -= order.amount;
                                order.amount
                            } else {
                                let filled = remaining_amount;
                                remaining_amount = 0.0;
                                order.amount -= filled;
                                orders.insert(p, order);
                                filled
                            };

                            matched_orders.push(MatchedOrder {
                                id: limit_order.id,
                                matched_with_id: id,
                                order_type: limit_order.order_type.clone(),
                                price: *price,
                                amount: filled_amount,
                                bid_or_ask: limit_order.bid_or_ask.clone(),
                                trading_pair: trading_pair.to_string(),
                            });

                            if eligible_here {
                                matched_in_eligible = true;
                            }

                            if remaining_amount <= 0.0 {
                                break;
                            }

                            continue;
                        } else {
                            break;
                        }
                    }

                    if orders.is_empty() {
                        to_remove.push(*price);
                    }

                    if remaining_amount <= 0.0 {
                        break;
                    }
                }

                for p in to_remove {
                    book_side.remove(&p);
                }
            }
        }

        if let Some(sender) = self.notifier.as_ref() {
            for m in &matched_orders {
                let _ = sender.send(m.clone());
            }
        }

        matched_orders
    }
}
