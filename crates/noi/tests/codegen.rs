//! Compile-time checks for codegen edge cases. The fixtures have an ABI but no bytecode, so these
//! tests only construct and convert values; they never execute.

noi::nrg!(module = edge, export_dir = "tests/fixtures/codegen");

#[test]
fn keywords_collisions_and_large_arrays() {
    // `move` is a keyword (raw ident); `type`/`self` are escaped; `callData` becomes snake_case.
    // Structs from different paths sharing a short name get path-based names.
    let args = edge::r#move::Args {
        r#type: edge::OrdersOrder {
            self_: true,
            fills: std::array::from_fn(|_| noi::FieldElement::default()),
        },
        // Field names that match generated locals (`fields`, `value`) must not break codegen.
        other: edge::BookOrder {
            fields: "8 bytes!".into(),
            value: true,
        },
        call_data: -7,
    };
    let _: edge::r#move::Output = ();
    assert_eq!(args.call_data, -7);
}

#[test]
fn generics_and_reserved_names() {
    // Leading underscores are dropped by snake_case (`_is_some` -> `is_some`).
    // Each instantiation of a generic struct is its own type; prelude and `Client` names are
    // prefixed so they cannot shadow `Option` or the module's `Client`.
    let a = edge::NoirOption {
        is_some: true,
        value: 1u8,
    };
    let b = edge::NoirOption2 {
        is_some: false,
        value: 7u32,
    };
    let c = edge::NoirClient {
        id: noi::FieldElement::default(),
    };
    let args = edge::generics::Args {
        a: a.clone(),
        b,
        c,
        wide: (
            false, false, false, false, false, false, false, false, false, false, false, true,
        ),
    };
    let _: edge::generics::Output = a;
    assert!(args.a.is_some);
}

#[cfg(feature = "runner")]
#[test]
fn struct_conversions_round_trip() {
    use noi::{FromValue, ToValue};
    let order = edge::BookOrder {
        fields: "8 bytes!".into(),
        value: true,
    };
    assert_eq!(
        edge::BookOrder::from_value(order.to_value()).unwrap(),
        order
    );
}
