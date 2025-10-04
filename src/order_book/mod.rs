use crate::models::{BidOrAsk, MatchedOrder, Order, OrderType, Price};
use serde::Deserialize;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, VecDeque};
use tokio::sync::broadcast::Sender;

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

        // Rest only if there's remaining quantity AND a price (i.e., limit order)
        if order.amount > 0.0 {
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

        // If the pair is empty, drop the book
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

    pub fn match_market_order(
        &mut self,
        trading_pair: &str,
        market_order: Order,
    ) -> Vec<MatchedOrder> {
        let mut matched_orders = Vec::new();
        let mut remaining_amount = market_order.amount;

        let maybe_book = self.books.get_mut(trading_pair);
        if maybe_book.is_none() {
            return matched_orders;
        }
        let book = maybe_book.unwrap();

        // Select opposite side to take liquidity from
        let book_side = match market_order.bid_or_ask {
            BidOrAsk::Bid => &mut book.asks, // buy market -> hit asks (ascending)
            BidOrAsk::Ask => &mut book.bids, // sell market -> hit bids (descending)
        };

        while remaining_amount > 0.0 {
            let mut to_remove = Vec::new();
            let mut matched_level = false;

            match market_order.bid_or_ask {
                BidOrAsk::Bid => {
                    // Lowest ask first
                    let mut it = book_side.iter_mut();
                    if let Some((price, orders)) = it.next() {
                        while let Some(mut order) = orders.pop_front() {
                            let id = order.id;
                            let filled_amount = if order.amount <= remaining_amount {
                                remaining_amount -= order.amount;
                                order.amount
                            } else {
                                let filled = remaining_amount;
                                remaining_amount = 0.0;
                                order.amount -= filled;
                                orders.push_front(order);
                                filled
                            };

                            matched_orders.push(MatchedOrder {
                                id: market_order.id,
                                matched_with_id: id,
                                order_type: market_order.order_type.clone(),
                                price: price.clone(),
                                amount: filled_amount,
                                bid_or_ask: market_order.bid_or_ask.clone(),
                                trading_pair: trading_pair.to_string(),
                            });

                            if remaining_amount <= 0.0 {
                                break;
                            }
                        }

                        if orders.is_empty() {
                            to_remove.push(*price);
                        }

                        // We had at least one level to process
                        matched_level = true;
                    }
                }
                BidOrAsk::Ask => {
                    // Highest bid first
                    let mut it = book_side.iter_mut();
                    if let Some((price, orders)) = it.next_back() {
                        while let Some(mut order) = orders.pop_front() {
                            let id = order.id;
                            let filled_amount = if order.amount <= remaining_amount {
                                remaining_amount -= order.amount;
                                order.amount
                            } else {
                                let filled = remaining_amount;
                                remaining_amount = 0.0;
                                order.amount -= filled;
                                orders.push_front(order);
                                filled
                            };

                            matched_orders.push(MatchedOrder {
                                id: market_order.id,
                                matched_with_id: id,
                                order_type: market_order.order_type.clone(),
                                price: price.clone(),
                                amount: filled_amount,
                                bid_or_ask: market_order.bid_or_ask.clone(),
                                trading_pair: trading_pair.to_string(),
                            });

                            if remaining_amount <= 0.0 {
                                break;
                            }
                        }

                        if orders.is_empty() {
                            to_remove.push(*price);
                        }

                        matched_level = true;
                    }
                }
            }

            for price in to_remove {
                book_side.remove(&price);
            }

            // Nothing to match (book empty)
            if !matched_level {
                break;
            }
        }

        for matched_order in &matched_orders {
            if let Some(sender) = self.notifier.as_ref() {
                let _ = sender.send(matched_order.clone());
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
        let bid_or_ask = limit_order.bid_or_ask;

        let order_price = limit_order
            .price
            .expect("Limit orders must specify a price");

        let maybe_book = self.books.get_mut(trading_pair);
        if maybe_book.is_none() {
            return matched_orders;
        }
        let book = maybe_book.unwrap();

        // Select opposite side to take liquidity from
        let book_side = match bid_or_ask {
            BidOrAsk::Bid => &mut book.asks, // buy limit -> hit asks (ascending) up to order_price
            BidOrAsk::Ask => &mut book.bids, // sell limit -> hit bids (descending) down to order_price
        };

        let mut remaining_amount = limit_order.amount;

        while remaining_amount > 0.0 {
            let mut to_remove = Vec::new();
            let mut matched = false;

            if limit_order.bid_or_ask == BidOrAsk::Bid {
                // Ascending asks up to <= order_price
                for (price, orders) in book_side.iter_mut() {
                    if *price > order_price {
                        break;
                    }

                    while let Some(mut order) = orders.pop_front() {
                        let id = order.id;
                        let filled_amount = if order.amount <= remaining_amount {
                            remaining_amount -= order.amount;
                            order.amount
                        } else {
                            let filled = remaining_amount;
                            remaining_amount = 0.0;
                            order.amount -= filled;
                            orders.push_front(order);
                            filled
                        };

                        matched_orders.push(MatchedOrder {
                            id: limit_order.id,
                            matched_with_id: id,
                            order_type: limit_order.order_type.clone(),
                            price: price.clone(),
                            amount: filled_amount,
                            bid_or_ask: limit_order.bid_or_ask.clone(),
                            trading_pair: trading_pair.to_string(),
                        });

                        matched = true;

                        if remaining_amount <= 0.0 {
                            break;
                        }
                    }

                    if orders.is_empty() {
                        to_remove.push(*price);
                    }

                    if matched {
                        break;
                    }
                }
            } else {
                // Descending bids down to >= order_price
                for (price, orders) in book_side.iter_mut().rev() {
                    if *price < order_price {
                        break;
                    }

                    while let Some(mut order) = orders.pop_front() {
                        let id = order.id;
                        let filled_amount = if order.amount <= remaining_amount {
                            remaining_amount -= order.amount;
                            order.amount
                        } else {
                            let filled = remaining_amount;
                            remaining_amount = 0.0;
                            order.amount -= filled;
                            orders.push_front(order);
                            filled
                        };

                        matched_orders.push(MatchedOrder {
                            id: limit_order.id,
                            matched_with_id: id,
                            order_type: limit_order.order_type.clone(),
                            price: price.clone(),
                            amount: filled_amount,
                            bid_or_ask: limit_order.bid_or_ask.clone(),
                            trading_pair: trading_pair.to_string(),
                        });

                        matched = true;

                        if remaining_amount <= 0.0 {
                            break;
                        }
                    }

                    if orders.is_empty() {
                        to_remove.push(*price);
                    }

                    if matched {
                        break;
                    }
                }
            }

            for price in to_remove {
                book_side.remove(&price);
            }

            if !matched {
                break;
            }
        }

        for matched_order in &matched_orders {
            if let Some(sender) = self.notifier.as_ref() {
                let _ = sender.send(matched_order.clone());
            }
        }

        matched_orders
    }
}

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.integral().cmp(&other.integral()) {
            Ordering::Equal => self.fractional().cmp(&other.fractional()),
            other => other,
        }
    }
}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Price {
    fn eq(&self, other: &Self) -> bool {
        self.integral() == other.integral() && self.fractional() == other.fractional()
    }
}

