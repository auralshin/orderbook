use crate::models::{BidOrAsk, MatchedOrder, Order, OrderType, Price};
use serde::Deserialize;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::mpsc::Sender;

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
            for (_, order) in side_book.bids.iter() {
                for o in order.iter() {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_all_asks(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, order) in side_book.asks.iter() {
                for o in order.iter() {
                    orders.push(o.clone());
                }
            }
        }
        orders
    }

    pub fn get_orders(&self, trading_pair: &str) -> Vec<Order> {
        let mut orders = Vec::new();
        if let Some(side_book) = self.get_book(trading_pair) {
            for (_, order) in side_book.bids.iter() {
                for o in order.iter() {
                    orders.push(o.clone());
                }
            }
            for (_, order) in side_book.asks.iter() {
                for o in order.iter() {
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
            for (_, order) in side_book.bids.iter() {
                for o in order.iter() {
                    if o.order_type == OrderType::Market {
                        orders.push(o.clone());
                    }
                }
            }
            for (_, order) in side_book.asks.iter() {
                for o in order.iter() {
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
            for (_, order) in side_book.bids.iter() {
                for o in order.iter() {
                    if o.order_type == OrderType::Limit {
                        orders.push(o.clone());
                    }
                }
            }
            for (_, order) in side_book.asks.iter() {
                for o in order.iter() {
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
        let mut removal_candidates = Vec::new();
        let mut remaining_amount = market_order.amount;

        let maybe_book = self.books.get_mut(trading_pair);
        if maybe_book.is_none() {
            return matched_orders;
        }
        let book = maybe_book.unwrap();

        let book_side = match market_order.bid_or_ask {
            BidOrAsk::Bid => &mut book.asks,
            BidOrAsk::Ask => &mut book.bids,
        };

        let mut book_iter = book_side.iter_mut();

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
                        trading_pair: trading_pair.to_string(),
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
            book_side.remove(&price);
        }
        for matched_order in &matched_orders {
            if let Some(sender) = self.notifier.as_ref() {
                sender.send(matched_order.clone()).unwrap();
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

        let book_side = match bid_or_ask {
            BidOrAsk::Bid => &mut book.asks,
            BidOrAsk::Ask => &mut book.bids,
        };

        let mut remaining_amount = limit_order.amount;

        while remaining_amount > 0.0 {
            let mut to_remove = Vec::new();
            let mut matched = false;

            for (price, orders) in book_side.iter_mut() {
                if (limit_order.bid_or_ask == BidOrAsk::Bid && *price > order_price)
                    || (limit_order.bid_or_ask == BidOrAsk::Ask && *price < order_price)
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

            for price in to_remove {
                book_side.remove(&price);
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
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(1, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(10000.0));
        book.add_order("BTC-USD", order, 0);

        let bids = book.get_all_bids("BTC-USD");
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].amount, 1.0);
    }

    #[test]
    fn test_add_limit_ask_order() {
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(2, OrderType::Limit, BidOrAsk::Ask, 2.0, Some(10500.0));
        book.add_order("BTC-USD", order, 0);

        let asks = book.get_all_asks("BTC-USD");
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].amount, 2.0);
    }

    #[test]
    fn test_match_limit_order_bid_hits_ask() {
        let (tx, _rx) = std::sync::mpsc::channel::<MatchedOrder>();
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
        let (tx, _rx) = std::sync::mpsc::channel::<MatchedOrder>();
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
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
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
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let ask1 = test_order(1, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9800.0));
        book.add_order("BTC-USD", ask1, 0);

        let canceled_order = book.cancel_order("BTC-USD", "1");
        assert!(canceled_order.is_some());
        assert!(book.get_all_bids("BTC-USD").is_empty());
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let order = test_order(1, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(10000.0));
        book.add_order("BTC-USD", order, 0);

        let removed = book.cancel_order("BTC-USD", "999");
        assert!(removed.is_none());
    }

    #[test]
    fn test_match_market_order_without_price_uses_book_price() {
        let (tx, _rx) = std::sync::mpsc::channel::<MatchedOrder>();
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
        let dummy_tx = std::sync::mpsc::channel::<MatchedOrder>().0;
        let mut book = OrderBook::new(dummy_tx);

        let market_bid = test_order(1, OrderType::Market, BidOrAsk::Bid, 2.0, None);
        book.add_order("BTC-USD", market_bid, 0);

        assert!(book.get_all_bids("BTC-USD").is_empty());
        assert!(book.get_all_asks("BTC-USD").is_empty());
    }
}
