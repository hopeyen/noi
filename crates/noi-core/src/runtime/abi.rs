//! ABI encoding: [`Value`]s ⇄ witness field elements.
//!
//! Mirrors `noirc_abi::Abi::{encode, decode}` from Noir 1.0 (which is not published on crates.io):
//! parameters are flattened depth-first in declaration order into witnesses `0..n`, and the return
//! value occupies the witnesses that immediately follow. Every leaf is type-checked so that invalid
//! inputs are rejected before execution with a path such as `p.x` or `arr[2]`.

use std::collections::BTreeMap;

use acvm::{
    acir::native_types::{Witness, WitnessMap},
    AcirField, FieldElement,
};
use anyhow::{anyhow, bail, ensure, Result};

use super::{InputMap, Value};
use crate::export::{ExportFunction, TypeRepr};

/// Number of field elements `ty` occupies once encoded (`AbiType::field_count`).
///
/// Artifacts are validated on load (`export::checked_field_count`), so this cannot overflow for
/// types that came from a `Program`; saturating arithmetic is a second guard.
pub(crate) fn field_count(ty: &TypeRepr) -> usize {
    match ty {
        TypeRepr::Bool | TypeRepr::Field | TypeRepr::Unsigned(_) | TypeRepr::Signed(_) => 1,
        TypeRepr::String(len) => *len,
        TypeRepr::Array(inner, len) => field_count(inner).saturating_mul(*len),
        TypeRepr::Tuple(items) => items.iter().map(field_count).fold(0, usize::saturating_add),
        TypeRepr::Struct(st) => st
            .fields
            .iter()
            .map(|f| field_count(&f.ty))
            .fold(0, usize::saturating_add),
    }
}

/// Encodes `inputs` (keyed by Noir parameter name) as the initial witness of `function`.
pub(crate) fn encode_inputs(
    function: &ExportFunction,
    inputs: &InputMap,
) -> Result<WitnessMap<FieldElement>> {
    if let Some(extra) = inputs
        .keys()
        .find(|name| !function.parameters.iter().any(|p| &p.name == *name))
    {
        bail!(
            "unexpected input `{extra}`; `{}` has no such parameter",
            function.name
        );
    }
    let mut fields = Vec::new();
    for param in &function.parameters {
        let value = inputs
            .get(&param.name)
            .ok_or_else(|| anyhow!("missing input `{}`", param.name))?;
        encode_value(value, &param.ty, &param.name, &mut fields)?;
    }
    let witnesses: BTreeMap<Witness, FieldElement> = fields
        .into_iter()
        .enumerate()
        .map(|(i, field)| (Witness(i as u32), field))
        .collect();
    Ok(witnesses.into())
}

/// Encodes one value, appending its field elements to `out`. `path` names the value in errors.
pub(crate) fn encode_value(
    value: &Value,
    ty: &TypeRepr,
    path: &str,
    out: &mut Vec<FieldElement>,
) -> Result<()> {
    match (value, ty) {
        (Value::Field(field), TypeRepr::Field) => out.push(*field),
        (Value::Field(field), TypeRepr::Bool) => {
            ensure!(
                field.is_zero() || field.is_one(),
                "`{path}`: {field} is not a valid bool (expected 0 or 1)"
            );
            out.push(*field);
        }
        (Value::Field(field), TypeRepr::Unsigned(width) | TypeRepr::Signed(width)) => {
            ensure!(
                field.num_bits() <= u32::from(*width),
                "`{path}`: {field} does not fit in {ty}"
            );
            out.push(*field);
        }
        (Value::String(string), TypeRepr::String(len)) => {
            ensure!(
                string.len() == *len,
                "`{path}`: expected a string of exactly {len} bytes, got {} bytes",
                string.len()
            );
            out.extend(string.bytes().map(|b| FieldElement::from(u128::from(b))));
        }
        (Value::Array(items), TypeRepr::Array(inner, len)) => {
            ensure!(
                items.len() == *len,
                "`{path}`: expected an array of length {len}, got {}",
                items.len()
            );
            for (i, item) in items.iter().enumerate() {
                encode_value(item, inner, &format!("{path}[{i}]"), out)?;
            }
        }
        (Value::Array(items), TypeRepr::Tuple(types)) => {
            ensure!(
                items.len() == types.len(),
                "`{path}`: expected a tuple of length {}, got {}",
                types.len(),
                items.len()
            );
            for (i, (item, ty)) in items.iter().zip(types).enumerate() {
                encode_value(item, ty, &format!("{path}.{i}"), out)?;
            }
        }
        (Value::Struct(map), TypeRepr::Struct(st)) => {
            if let Some(extra) = map
                .keys()
                .find(|k| !st.fields.iter().any(|f| &f.name == *k))
            {
                bail!("`{path}`: struct `{}` has no field `{extra}`", st.path);
            }
            for field in &st.fields {
                let item = map.get(&field.name).ok_or_else(|| {
                    anyhow!(
                        "`{path}`: missing field `{}` of struct `{}`",
                        field.name,
                        st.path
                    )
                })?;
                encode_value(item, &field.ty, &format!("{path}.{}", field.name), out)?;
            }
        }
        (value, ty) => bail!("`{path}`: expected {ty}, got {}", value.kind()),
    }
    Ok(())
}

