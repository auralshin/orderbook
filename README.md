# Rust OrderBook
This Rust project implements an OrderBook for a cryptocurrency exchange, allowing for efficient management and matching of market and limit orders for various trading pairs (e.g., BTC-USD). The OrderBook maintains separate lists for buy (bid) and sell (ask) orders and provides functionality for adding orders, matching market and limit orders, and querying the current state of the order book.

## Features
- Order Management: Add market and limit orders to the order book.
- Order Matching: Match market orders immediately with existing limit orders and limit orders with the best available market orders.
- Querying Orders: Retrieve all orders, either bids or asks, and orders based on specific criteria.
- Notifier Integration: Integration with websocket to inform about matched orders.
- Order Priority: Orders are managed based on price and timestamp, ensuring fair and efficient matching.