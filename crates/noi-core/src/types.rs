#[cfg(feature = "field")]
pub use acvm::FieldElement;

#[cfg(not(feature = "field"))]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldElement(pub String);

#[cfg(not(feature = "field"))]
impl FieldElement {
    pub fn new<T: Into<String>>(value: T) -> Self {
        Self(value.into())
    }
}

#[cfg(not(feature = "field"))]
impl Default for FieldElement {
    fn default() -> Self {
        Self(String::new())
    }
}
