#![cfg(feature = "runner")]

use std::collections::BTreeMap;
use std::path::Path;

use acvm::AcirField;
use anyhow::{anyhow, Result};
use noir_runner::{FieldElement, NoirRunner};

pub use noir_runner::InputValue;

pub type InputMap = BTreeMap<String, InputValue>;

pub fn run_function(
    program_dir: &Path,
    fn_name: &str,
    inputs: InputMap,
) -> Result<Option<InputValue>> {
    let runner = NoirRunner::try_new(program_dir.to_path_buf())
        .map_err(|err| anyhow!("failed to initialize Noir runner: {err:?}"))?;
    runner
        .run(fn_name, inputs)
        .map_err(|err| anyhow!("Noir execution failed: {err:?}"))
}

pub trait ToInputValue {
    fn to_input_value(&self) -> InputValue;
}

pub trait FromOutputValue: Sized {
    fn from_output_value(value: InputValue) -> Result<Self>;
}

impl ToInputValue for bool {
    fn to_input_value(&self) -> InputValue {
        InputValue::Field(if *self {
            FieldElement::one()
        } else {
            FieldElement::zero()
        })
    }
}

impl FromOutputValue for bool {
    fn from_output_value(value: InputValue) -> Result<Self> {
        match value {
            InputValue::Field(field) => {
                if field.is_zero() {
                    Ok(false)
                } else if field.is_one() {
                    Ok(true)
                } else {
                    Err(anyhow!("expected boolean field element"))
                }
            }
            other => Err(anyhow!("expected field element for bool, got {other:?}")),
        }
    }
}

macro_rules! impl_unsigned {
    ($($ty:ty),+) => {
        $(
        impl ToInputValue for $ty {
            fn to_input_value(&self) -> InputValue {
                InputValue::Field(FieldElement::from(*self as u128))
            }
        }

        impl FromOutputValue for $ty {
            fn from_output_value(value: InputValue) -> Result<Self> {
                match value {
                    InputValue::Field(field) => field
                        .try_into_u128()
                        .and_then(|val| <Self as TryFrom<u128>>::try_from(val).ok())
                        .ok_or_else(|| anyhow!("value does not fit into {}", stringify!($ty))),
                    other => Err(anyhow!(
                        "expected field element for {}, got {other:?}",
                        stringify!($ty)
                    )),
                }
            }
        }
        )+
    };
}

macro_rules! impl_signed {
    ($($ty:ty),+) => {
        $(
        impl ToInputValue for $ty {
            fn to_input_value(&self) -> InputValue {
                InputValue::Field(FieldElement::from(*self as i128))
            }
        }

        impl FromOutputValue for $ty {
            fn from_output_value(value: InputValue) -> Result<Self> {
                match value {
                    InputValue::Field(field) => {
                        let raw = field.to_i128();
                        if raw < Self::MIN as i128 || raw > Self::MAX as i128 {
                            Err(anyhow!("value does not fit into {}", stringify!($ty)))
                        } else {
                            Ok(raw as Self)
                        }
                    }
                    other => Err(anyhow!(
                        "expected field element for {}, got {other:?}",
                        stringify!($ty)
                    )),
                }
            }
        }
        )+
    };
}

impl_unsigned!(u8, u16, u32, u64, u128, usize);
impl_signed!(i8, i16, i32, i64, i128, isize);

impl ToInputValue for FieldElement {
    fn to_input_value(&self) -> InputValue {
        InputValue::Field(*self)
    }
}

impl FromOutputValue for FieldElement {
    fn from_output_value(value: InputValue) -> Result<Self> {
        match value {
            InputValue::Field(field) => Ok(field),
            other => Err(anyhow!("expected field element, got {other:?}")),
        }
    }
}

impl<T, const N: usize> ToInputValue for [T; N]
where
    T: ToInputValue,
{
    fn to_input_value(&self) -> InputValue {
        InputValue::Vec(self.iter().map(|item| item.to_input_value()).collect())
    }
}

impl<T, const N: usize> FromOutputValue for [T; N]
where
    T: FromOutputValue,
{
    fn from_output_value(value: InputValue) -> Result<Self> {
        match value {
            InputValue::Vec(values) => {
                if values.len() != N {
                    return Err(anyhow!(
                        "expected array of length {N}, got {}",
                        values.len()
                    ));
                }
                let converted: Vec<T> = values
                    .into_iter()
                    .map(FromOutputValue::from_output_value)
                    .collect::<Result<_>>()?;
                converted
                    .try_into()
                    .map_err(|_| anyhow!("array conversion failed"))
            }
            other => Err(anyhow!("expected array input, got {other:?}")),
        }
    }
}

macro_rules! count_idents {
    ($($name:ident),+) => {
        <[()]>::len(&[ $(count_idents!(@single $name)),+ ])
    };
    (@single $name:ident) => { () };
}

macro_rules! impl_tuple {
    ($($name:ident),+) => {
        #[allow(non_camel_case_types, non_snake_case)]
        impl<$($name),+> ToInputValue for ($($name,)+)
        where
            $( $name: ToInputValue ),+
        {
            fn to_input_value(&self) -> InputValue {
                let ($($name,)+) = self;
                InputValue::Vec(vec![ $( $name.to_input_value() ),+ ])
            }
        }

        #[allow(non_camel_case_types, non_snake_case)]
        impl<$($name),+> FromOutputValue for ($($name,)+)
        where
            $( $name: FromOutputValue ),+
        {
            fn from_output_value(value: InputValue) -> Result<Self> {
                if let InputValue::Vec(values) = value {
                    if values.len() != count_idents!($($name),+) {
                        return Err(anyhow!(
                            "expected tuple of length {}, got {}",
                            count_idents!($($name),+),
                            values.len()
                        ));
                    }
                    let mut iter = values.into_iter();
                    Ok((
                        $( $name::from_output_value(iter.next().unwrap())? ,)+
                    ))
                } else {
                    Err(anyhow!("expected tuple encoded as array"))
                }
            }
        }
    };
}

impl_tuple!(A, B);
impl_tuple!(A, B, C);
impl_tuple!(A, B, C, D);
impl_tuple!(A, B, C, D, E);
impl_tuple!(A, B, C, D, E, F);

impl ToInputValue for () {
    fn to_input_value(&self) -> InputValue {
        InputValue::Vec(Vec::new())
    }
}

impl FromOutputValue for () {
    fn from_output_value(_: InputValue) -> Result<Self> {
        Ok(())
    }
}
