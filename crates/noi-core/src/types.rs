#[cfg(feature = "runner")]
pub use noir_runner::FieldElement;

#[cfg(all(not(feature = "runner"), feature = "field"))]
pub use acvm::FieldElement;

#[cfg(not(any(feature = "runner", feature = "field")))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FieldElement(pub String);

#[cfg(not(any(feature = "runner", feature = "field")))]
impl FieldElement {
    pub fn new<T: Into<String>>(value: T) -> Self {
        Self(value.into())
    }
}