/// Decodes the return value of `function` from the solved witness of its circuit.
pub(crate) fn decode_return(
    function: &ExportFunction,
    witness: &WitnessMap<FieldElement>,
) -> Result<Option<Value>> {
    let Some(ty) = &function.return_type else {
        return Ok(None);
    };
    let start: usize = function.parameters.iter().map(|p| field_count(&p.ty)).sum();
    let fields = (start..start + field_count(ty))
        .map(|i| {
            witness
                .get_index(i as u32)
                .copied()
                .ok_or_else(|| anyhow!("the solved witness has no value for return witness {i}"))
        })
        .collect::<Result<Vec<_>>>()?;
    decode_value(&mut fields.into_iter(), ty, "return").map(Some)
}

/// Decodes one value of type `ty` from `fields`, validating every leaf.
pub(crate) fn decode_value(
    fields: &mut impl Iterator<Item = FieldElement>,
    ty: &TypeRepr,
    path: &str,
) -> Result<Value> {
    let mut next = || {
        fields
            .next()
            .ok_or_else(|| anyhow!("`{path}`: ran out of field elements while decoding {ty}"))
    };
    Ok(match ty {
        TypeRepr::Field => Value::Field(next()?),
        TypeRepr::Bool => {
            let field = next()?;
            ensure!(
                field.is_zero() || field.is_one(),
                "`{path}`: {field} is not a bool"
            );
            Value::Field(field)
        }
        TypeRepr::Unsigned(width) | TypeRepr::Signed(width) => {
            let field = next()?;
            ensure!(
                field.num_bits() <= u32::from(*width),
                "`{path}`: {field} does not fit in {ty}"
            );
            Value::Field(field)
        }
        TypeRepr::String(len) => {
            let bytes = (0..*len)
                .map(|_| {
                    let field = next()?;
                    field
                        .try_into_u128()
                        .and_then(|b| u8::try_from(b).ok())
                        .ok_or_else(|| anyhow!("`{path}`: {field} is not a byte"))
                })
                .collect::<Result<Vec<u8>>>()?;
            // Noir strings may hold arbitrary bytes (`[u8; N].as_str_unchecked()`); decode them
            // lossily, like nargo, rather than failing a circuit that solved.
            Value::String(String::from_utf8_lossy(&bytes).into_owned())
        }
        TypeRepr::Array(inner, len) => Value::Array(
            (0..*len)
                .map(|i| decode_value(fields, inner, &format!("{path}[{i}]")))
                .collect::<Result<_>>()?,
        ),
        TypeRepr::Tuple(types) => Value::Array(
            types
                .iter()
                .enumerate()
                .map(|(i, ty)| decode_value(fields, ty, &format!("{path}.{i}")))
                .collect::<Result<_>>()?,
        ),
        TypeRepr::Struct(st) => Value::Struct(
            st.fields
                .iter()
                .map(|f| {
                    let value = decode_value(fields, &f.ty, &format!("{path}.{}", f.name))?;
                    Ok((f.name.clone(), value))
                })
                .collect::<Result<_>>()?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{StructField, StructType};

    fn point() -> TypeRepr {
        TypeRepr::Struct(StructType {
            path: "P".into(),
            fields: vec![
                StructField {
                    name: "x".into(),
                    ty: TypeRepr::Unsigned(8),
                },
                StructField {
                    name: "tag".into(),
                    ty: TypeRepr::String(2),
                },
            ],
        })
    }

    fn f(v: u128) -> FieldElement {
        FieldElement::from(v)
    }

    #[test]
    fn round_trips_nested_values() {
        let ty = TypeRepr::Tuple(vec![TypeRepr::Array(Box::new(point()), 2), TypeRepr::Bool]);
        let p = |x: u128, tag: &str| {
            Value::Struct(BTreeMap::from([
                ("x".to_string(), Value::Field(f(x))),
                ("tag".to_string(), Value::String(tag.into())),
            ]))
        };
        let value = Value::Array(vec![
            Value::Array(vec![p(1, "ab"), p(2, "cd")]),
            Value::Field(f(1)),
        ]);
        let mut fields = Vec::new();
        encode_value(&value, &ty, "v", &mut fields).unwrap();
        // Struct fields are laid out in declaration order: x, then the two string bytes.
        assert_eq!(fields, [f(1), f(97), f(98), f(2), f(99), f(100), f(1)]);
        assert_eq!(field_count(&ty), fields.len());
        assert_eq!(
            decode_value(&mut fields.into_iter(), &ty, "v").unwrap(),
            value
        );
    }

    #[test]
    fn rejects_out_of_range_and_shape_errors_with_paths() {
        let mut out = Vec::new();
        let err =
            encode_value(&Value::Field(f(256)), &TypeRepr::Unsigned(8), "a", &mut out).unwrap_err();
        assert!(err.to_string().contains("`a`"), "{err}");

        let bad = Value::Struct(BTreeMap::from([("x".to_string(), Value::Field(f(1)))]));
        let err = encode_value(&bad, &point(), "p", &mut out).unwrap_err();
        assert!(err.to_string().contains("missing field `tag`"), "{err}");

        let err = encode_value(&Value::Field(f(2)), &TypeRepr::Bool, "b", &mut out).unwrap_err();
        assert!(err.to_string().contains("not a valid bool"), "{err}");

        let err = decode_value(&mut [f(300)].into_iter(), &TypeRepr::Unsigned(8), "r").unwrap_err();
        assert!(err.to_string().contains("does not fit"), "{err}");
    }

    #[test]
    fn non_utf8_strings_decode_lossily() {
        let out =
            decode_value(&mut [f(97), f(0xfe)].into_iter(), &TypeRepr::String(2), "s").unwrap();
        assert_eq!(out, Value::String("a\u{FFFD}".into()));
    }
}
