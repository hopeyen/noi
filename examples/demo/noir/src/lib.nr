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
