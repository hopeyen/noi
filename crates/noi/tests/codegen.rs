//! Compile-time checks for codegen edge cases. The fixture has an ABI but no bytecode, so these
//! tests only construct values; they never execute.

noi::nrg!(module = edge, export_dir = "tests/fixtures/codegen");

#[test]
fn keywords_collisions_and_large_arrays() {
    // `move` is a keyword (raw ident); `type`/`self` are escaped; `callData` becomes snake_case.
    let args = edge::r#move::Args {
        r#type: edge::OrdersOrder {
            self_: true,
            fills: std::array::from_fn(|_| noi::FieldElement::default()),
        },
        other: edge::BookOrder {
            memo: "8 bytes!".into(),
        },
        call_data: -7,
    };
    // Databus parameters are grouped with public inputs.
    let inputs = edge::r#move::Inputs::from(args);
    assert_eq!(inputs.public.call_data, -7);
    assert_eq!(inputs.private.other.memo, "8 bytes!");
    let _: edge::r#move::Output = ();
}