impl Eq for Price {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BidOrAsk, Order, OrderType, Price};

    fn test_order(
        id: u64,
        order_type: OrderType,
        bid_or_ask: BidOrAsk,
        amount: f64,
        price: Option<f64>,
    ) -> Order {
        Order {
            id,
            order_type,
            trading_pair: "BTC-USD".to_string(),
            amount,
            price: price.map(Price::new),
            timestamp: 0,
            bid_or_ask,
        }
    }

    #[test]
    fn test_add_limit_bid_order() {
        let dummy_tx = tokio::sync::broadcast::channel::<MatchedOrder>(64).0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(1, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(10000.0));
        book.add_order("BTC-USD", order, 0);

        let bids = book.get_all_bids("BTC-USD");
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].amount, 1.0);
    }

    #[test]
    fn test_add_limit_ask_order() {
        let dummy_tx = tokio::sync::broadcast::channel::<MatchedOrder>(64).0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(2, OrderType::Limit, BidOrAsk::Ask, 2.0, Some(10500.0));
        book.add_order("BTC-USD", order, 0);

        let asks = book.get_all_asks("BTC-USD");
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].amount, 2.0);
    }

    #[test]
    fn test_match_limit_order_bid_hits_ask() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let ask = test_order(10, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9500.0));
        book.add_order("BTC-USD", ask, 0);

        let bid = test_order(11, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        let matches = book.match_limit_order("BTC-USD", bid);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matched_with_id, 10);
    }

    #[test]
    fn test_partial_fill() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let ask = test_order(1, OrderType::Limit, BidOrAsk::Ask, 2.0, Some(9500.0));
        book.add_order("BTC-USD", ask, 0);

        let bid = test_order(2, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        let matched = book.match_limit_order("BTC-USD", bid);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].amount, 1.0);

        // Check remaining ask
        let remaining = book.get_all_asks("BTC-USD");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].amount, 1.0);
    }

    #[test]
    fn test_best_bid_and_ask() {
        let dummy_tx = tokio::sync::broadcast::channel::<MatchedOrder>(64).0;
        let mut book = OrderBook::new(dummy_tx);

        let ask1 = test_order(1, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9800.0));
        let ask2 = test_order(2, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9700.0));
        book.add_order("BTC-USD", ask1, 0);
        book.add_order("BTC-USD", ask2, 0);

        let bid1 = test_order(3, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9400.0));
        let bid2 = test_order(4, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        book.add_order("BTC-USD", bid1, 0);
        book.add_order("BTC-USD", bid2, 0);

        let best_ask = book.get_best_ask("BTC-USD").unwrap();
        let best_bid = book.get_best_bid("BTC-USD").unwrap();

        assert_eq!(best_ask.integral(), 9700);
        assert_eq!(best_bid.integral(), 9600);
    }

    #[test]
    fn cancel_existing_order() {
        let dummy_tx = tokio::sync::broadcast::channel::<MatchedOrder>(64).0;
        let mut book = OrderBook::new(dummy_tx);

        let ask1 = test_order(1, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9800.0));
        book.add_order("BTC-USD", ask1, 0);

        let canceled_order = book.cancel_order("BTC-USD", "1");
        assert!(canceled_order.is_some());
        assert!(book.get_all_bids("BTC-USD").is_empty());
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let dummy_tx = tokio::sync::broadcast::channel::<MatchedOrder>(64).0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(1, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(10000.0));
        book.add_order("BTC-USD", order, 0);

        let removed = book.cancel_order("BTC-USD", "999");
        assert!(removed.is_none());
    }

    #[test]
    fn test_match_market_order_without_price_uses_book_price() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let ask = test_order(1, OrderType::Limit, BidOrAsk::Ask, 1.5, Some(9500.0));
        book.add_order("BTC-USD", ask, 0);

        let market_bid = test_order(2, OrderType::Market, BidOrAsk::Bid, 1.0, None);
        let matches = book.match_market_order("BTC-USD", market_bid);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].price.integral(), 9500);
        assert_eq!(matches[0].amount, 1.0);

        let remaining_asks = book.get_all_asks("BTC-USD");
        assert_eq!(remaining_asks.len(), 1);
        assert_eq!(remaining_asks[0].amount, 0.5);
    }

    #[test]
    fn test_unmatched_market_order_not_added_to_book() {
        let dummy_tx = tokio::sync::broadcast::channel::<MatchedOrder>(64).0;
        let mut book = OrderBook::new(dummy_tx);

        let market_bid = test_order(1, OrderType::Market, BidOrAsk::Bid, 2.0, None);
        book.add_order("BTC-USD", market_bid, 0);

        assert!(book.get_all_bids("BTC-USD").is_empty());
        assert!(book.get_all_asks("BTC-USD").is_empty());
    }

    #[test]
    fn test_fifo_within_price_level() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        // Two asks at the same price; should match FIFO
        let ask1 = test_order(100, OrderType::Limit, BidOrAsk::Ask, 0.6, Some(9500.0));
        let ask2 = test_order(101, OrderType::Limit, BidOrAsk::Ask, 0.7, Some(9500.0));
        book.add_order("BTC-USD", ask1, 0);
        book.add_order("BTC-USD", ask2, 0);

        let bid = test_order(102, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        let matched = book.match_limit_order("BTC-USD", bid);

        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].matched_with_id, 100);
        assert_eq!(matched[0].amount, 0.6);
        assert_eq!(matched[1].matched_with_id, 101);
        assert!((matched[1].amount - 0.4).abs() < 1e-9);

        let remaining = book.get_all_asks("BTC-USD");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, 101);
        assert!((remaining[0].amount - 0.3).abs() < 1e-9);
    }

    #[test]
    fn test_market_consumes_multiple_ask_levels() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        // Two price levels on the ask side
        let ask_low = test_order(200, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9500.0));
        let ask_high = test_order(201, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9600.0));
        book.add_order("BTC-USD", ask_low, 0);
        book.add_order("BTC-USD", ask_high, 0);

        // Market bid should consume best asks first: 9500 fully, then 9600 partially
        let mkt_bid = test_order(202, OrderType::Market, BidOrAsk::Bid, 1.5, None);
        let matched = book.match_market_order("BTC-USD", mkt_bid);

        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].matched_with_id, 200);
        assert_eq!(matched[0].price.integral(), 9500);
        assert_eq!(matched[0].amount, 1.0);

        assert_eq!(matched[1].matched_with_id, 201);
        assert_eq!(matched[1].price.integral(), 9600);
        assert_eq!(matched[1].amount, 0.5);

        let remaining_asks = book.get_all_asks("BTC-USD");
        assert_eq!(remaining_asks.len(), 1);
        assert_eq!(remaining_asks[0].id, 201);
        assert_eq!(remaining_asks[0].amount, 0.5);
    }

    #[test]
    fn test_limit_ask_crosses_bid_matches_highest_bid_first() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        // Two bids at different prices
        let bid_low = test_order(300, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9400.0));
        let bid_high = test_order(301, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        book.add_order("BTC-USD", bid_low, 0);
        book.add_order("BTC-USD", bid_high, 0);

        // Ask priced to cross both bids; should hit 9600 first
        let ask = test_order(302, OrderType::Limit, BidOrAsk::Ask, 0.8, Some(9500.0));
        let matched = book.match_limit_order("BTC-USD", ask);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].matched_with_id, 301);
        assert_eq!(matched[0].price.integral(), 9600);
        assert!((matched[0].amount - 0.8).abs() < 1e-9);

        // Remaining on the 9600 bid should be ~0.2 (allowing for fp rounding)
        let remaining_bids = book.get_all_bids("BTC-USD");
        let high = remaining_bids.into_iter().find(|o| o.id == 301).unwrap();
        assert!(
            (high.amount - 0.2).abs() < 1e-9,
            "remaining={} expected=0.2",
            high.amount
        );
    }

    #[test]
    fn test_market_ask_hits_highest_bid_first() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        // Two bids: best is 9600
        book.add_order(
            "BTC-USD",
            test_order(1000, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9400.0)),
            0,
        );
        book.add_order(
            "BTC-USD",
            test_order(1001, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0)),
            0,
        );

        let mkt_ask = test_order(1002, OrderType::Market, BidOrAsk::Ask, 1.5, None);
        let matched = book.match_market_order("BTC-USD", mkt_ask);

        assert_eq!(matched.len(), 2);
        // First fill at highest bid 9600
        assert_eq!(matched[0].matched_with_id, 1001);
        assert_eq!(matched[0].price.integral(), 9600);
        assert_eq!(matched[0].amount, 1.0);
        // Then remaining at 9400
        assert_eq!(matched[1].matched_with_id, 1000);
        assert_eq!(matched[1].price.integral(), 9400);
        assert_eq!(matched[1].amount, 0.5);
    }

    #[test]
    fn test_add_order_crossing_limit_remainder_rests() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        // Existing ask
        let ask = test_order(400, OrderType::Limit, BidOrAsk::Ask, 2.0, Some(9500.0));
        book.add_order("BTC-USD", ask, 0);

        // Add a crossing bid via add_order (not calling match_* directly)
        let bid = test_order(401, OrderType::Limit, BidOrAsk::Bid, 1.5, Some(9600.0));
        book.add_order("BTC-USD", bid, 1);

        // Ask should have 0.5 remaining; no resting bid
        let asks = book.get_all_asks("BTC-USD");
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].amount, 0.5);

        let bids = book.get_all_bids("BTC-USD");
        assert!(bids.is_empty());
    }

    #[test]
    fn test_limit_not_crossing_adds_to_book() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let ask = test_order(500, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9800.0));
        book.add_order("BTC-USD", ask, 0);

        let bid = test_order(501, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9700.0));
        book.add_order("BTC-USD", bid, 0);

        // Should not match; best bid and best ask set correctly
        assert_eq!(book.get_best_bid("BTC-USD").unwrap().integral(), 9700);
        assert_eq!(book.get_best_ask("BTC-USD").unwrap().integral(), 9800);
        assert_eq!(book.get_all_bids("BTC-USD").len(), 1);
        assert_eq!(book.get_all_asks("BTC-USD").len(), 1);
    }

    #[test]
    fn test_get_orders_and_by_id() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let bid = test_order(600, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        let ask = test_order(601, OrderType::Limit, BidOrAsk::Ask, 2.0, Some(9800.0));
        book.add_order("BTC-USD", bid, 0);
        book.add_order("BTC-USD", ask, 0);

        let all = book.get_orders("BTC-USD");
        assert_eq!(all.len(), 2);

        let fetched = book.get_order_by_id("BTC-USD", 600);
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().price.as_ref().unwrap().integral(), 9600);
    }

    #[test]
    fn test_cancel_from_bids_and_cleanup() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let bid1 = test_order(700, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        let bid2 = test_order(701, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9500.0));
        book.add_order("BTC-USD", bid1, 0);
        book.add_order("BTC-USD", bid2, 0);

        // Cancel highest bid
        let removed = book.cancel_order("BTC-USD", "700");
        assert!(removed.is_some());

        // Best bid should now be 9500
        assert_eq!(book.get_best_bid("BTC-USD").unwrap().integral(), 9500);

        // Cancel remaining; pair should be effectively empty
        let _ = book.cancel_order("BTC-USD", "701");
        assert!(book.get_all_bids("BTC-USD").is_empty());
        assert!(book.get_all_asks("BTC-USD").is_empty());
        assert!(book.get_best_bid("BTC-USD").is_none());
        assert!(book.get_best_ask("BTC-USD").is_none());
    }

    #[test]
    fn test_best_levels_none_when_empty() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let book = OrderBook::new(tx);
        assert!(book.get_best_bid("BTC-USD").is_none());
        assert!(book.get_best_ask("BTC-USD").is_none());
    }

    #[test]
    fn test_price_ordering_fractional() {
        let p1 = Price::new(9500.25);
        let p2 = Price::new(9500.50);
        assert!(p1 < p2);
        assert_ne!(p1, p2);
        assert_eq!(p1.integral(), 9500);
        assert_eq!(p2.integral(), 9500);
        assert!(p1.fractional() < p2.fractional());
    }

    #[test]
    fn test_notifier_broadcasts_on_matches() {
        use tokio::sync::broadcast::error::TryRecvError;

        let (tx, mut rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        let ask = test_order(800, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9500.0));
        book.add_order("BTC-USD", ask, 0);

        let bid = test_order(801, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
        let _ = book.match_limit_order("BTC-USD", bid);

        let mut seen = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(m) => seen.push(m),
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(_)) => break,
            }
        }

        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].matched_with_id, 800);
        assert_eq!(seen[0].id, 801);
        assert_eq!(seen[0].price.integral(), 9500);
    }

    #[test]
    fn test_get_limit_orders_to_match_collects_all_resting_limits() {
        let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
        let mut book = OrderBook::new(tx);

        // Add 2 bids + 2 asks (none cross)
        book.add_order(
            "BTC-USD",
            test_order(900, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9400.0)),
            0,
        );
        book.add_order(
            "BTC-USD",
            test_order(901, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9300.0)),
            0,
        );
        book.add_order(
            "BTC-USD",
            test_order(902, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9700.0)),
            0,
        );
        book.add_order(
            "BTC-USD",
            test_order(903, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9800.0)),
            0,
        );

        let limits = book.get_limit_orders_to_match("BTC-USD");
        assert_eq!(limits.len(), 4);

        // Market orders should never rest in the book
        let markets = book.get_market_orders_to_match("BTC-USD");
        assert!(markets.is_empty());
    }
}
