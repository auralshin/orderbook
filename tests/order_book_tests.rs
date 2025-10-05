use orderbook::models::{BidOrAsk, MatchedOrder, Order, OrderType, Price, Tif};
use orderbook::order_book::OrderBook;

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
        owner_id: None,
        tif: None,
        post_only: false,
        max_slippage_bps: None,
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

    let ask_low = test_order(200, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9500.0));
    let ask_high = test_order(201, OrderType::Limit, BidOrAsk::Ask, 1.0, Some(9600.0));
    book.add_order("BTC-USD", ask_low, 0);
    book.add_order("BTC-USD", ask_high, 0);

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

    let bid_low = test_order(300, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9400.0));
    let bid_high = test_order(301, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
    book.add_order("BTC-USD", bid_low, 0);
    book.add_order("BTC-USD", bid_high, 0);

    let ask = test_order(302, OrderType::Limit, BidOrAsk::Ask, 0.8, Some(9500.0));
    let matched = book.match_limit_order("BTC-USD", ask);

    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].matched_with_id, 301);
    assert_eq!(matched[0].price.integral(), 9600);
    assert!((matched[0].amount - 0.8).abs() < 1e-9);

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
    assert_eq!(matched[0].matched_with_id, 1001);
    assert_eq!(matched[0].price.integral(), 9600);
    assert_eq!(matched[0].amount, 1.0);
    assert_eq!(matched[1].matched_with_id, 1000);
    assert_eq!(matched[1].price.integral(), 9400);
    assert_eq!(matched[1].amount, 0.5);
}

#[test]
fn test_add_order_crossing_limit_remainder_rests() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
    let mut book = OrderBook::new(tx);

    let ask = test_order(400, OrderType::Limit, BidOrAsk::Ask, 2.0, Some(9500.0));
    book.add_order("BTC-USD", ask, 0);

    let bid = test_order(401, OrderType::Limit, BidOrAsk::Bid, 1.5, Some(9600.0));
    book.add_order("BTC-USD", bid, 1);

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

    let removed = book.cancel_order("BTC-USD", "700");
    assert!(removed.is_some());

    assert_eq!(book.get_best_bid("BTC-USD").unwrap().integral(), 9500);

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

    let markets = book.get_market_orders_to_match("BTC-USD");
    assert!(markets.is_empty());
}

#[test]
fn test_market_bid_skips_stp_blocked_best_ask_and_fills_next() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
    let mut book = OrderBook::new(tx);

    let mut ask_best = test_order(7100, OrderType::Limit, BidOrAsk::Ask, 0.7, Some(10000.0));
    ask_best.owner_id = Some("alice".into());
    book.add_order("BTC-USD", ask_best, 0);

    let mut ask_next = test_order(7101, OrderType::Limit, BidOrAsk::Ask, 0.5, Some(10100.0));
    ask_next.owner_id = Some("bob".into());
    book.add_order("BTC-USD", ask_next, 0);

    let mut mkt_bid = test_order(7102, OrderType::Market, BidOrAsk::Bid, 0.6, None);
    mkt_bid.owner_id = Some("alice".into());
    let matched = book.match_market_order("BTC-USD", mkt_bid);

    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].matched_with_id, 7101);
    assert_eq!(matched[0].price.integral(), 10100);
    assert!((matched[0].amount - 0.5).abs() < 1e-9);

    let asks = book.get_all_asks("BTC-USD");
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].id, 7100);
    assert!((asks[0].amount - 0.7).abs() < 1e-9);
}

