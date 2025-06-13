use crate::models::{BidOrAsk, MatchedOrder, Order, OrderType, Price};
use serde::Deserialize;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::Sender;

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderBook {
    pub bids: BTreeMap<Price, VecDeque<Order>>,
    pub asks: BTreeMap<Price, VecDeque<Order>>,
    #[serde(skip_serializing, skip_deserializing)]
    notifier: Option<Sender<MatchedOrder>>,
}

impl Default for OrderBook {
    fn default() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            notifier: None,
        }
    }
}

impl OrderBook {
    pub fn new(notifier: Sender<MatchedOrder>) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            notifier: Some(notifier),
        }
    }

    pub fn add_order(&mut self, mut order: Order, timestamp: u64) -> &Self {
        order.timestamp = timestamp;

        let is_market_order = order.order_type == OrderType::Market;
        let bid_or_ask = order.bid_or_ask.clone();

        let price = order.price.clone().unwrap();

        let matched_orders = if is_market_order {
            self.match_market_order(order.clone())
        } else {
            self.match_limit_order(order.clone())
        };

        if !matched_orders.is_empty() {
            let total_matched: f64 = matched_orders.iter().map(|o| o.amount).sum();
            order.amount -= total_matched;
        }

        if order.amount > 0.0 {
            let book = match bid_or_ask {
                BidOrAsk::Bid => &mut self.bids,
                BidOrAsk::Ask => &mut self.asks,
            };
            let entry = book.entry(price).or_insert_with(VecDeque::new);
            entry.push_back(order);
        }

        self
    }

    pub fn get_all_bids(&self) -> Vec<Order> {
        let mut orders = Vec::new();
        for (_, order) in self.bids.iter() {
            for o in order.iter() {
                orders.push(o.clone());
            }
        }
        orders
    }

    pub fn get_all_asks(&self) -> Vec<Order> {
        let mut orders = Vec::new();
        for (_, order) in self.asks.iter() {
            for o in order.iter() {
                orders.push(o.clone());
            }
        }
        orders
    }

    pub fn get_orders(&self) -> Vec<Order> {
        let mut orders = Vec::new();
        for (_, order) in self.bids.iter() {
            for o in order.iter() {
                orders.push(o.clone());
            }
        }
        for (_, order) in self.asks.iter() {
            for o in order.iter() {
                orders.push(o.clone());
            }
        }
        orders
    }

    pub fn get_order_by_id(&self, id: u64) -> Option<&Order> {
        for (_, orders) in self.bids.iter() {
            for order in orders.iter() {
                if order.id == id {
                    return Some(order);
                }
            }
        }
        for (_, orders) in self.asks.iter() {
            for order in orders.iter() {
                if order.id == id {
                    return Some(order);
                }
            }
        }
        None
    }

    pub fn get_market_orders_to_match(&self) -> Vec<Order> {
        let mut orders = Vec::new();
        for (_, order) in self.bids.iter() {
            for o in order.iter() {
                if o.order_type == OrderType::Market {
                    orders.push(o.clone());
                }
            }
        }
        for (_, order) in self.asks.iter() {
            for o in order.iter() {
                if o.order_type == OrderType::Market {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_limit_orders_to_match(&self) -> Vec<Order> {
        let mut orders = Vec::new();
        for (_, order) in self.bids.iter() {
            for o in order.iter() {
                if o.order_type == OrderType::Limit {
                    orders.push(o.clone());
                }
            }
        }
        for (_, order) in self.asks.iter() {
            for o in order.iter() {
                if o.order_type == OrderType::Limit {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_best_bid(&self) -> Option<&Price> {
        self.bids.keys().next_back()
    }

    pub fn get_best_ask(&self) -> Option<&Price> {
        self.asks.keys().next()
    }

    pub fn match_market_order(&mut self, market_order: Order) -> Vec<MatchedOrder> {
        let mut matched_orders = Vec::new();
        let mut removal_candidates = Vec::new();
        let mut remaining_amount = market_order.amount;
        let _order_id = market_order.id;

        let book = match market_order.bid_or_ask {
            BidOrAsk::Bid => &mut self.asks,
            BidOrAsk::Ask => &mut self.bids,
        };

        if let Some((first_price, _)) = book.iter().next() {
            if market_order.bid_or_ask == BidOrAsk::Bid
                && *first_price >= market_order.price.unwrap_or(Price::new(f64::MAX))
            {
                return matched_orders;
            }
            if market_order.bid_or_ask == BidOrAsk::Ask
                && *first_price < market_order.price.unwrap_or(Price::new(f64::MIN))
            {
                return matched_orders;
            }
        }
        let mut book_iter = book.iter_mut();

        while remaining_amount > 0.0 {
            if let Some((price, orders)) = book_iter.next() {
                while let Some(mut order) = orders.pop_front() {
                    let id = order.id;
                    let filled_amount = if order.amount <= remaining_amount {
                        remaining_amount -= order.amount;
                        order.amount
                    } else {
                        let filled_amount = remaining_amount;
                        remaining_amount = 0.0;
                        order.amount -= filled_amount;
                        orders.push_front(order);
                        filled_amount
                    };

                    matched_orders.push(MatchedOrder {
                        id: market_order.id,
                        matched_with_id: id,
                        order_type: market_order.order_type.clone(),
                        price: price.clone(),
                        amount: filled_amount,
                        bid_or_ask: market_order.bid_or_ask.clone(),
                    });

                    if remaining_amount <= 0.0 {
                        break;
                    }
                }

                if orders.is_empty() {
                    removal_candidates.push(*price);
                }
            } else {
                break;
            }
        }

        for price in removal_candidates {
            book.remove(&price);
        }
        for matched_order in &matched_orders {
            if let Some(sender) = self.notifier.as_ref() {
                sender.send(matched_order.clone()).unwrap();
            }
        }

        matched_orders
    }

    pub fn match_limit_order(&mut self, limit_order: Order) -> Vec<MatchedOrder> {
        let mut matched_orders = Vec::new();
        let order_type = limit_order.order_type;
        let bid_or_ask = limit_order.bid_or_ask;
        let order_price = limit_order.price; // Clone the price if necessary
        let order_id = limit_order.id;

        let book = match bid_or_ask {
            BidOrAsk::Bid => &mut self.asks,
            BidOrAsk::Ask => &mut self.bids,
        };

        let mut remaining_amount = limit_order.amount;

        while remaining_amount > 0.0 {
            let mut to_remove = Vec::new();
            let mut matched = false;

            for (price, orders) in book.iter_mut() {
                if (limit_order.bid_or_ask == BidOrAsk::Bid && *price > limit_order.price.unwrap())
                    || (limit_order.bid_or_ask == BidOrAsk::Ask
                        && *price < limit_order.price.unwrap())
                {
                    break;
                }

                while let Some(mut order) = orders.pop_front() {
                    let id = order.id;
                    let filled_amount = if order.amount <= remaining_amount {
                        remaining_amount -= order.amount;
                        order.amount
                    } else {
                        let filled_amount = remaining_amount;
                        remaining_amount = 0.0;
                        order.amount -= filled_amount;
                        orders.push_front(order);
                        filled_amount
                    };

                    matched_orders.push(MatchedOrder {
                        id: limit_order.id,
                        matched_with_id: id,
                        order_type: limit_order.order_type.clone(),
                        price: price.clone(),
                        amount: filled_amount,

                        bid_or_ask: limit_order.bid_or_ask.clone(),
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

            for price in to_remove {
                book.remove(&price);
            }

            if !matched {
                break;
            }
        }

        for matched_order in &matched_orders {
            self.notifier
                .as_ref()
                .expect("Notifier is not initialized")
                .send(matched_order.clone())
                .unwrap();
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
        price: f64,
    ) -> Order {
        Order {
            id,
            order_type,
            trading_pair: "BTC-USD".to_string(),
            amount,
            price: Some(Price::new(price)),
            timestamp: 0,
            bid_or_ask,
        }
    }

    #[test]
    fn test_add_limit_bid_order() {
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(1, OrderType::Limit, BidOrAsk::Bid, 1.0, 10000.0);
        book.add_order(order, 0);

        let bids = book.get_all_bids();
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].amount, 1.0);
    }

    #[test]
    fn test_add_limit_ask_order() {
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(2, OrderType::Limit, BidOrAsk::Ask, 2.0, 10500.0);
        book.add_order(order, 0);

        let asks = book.get_all_asks();
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].amount, 2.0);
    }

    #[test]
    fn test_match_limit_order_bid_hits_ask() {
        let (tx, _rx) = std::sync::mpsc::channel::<MatchedOrder>();
        let mut book = OrderBook::new(tx);

        let ask = test_order(10, OrderType::Limit, BidOrAsk::Ask, 1.0, 9500.0);
        book.add_order(ask, 0);

        let bid = test_order(11, OrderType::Limit, BidOrAsk::Bid, 1.0, 9600.0);
        let matches = book.match_limit_order(bid);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matched_with_id, 10);
    }

    #[test]
    fn test_partial_fill() {
        let (tx, _rx) = std::sync::mpsc::channel::<MatchedOrder>();
        let mut book = OrderBook::new(tx);

        let ask = test_order(1, OrderType::Limit, BidOrAsk::Ask, 2.0, 9500.0);
        book.add_order(ask, 0);

        let bid = test_order(2, OrderType::Limit, BidOrAsk::Bid, 1.0, 9600.0);
        let matched = book.match_limit_order(bid);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].amount, 1.0);

        // Check remaining ask
        let remaining = book.get_all_asks();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].amount, 1.0);
    }

    #[test]
    fn test_best_bid_and_ask() {
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let ask1 = test_order(1, OrderType::Limit, BidOrAsk::Ask, 1.0, 9800.0);
        let ask2 = test_order(2, OrderType::Limit, BidOrAsk::Ask, 1.0, 9700.0);
        book.add_order(ask1, 0);
        book.add_order(ask2, 0);

        let bid1 = test_order(3, OrderType::Limit, BidOrAsk::Bid, 1.0, 9400.0);
        let bid2 = test_order(4, OrderType::Limit, BidOrAsk::Bid, 1.0, 9600.0);
        book.add_order(bid1, 0);
        book.add_order(bid2, 0);

        let best_ask = book.get_best_ask().unwrap();
        let best_bid = book.get_best_bid().unwrap();

        assert_eq!(best_ask.integral(), 9700);
        assert_eq!(best_bid.integral(), 9600);
    }
}
