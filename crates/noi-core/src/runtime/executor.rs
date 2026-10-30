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
            // Like nargo's `execution_error_from`: out-of-bounds accesses into flattened arrays of
            // structs/tuples carry a payload stating the error in the program's own coordinates.
            OpcodeResolutionError::UnsatisfiedConstrain { payload, .. }
            | OpcodeResolutionError::IndexOutOfBounds { payload, .. }
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

/// Renders a Noir format string, replacing each `{...}` placeholder with the next value in order.
///
/// A port of `noirc_printable_type::write_template_replacing_interpolations`: `{{` and `}}` are
/// escapes for literal braces and consume no value; the name inside a placeholder is ignored.
fn fill_placeholders(template: &str, values: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut values = values.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '}' => {
                // Outside a placeholder, `}` only appears escaped as `}}`.
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                out.push('}');
            }
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '{' => {
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
                out.push_str(values.next().map_or("{}", String::as_str));
            }
            c => out.push(c),
        }
    }
    out
}

/// Formats a decoded value exactly as Noir's `println` and nargo's error messages do
/// (`noirc_printable_type::to_string`).
pub(crate) fn display_value(value: &Value, ty: &TypeRepr) -> String {
    let to_u128 = |f: &FieldElement| f.to_u128();
    match (value, ty) {
        (Value::Field(f), TypeRepr::Bool) => f.is_one().to_string(),
        (Value::Field(f), TypeRepr::Unsigned(width)) => {
            let mut v = to_u128(f);
            if *width < 128 {
                v &= (1u128 << width) - 1;
            }
            v.to_string()
        }
        (Value::Field(f), TypeRepr::Signed(width)) => {
            let width = u32::from(*width).clamp(1, 128);
            let mut v = to_u128(f);
            if width < 128 {
                v &= (1u128 << width) - 1;
            }
            if v >> (width - 1) == 1 {
                // Two's complement at `width` bits.
                let magnitude = if width == 128 {
                    v.wrapping_neg()
                } else {
                    (v ^ ((1u128 << width) - 1)) + 1
                };
                format!("-{magnitude}")
            } else {
                v.to_string()
            }
        }
        (Value::Field(f), _) => f.to_short_hex(),
        (Value::String(s), _) => s.clone(),
        (Value::Array(items), TypeRepr::Array(inner, _)) => format!(
            "[{}]",
            items
                .iter()
                .map(|v| display_value(v, inner))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Value::Array(items), TypeRepr::Tuple(types)) => {
            let inner = items
                .iter()
                .zip(types)
                .map(|(v, t)| display_value(v, t))
                .collect::<Vec<_>>()
                .join(", ");
            if types.len() == 1 {
                format!("({inner},)")
            } else {
                format!("({inner})")
            }
        }
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
    use crate::export::{StructField, StructType};

    fn f(v: u128) -> Value {
        Value::Field(FieldElement::from(v))
    }

    #[test]
    fn placeholders_follow_noir_escaping() {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            fill_placeholders("x={x}, y={y}, z={z}", &v(&["1", "2"])),
            "x=1, y=2, z={}"
        );
        // Expected strings are what nargo 1.0.0 prints for the same assertions.
        assert_eq!(
            fill_placeholders("esc {{literal}} then {a}", &v(&["200"])),
            "esc {literal} then 200"
        );
        assert_eq!(
            fill_placeholders("n={n} and {{}}", &v(&["3"])),
            "n=3 and {}"
        );
        assert_eq!(fill_placeholders("x}}y {s}", &v(&["-7"])), "x}y -7");
    }

    #[test]
    fn values_display_like_nargo() {
        let max_u8 = f(u128::from(u8::MAX));
        assert_eq!(display_value(&max_u8, &TypeRepr::Signed(8)), "-1");
        assert_eq!(display_value(&max_u8, &TypeRepr::Unsigned(8)), "255");
        assert_eq!(display_value(&f(0x80), &TypeRepr::Signed(8)), "-128");
        assert_eq!(display_value(&f(5), &TypeRepr::Field), "0x05");
        assert_eq!(display_value(&f(0), &TypeRepr::Field), "0x00");
        assert_eq!(display_value(&f(1), &TypeRepr::Bool), "true");
        let one = TypeRepr::Tuple(vec![TypeRepr::Unsigned(8)]);
        assert_eq!(display_value(&Value::Array(vec![f(9)]), &one), "(9,)");
        let st = TypeRepr::Struct(StructType {
            path: "m::P".into(),
            fields: vec![
                StructField {
                    name: "x".into(),
                    ty: TypeRepr::Field,
                },
                StructField {
                    name: "y".into(),
                    ty: TypeRepr::Signed(8),
                },
            ],
        });
        let p = Value::Struct(BTreeMap::from([
            ("x".to_string(), f(0x1234)),
            ("y".to_string(), f(0x80)),
        ]));
        assert_eq!(display_value(&p, &st), "P { x: 0x1234, y: -128 }");
        // Malformed widths from an artifact must not panic.
        assert_eq!(display_value(&f(1), &TypeRepr::Signed(0)), "-1");
        assert_eq!(display_value(&max_u8, &TypeRepr::Unsigned(300)), "255");
    }
}
