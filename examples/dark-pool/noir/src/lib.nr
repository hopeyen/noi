struct Order {
    price: u64,
    size: u64,
}

#[export]
fn match_orders(bid: Order, ask: Order, min_size: u64) -> bool {
    bid.price >= ask.price && bid.size >= min_size && ask.size >= min_size
}

#[export]
fn settle_batch(filled: [u64; 3], reserve: u64) -> i64 {
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
