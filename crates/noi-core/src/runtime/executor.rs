//! Solving a multi-circuit ACIR program.
//!
//! A port of `nargo::ops::execute_program` (Noir 1.0; `nargo` is not published on crates.io)
//! without its profiling and fuzzing hooks. The entry circuit is function 0; `Call` opcodes run
//! other circuits recursively, whose witnesses are pushed onto the witness stack before the entry
//! circuit's, which is pushed last at index 0 (the layout `bb prove` expects).

use std::collections::BTreeMap;

use acvm::{
    acir::{
        brillig::{ForeignCallParam, ForeignCallResult},
        circuit::Program as AcirProgram,
        native_types::{WitnessMap, WitnessStack},
    },
    pwg::{ACVMStatus, OpcodeResolutionError, ResolvedAssertionPayload, ACVM},
    AcirField, FieldElement,
};
use bn254_blackbox_solver::Bn254BlackBoxSolver;

use super::{abi, Client, OracleValue, Value};
use crate::export::{ErrorType, TypeRepr};

/// Why execution stopped, before it is attached to a function name.
pub(crate) struct Failure {
    pub(crate) message: Option<String>,
    pub(crate) detail: String,
}

pub(crate) fn execute(
    program: &AcirProgram<FieldElement>,
    initial_witness: WitnessMap<FieldElement>,
    client: &Client,
    error_types: &BTreeMap<u64, ErrorType>,
) -> Result<WitnessStack<FieldElement>, Failure> {
    let mut executor = Executor {
        program,
        client,
        error_types,
        witness_stack: WitnessStack::default(),
    };
    let main = executor.execute_circuit(0, initial_witness)?;
    executor.witness_stack.push(0, main);
    Ok(executor.witness_stack)
}

struct Executor<'a> {
    program: &'a AcirProgram<FieldElement>,
    client: &'a Client,
    error_types: &'a BTreeMap<u64, ErrorType>,
    witness_stack: WitnessStack<FieldElement>,
}

impl Executor<'_> {
    fn execute_circuit(
        &mut self,
        index: usize,
        initial_witness: WitnessMap<FieldElement>,
    ) -> Result<WitnessMap<FieldElement>, Failure> {
        let circuit = self.program.functions.get(index).ok_or_else(|| Failure {
            message: None,
            detail: format!("the program has no circuit {index}"),
        })?;
        let mut acvm = ACVM::new(
            &Bn254BlackBoxSolver,
            &circuit.opcodes,
            initial_witness,
            &self.program.unconstrained_functions,
            &circuit.assert_messages,
        );

        loop {
            match acvm.solve() {
                ACVMStatus::Solved => break,
                ACVMStatus::InProgress => unreachable!("ACVM::solve never returns InProgress"),
                ACVMStatus::Failure(error) => return Err(self.failure(error)),
                ACVMStatus::RequiresForeignCall(call) => {
                    let inputs: Vec<OracleValue> =
                        call.inputs.iter().map(OracleValue::from_param).collect();
                    let outputs =
                        self.client
                            .call_oracle(&call.function, &inputs)
                            .map_err(|err| Failure {
                                message: None,
                                detail: format!("oracle `{}` failed: {err:#}", call.function),
                            })?;
                    acvm.resolve_pending_foreign_call(ForeignCallResult {
                        values: outputs.into_iter().map(OracleValue::into_param).collect(),
                    });
                }
                ACVMStatus::RequiresAcirCall(call) => {
                    let callee = call.id.as_usize();
                    let solved = self.execute_circuit(callee, call.initial_witness)?;
                    let returns = &self.program.functions[callee].return_values;
                    let outputs = returns
                        .indices()
                        .into_iter()
                        .map(|i| {
                            solved.get_index(i).copied().ok_or_else(|| Failure {
                                message: None,
                                detail: format!(
                                    "circuit {callee} did not assign return witness {i}"
                                ),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    acvm.resolve_pending_acir_call(outputs);
                    self.witness_stack.push(call.id.as_u32(), solved);
                }
            }
        }
        Ok(acvm.finalize())
    }

    fn failure(&self, error: OpcodeResolutionError<FieldElement>) -> Failure {
        let payload = match &error {
            OpcodeResolutionError::UnsatisfiedConstrain { payload, .. }
            | OpcodeResolutionError::BrilligFunctionFailed { payload, .. } => payload.as_ref(),
            _ => None,
        };
        Failure {
            message: payload.map(|p| resolve_payload(p, self.error_types)),
            detail: error.to_string(),
        }
    }
}

/// Turns an assertion payload into a human-readable message, like `nargo` does.
fn resolve_payload(
    payload: &ResolvedAssertionPayload<FieldElement>,
    error_types: &BTreeMap<u64, ErrorType>,
) -> String {
    let raw = match payload {
        ResolvedAssertionPayload::String(message) => return message.clone(),
        ResolvedAssertionPayload::Raw(raw) => raw,
    };
    let Some(ty) = error_types.get(&raw.selector.as_u64()) else {
        return format!(
            "assertion failed with unknown error selector {}",
            raw.selector.as_u64()
        );
    };
    let mut fields = raw.data.iter().copied();
    match ty {
        ErrorType::String(message) => message.clone(),
        ErrorType::FmtString { length, items } => {
            let template = abi::decode_value(&mut fields, &TypeRepr::String(*length), "message")
                .ok()
                .and_then(|v| match v {
                    Value::String(s) => Some(s),
                    _ => None,
                })
                .unwrap_or_default();
            // The item count precedes the items.
            let _ = fields.next();
            let values: Vec<String> = items
                .iter()
                .map(|ty| match abi::decode_value(&mut fields, ty, "item") {
                    Ok(value) => display_value(&value, ty),
                    Err(_) => "<?>".to_owned(),
                })
                .collect();
            fill_placeholders(&template, &values)
        }
        ErrorType::Custom(ty) => match abi::decode_value(&mut fields, ty, "error") {
            Ok(value) => display_value(&value, ty),
            Err(_) => format!("assertion failed with a {ty} payload"),
        },
    }
}

/// Replaces each `{...}` placeholder of a Noir format string with the next value, in order.
fn fill_placeholders(template: &str, values: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut values = values.iter();
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            // Skip the placeholder name; Noir substitutes items positionally.
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
            }
            out.push_str(values.next().map_or("{}", String::as_str));
        } else {
            out.push(c);
        }
    }
    out
}

