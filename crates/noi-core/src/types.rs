//! Value types shared by generated bindings.

/// An element of the BN254 scalar field, the type of Noir's `Field`.
///
/// With the `runner` or `field` feature this is `acvm::FieldElement`, which supports arithmetic
/// and conversions (`FieldElement::from(42u128)`, `FieldElement::try_from_str("0x2a")`).
/// Without either feature it is a lightweight placeholder holding the decimal or hex string, so
/// generated types still compile in schema-only builds.
#[cfg(any(feature = "runner", feature = "field"))]
pub use acvm::FieldElement;

/// Placeholder for Noir's `Field` when neither `runner` nor `field` is enabled.
///
/// Holds the value's textual representation (decimal or `0x`-prefixed hex). Enable the `field`
/// feature for a real field element.
#[cfg(not(any(feature = "runner", feature = "field")))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FieldElement(pub String);

#[cfg(not(any(feature = "runner", feature = "field")))]
impl FieldElement {
    /// Wraps a textual field value, e.g. `FieldElement::new("0x2a")`.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
