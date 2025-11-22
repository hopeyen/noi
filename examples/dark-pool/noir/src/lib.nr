struct Order {
    price: u64,
    size: u64,
}

#[export]
fn match_orders(bid: Order, ask: Order, min_size: u64) -> bool {
    touch_dark_pool_exports();
    let price_ok = bid.price >= ask.price;
    let bid_ok = bid.size >= min_size;
    let ask_ok = ask.size >= min_size;
    price_ok & bid_ok & ask_ok
}

#[export]
fn settle_batch(filled: [u64; 3], reserve: u64) -> i64 {
    if false {
        let dummy = Order { price: 0, size: 0 };
        let _ = match_orders(dummy, dummy, 0);
    }
    let mut total: u64 = 0;
    for size in filled {
        total += size;
    }

    if total > reserve {
        // Negative delta = need more inventory
        (reserve as i64) - (total as i64)
    } else {
        ((reserve - total) as i64)
    }
}

fn touch_dark_pool_exports() {
    if false {
        let dummy = Order { price: 0, size: 0 };
        let filled = [0, 0, 0];
        let _ = settle_batch(filled, 0);
        let _ = match_orders(dummy, dummy, 0);
    }
}