/// Formats a decoded value roughly the way Noir's `println` does.
pub(crate) fn display_value(value: &Value, ty: &TypeRepr) -> String {
    match (value, ty) {
        (Value::Field(f), TypeRepr::Bool) => (!f.is_zero()).to_string(),
        (Value::Field(f), TypeRepr::Unsigned(_)) => f
            .try_into_u128()
            .map_or_else(|| f.to_string(), |v| v.to_string()),
        (Value::Field(f), TypeRepr::Signed(width)) => match f.try_into_u128() {
            Some(raw) if *width < 128 && raw >> (width - 1) == 1 => {
                // Two's complement at `width` bits.
                (raw as i128 - (1i128 << width)).to_string()
            }
            Some(raw) => raw.to_string(),
            None => f.to_string(),
        },
        (Value::Field(f), _) => format!("0x{}", f.to_hex()),
        (Value::String(s), _) => s.clone(),
        (Value::Array(items), TypeRepr::Array(inner, _)) => format!(
            "[{}]",
            items
                .iter()
                .map(|v| display_value(v, inner))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Value::Array(items), TypeRepr::Tuple(types)) => format!(
            "({})",
            items
                .iter()
                .zip(types)
                .map(|(v, t)| display_value(v, t))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Value::Struct(map), TypeRepr::Struct(st)) => format!(
            "{} {{ {} }}",
            st.short_name(),
            st.fields
                .iter()
                .filter_map(|f| map.get(&f.name).map(|v| format!(
                    "{}: {}",
                    f.name,
                    display_value(v, &f.ty)
                )))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (value, _) => format!("{value:?}"),
    }
}

impl OracleValue {
    fn from_param(param: &ForeignCallParam<FieldElement>) -> Self {
        match param {
            ForeignCallParam::Single(f) => OracleValue::Field(*f),
            ForeignCallParam::Array(fs) => OracleValue::Array(fs.clone()),
        }
    }

    fn into_param(self) -> ForeignCallParam<FieldElement> {
        match self {
            OracleValue::Field(f) => ForeignCallParam::Single(f),
            OracleValue::Array(fs) => ForeignCallParam::Array(fs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_in_order() {
        let out = fill_placeholders("x={x}, y={y}, z={z}", &["1".into(), "2".into()]);
        assert_eq!(out, "x=1, y=2, z={}");
    }

    #[test]
    fn signed_values_display_negative() {
        let v = Value::Field(FieldElement::from(u128::from(u8::MAX)));
        assert_eq!(display_value(&v, &TypeRepr::Signed(8)), "-1");
        assert_eq!(display_value(&v, &TypeRepr::Unsigned(8)), "255");
    }
}