#[test]
fn test_market_ask_skips_stp_blocked_best_bid_and_fills_next() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
    let mut book = OrderBook::new(tx);

    let mut bid_best = test_order(7200, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
    bid_best.owner_id = Some("alice".into());
    book.add_order("BTC-USD", bid_best, 0);

    let mut bid_next = test_order(7201, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9400.0));
    bid_next.owner_id = Some("bob".into());
    book.add_order("BTC-USD", bid_next, 0);

    let mut mkt_ask = test_order(7202, OrderType::Market, BidOrAsk::Ask, 0.6, None);
    mkt_ask.owner_id = Some("alice".into());
    let matched = book.match_market_order("BTC-USD", mkt_ask);

    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].matched_with_id, 7201);
    assert_eq!(matched[0].price.integral(), 9400);
    assert!((matched[0].amount - 0.6).abs() < 1e-9);

    let bids = book.get_all_bids("BTC-USD");
    assert_eq!(bids.len(), 2);
    let b9400 = bids.iter().find(|o| o.id == 7201).unwrap();
    let b9600 = bids.iter().find(|o| o.id == 7200).unwrap();
    assert!((b9400.amount - 0.4).abs() < 1e-9);
    assert!((b9600.amount - 1.0).abs() < 1e-9);
}

#[test]
fn test_limit_gtc_skips_stp_blocked_best_and_rests_remainder() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
    let mut book = OrderBook::new(tx);

    let mut bid_best = test_order(7300, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
    bid_best.owner_id = Some("alice".into());
    book.add_order("BTC-USD", bid_best, 0);

    let mut bid_next = test_order(7301, OrderType::Limit, BidOrAsk::Bid, 0.8, Some(9400.0));
    bid_next.owner_id = Some("bob".into());
    book.add_order("BTC-USD", bid_next, 0);

    let mut ask = test_order(7302, OrderType::Limit, BidOrAsk::Ask, 0.6, Some(9500.0));
    ask.owner_id = Some("alice".into());
    book.add_order("BTC-USD", ask, 1);

    let bids = book.get_all_bids("BTC-USD");
    let b9400 = bids.iter().find(|o| o.id == 7301).unwrap();
    let b9600 = bids.iter().find(|o| o.id == 7300).unwrap();
    assert!((b9400.amount - 0.2).abs() < 1e-9);
    assert!((b9600.amount - 1.0).abs() < 1e-9);
}

#[test]
fn test_limit_ioc_skips_stp_blocked_best_and_cancels_remainder() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
    let mut book = OrderBook::new(tx);

    let mut bid_best = test_order(7400, OrderType::Limit, BidOrAsk::Bid, 1.0, Some(9600.0));
    bid_best.owner_id = Some("alice".into());
    book.add_order("BTC-USD", bid_best, 0);

    let mut bid_next = test_order(7401, OrderType::Limit, BidOrAsk::Bid, 0.5, Some(9400.0));
    bid_next.owner_id = Some("bob".into());
    book.add_order("BTC-USD", bid_next, 0);

    let mut ask = test_order(7402, OrderType::Limit, BidOrAsk::Ask, 0.6, Some(9500.0));
    ask.owner_id = Some("alice".into());
    ask.tif = Some(Tif::Ioc);
    book.add_order("BTC-USD", ask, 1);

    let asks = book.get_all_asks("BTC-USD");
    assert!(asks.is_empty());

    let bids = book.get_all_bids("BTC-USD");
    assert_eq!(bids.len(), 1);
    let b9600 = bids.iter().find(|o| o.id == 7400).unwrap();
    assert!((b9600.amount - 1.0).abs() < 1e-9);
}

#[test]
fn test_limit_fok_fails_if_not_fully_fillable_across_eligible_levels() {
    let (tx, _rx) = tokio::sync::broadcast::channel::<MatchedOrder>(64);
    let mut book = OrderBook::new(tx);

    let mut bid_next = test_order(7501, OrderType::Limit, BidOrAsk::Bid, 0.4, Some(9400.0));
    bid_next.owner_id = Some("bob".into());
    book.add_order("BTC-USD", bid_next, 0);

    let mut ask = test_order(7502, OrderType::Limit, BidOrAsk::Ask, 0.6, Some(9500.0));
    ask.owner_id = Some("alice".into());
    ask.tif = Some(Tif::Fok);
    book.add_order("BTC-USD", ask, 1);

    let asks = book.get_all_asks("BTC-USD");
    assert!(asks.is_empty());
    let bids = book.get_all_bids("BTC-USD");
    assert_eq!(bids.len(), 1);
    assert!((bids[0].amount - 0.4).abs() < 1e-9);
}
