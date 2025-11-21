struct Point {
    x: u32,
    y: u32,
}

#[export]
fn add(a: u32, b: u32) -> u32 {
    a + b
}

#[export]
fn mix(point: Point, toggles: (bool, [u16; 3])) -> Point {
    let (flag, deltas) = toggles;
    let delta = if flag { deltas[0] as u32 } else { deltas[1] as u32 };
    Point { x: point.x + delta, y: point.y + deltas[2] as u32 }
}
