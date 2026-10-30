#[export]
fn add(a: u32, b: u32) -> u32 {
    touch_exports();
    a + b
}

#[export]
fn mix(t: (u32, bool), arr: [u16; 3]) -> (u32, bool) {
    let (value, flag) = t;
    let delta = if flag { arr[0] as u32 } else { arr[1] as u32 };
    let sum = add(value, delta);
    (sum, flag)
}

struct P {
    x: u32,
    y: u32,
}

#[export]
fn move_point(p: P, dx: i32) -> P {
    let delta = dx as i64;
    let next = (p.x as i64) + delta;
    let clamped = if next < 0 { 0 } else { next as u64 };
    P { x: clamped as u32, y: p.y }
}

fn touch_exports() {
    if false {
        let zero_u32 = 0 as u32;
        let zero_u16 = 0 as u16;
        let dummy = [zero_u16, zero_u16, zero_u16];
        let _ = mix((zero_u32, false), dummy);
        let _ = move_point(P { x: zero_u32, y: zero_u32 }, 0);
    }
}

#[export]
fn checked_div(a: u32, b: u32) -> u32 {
    assert(b != 0, "division by zero");
    a / b
}

#[export]
fn greet(name: str<5>) -> str<5> {
    name
}

/// Proves `secret > threshold` without revealing `secret`.
///
/// Noir only allows `pub` on a program's `main`; for `#[export]` functions the ACIR return values
/// are what a proof makes public. Returning `threshold` therefore commits the proof to it.
#[export]
fn above_threshold(threshold: u32, secret: u32) -> (u32, bool) {
    (threshold, secret > threshold)
}

/// Compiled as its own ACIR circuit (`#[fold]`), so callers exercise ACIR-to-ACIR calls.
#[fold]
fn square(x: Field) -> Field {
    x * x
}

#[export]
fn sum_of_squares(a: Field, b: Field) -> Field {
    square(a) + square(b)
}

#[oracle(get_price)]
unconstrained fn get_price_oracle(item: Field) -> Field {}

unconstrained fn get_price(item: Field) -> Field {
    get_price_oracle(item)
}

/// Asks the host for a price through an oracle, then checks the order size.
#[export]
fn order_total(item: Field, qty: u32) -> Field {
    assert(qty < 1000, f"qty {qty} too large");
    println(qty);
    // Safety: the price is supplied by the host and is not constrained.
    let price = unsafe { get_price(item) };
    price * (qty as Field)
}
