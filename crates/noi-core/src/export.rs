//! Typed view of the JSON artifacts written by `nargo export`.
//!
//! `nargo export` compiles every function annotated with `#[export]` in a Noir library and writes
//! one JSON program artifact per function into `<workspace>/export/<function>.json` (for a
//! standalone package, `<package>/export/`)
//! (see <https://noir-lang.org/docs/reference/nargo_commands#nargo-export>). Each artifact carries
//! the compiled ACIR bytecode plus an ABI describing the function's parameters and return type.
//!
//! This module decodes only the ABI half of those artifacts into small, codegen-friendly types
//! ([`ExportFunction`], [`Param`], [`TypeRepr`]). It has no dependency on the Noir compiler crates,
//! so it is usable from proc macros and build scripts. Executing the bytecode is the job of
//! [`crate::runtime`].
//!
//! # Example
//!
//! ```
//! use noi_core::export::{load_export_dir, TypeRepr};
//!
//! # let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/demo");
//! let functions = load_export_dir(dir)?;
//! let add = functions.iter().find(|f| f.name == "add").unwrap();
//! assert_eq!(add.signature(), "fn add(a: u32, b: u32) -> u32");
//! assert_eq!(add.public_parameters().count(), 0); // Noir 1.0 exports have no `pub` parameters
//! assert_eq!(add.return_type, Some(TypeRepr::Unsigned(32)));
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use fs_err as fs;
use serde::Deserialize;

/// ABI metadata for a single Noir `#[export]` function.
///
/// Produced by [`load_export_dir`] / [`load_export_file`]. The `nrg!` macro turns each value into a
/// Rust module with `NAME`, `SIGNATURE`, typed `Args` and `Output`, `program`/`execute`/`simulate`
/// (feature `runner`), and `prove`/`verify` helpers (feature `prove`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExportFunction {
    /// Function name. `nargo export` names each artifact after its function, so this is the file
    /// stem of [`source_path`](Self::source_path).
    pub name: String,
    /// Parameters in declaration order (the order Noir lays them out in the witness).
    pub parameters: Vec<Param>,
    /// Return type, or `None` for functions returning `()`.
    pub return_type: Option<TypeRepr>,
    /// Visibility of the return value as recorded in the ABI. Exported functions report
    /// [`Private`](Visibility::Private), but `bb` still treats the return value of a circuit as its
    /// public inputs.
    pub return_visibility: Visibility,
    /// Compiler version recorded in the artifact (e.g. `1.0.0+275d3ab1…`), when present.
    pub noir_version: Option<String>,
    /// Path of the JSON artifact this function was loaded from.
    pub source_path: PathBuf,
}

impl ExportFunction {
    /// Renders a Noir-style signature, e.g. `fn add(a: u32, b: u32) -> u32` (`pub` appears only
    /// for `main` program artifacts).
    ///
    /// Used for generated doc comments and diagnostics.
    pub fn signature(&self) -> String {
        let params = self
            .parameters
            .iter()
            .map(|param| format!("{}{}: {}", param.visibility.prefix(), param.name, param.ty))
            .collect::<Vec<_>>()
            .join(", ");
        match &self.return_type {
            Some(ret) => format!(
                "fn {}({}) -> {}{}",
                self.name,
                params,
                self.return_visibility.prefix(),
                ret
            ),
            None => format!("fn {}({})", self.name, params),
        }
    }

    /// Iterates over parameters marked `pub` or `call_data`. Always empty for `#[export]`
    /// functions in Noir 1.0; present for `main` program artifacts.
    pub fn public_parameters(&self) -> impl Iterator<Item = &Param> {
        self.parameters.iter().filter(|p| p.visibility.is_public())
    }
}

/// Visibility of a parameter or return value in the Noir ABI.
///
/// Mirrors `noirc_abi::AbiVisibility`. Noir 1.0 only accepts `pub` and `call_data` on a program's
/// `main`, so the parameters of `#[export]` functions are always [`Private`](Self::Private) and a
/// proof's public inputs are the return value. The other variants appear in artifacts of `main`
/// programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Declared with `pub`; part of the proof's public inputs.
    Public,
    /// The default; known only to the prover.
    #[default]
    Private,
    /// Passed through the databus (`call_data(n)` / `return_data`), used by recursive and
    /// folding-based proving schemes.
    #[serde(rename = "databus")]
    DataBus,
}

impl Visibility {
    /// Returns `true` for [`Public`](Self::Public) and [`DataBus`](Self::DataBus).
    pub fn is_public(self) -> bool {
        !matches!(self, Visibility::Private)
    }

    fn prefix(self) -> &'static str {
        match self {
            Visibility::Public => "pub ",
            Visibility::Private => "",
            Visibility::DataBus => "call_data ",
        }
    }
}

/// A single function parameter from the exported ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Param {
    /// Parameter name as written in Noir. Also the key used in the input map at execution time.
    pub name: String,
    /// Parameter type.
    pub ty: TypeRepr,
    /// Whether the parameter is public, private, or on the databus.
    pub visibility: Visibility,
}

/// A Noir ABI type.
///
/// Mirrors `noirc_abi::AbiType`. The `Display` impl renders Noir syntax (`[u16; 3]`, `(u32, bool)`,
/// `str<8>`, …).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TypeRepr {
    /// `bool`.
    Bool,
    /// `Field`, an element of the proving backend's scalar field (BN254 for Barretenberg).
    Field,
    /// `u8`, `u16`, `u32`, `u64`, or `u128`. The payload is the bit width. (Width 1, the `u1` of
    /// pre-1.0 Noir, is still accepted and maps to `bool`.)
    Unsigned(u16),
    /// `i8`, `i16`, `i32`, or `i64`. The payload is the bit width.
    Signed(u16),
    /// Fixed-size array `[T; N]`.
    Array(Box<TypeRepr>, usize),
    /// Fixed-length string `str<N>`. The payload is the length in bytes.
    String(usize),
    /// Tuple `(A, B, …)`.
    Tuple(Vec<TypeRepr>),
    /// User-defined struct.
    Struct(StructType),
}

impl TypeRepr {
    /// Returns `true` if this is a [`TypeRepr::Struct`].
    pub fn is_struct(&self) -> bool {
        matches!(self, TypeRepr::Struct(_))
    }

    /// Calls `visit` for every struct reachable from this type, outermost first.
    pub fn visit_structs<'a>(&'a self, visit: &mut impl FnMut(&'a StructType)) {
        match self {
            TypeRepr::Array(inner, _) => inner.visit_structs(visit),
            TypeRepr::Tuple(items) => items.iter().for_each(|item| item.visit_structs(visit)),
            TypeRepr::Struct(st) => {
                visit(st);
                st.fields.iter().for_each(|f| f.ty.visit_structs(visit));
            }
            TypeRepr::Bool
            | TypeRepr::Field
            | TypeRepr::Unsigned(_)
            | TypeRepr::Signed(_)
            | TypeRepr::String(_) => {}
        }
    }
}

impl fmt::Display for TypeRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRepr::Bool => f.write_str("bool"),
            TypeRepr::Field => f.write_str("Field"),
            TypeRepr::Unsigned(bits) => write!(f, "u{bits}"),
            TypeRepr::Signed(bits) => write!(f, "i{bits}"),
            TypeRepr::Array(elem, len) => write!(f, "[{elem}; {len}]"),
            TypeRepr::String(len) => write!(f, "str<{len}>"),
            TypeRepr::Tuple(values) => {
                f.write_str("(")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str(")")
            }
            TypeRepr::Struct(struct_ty) => struct_ty.fmt(f),
        }
    }
}

/// A Noir struct as described by the ABI.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct StructType {
    /// Fully qualified Noir path, e.g. `demo::P` or `orders::Order`.
    pub path: String,
    /// Fields in declaration order.
    pub fields: Vec<StructField>,
}

impl StructType {
    /// The last path segment (`P` for `demo::P`).
    pub fn short_name(&self) -> &str {
        self.path.rsplit("::").next().unwrap_or(&self.path)
    }
}

impl fmt::Display for StructType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path)
    }
}

/// A single field of a [`StructType`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct StructField {
    /// Field name as written in Noir.
    pub name: String,
    /// Field type.
    pub ty: TypeRepr,
}

/// Loads every `*.json` artifact directly inside `dir` (non-recursive), sorted by function name.
///
/// `dir` is usually the `export/` directory written by `nargo export`, or the staged copy that
/// `noi-build` places in `$OUT_DIR/noi_exports`.
///
/// # Errors
///
/// Fails if `dir` is missing or unreadable, if any artifact cannot be parsed or describes a type
/// Noir cannot produce (the error names the file and, for malformed JSON, the line and column), if
/// two artifacts declare the same function, or if no artifacts exist.
pub fn load_export_dir(dir: impl AsRef<Path>) -> Result<Vec<ExportFunction>> {
    let dir = dir.as_ref();
    let entries = fs::read_dir(dir).with_context(|| {
        format!(
            "cannot read export directory `{}`; run `nargo export` in your Noir package first",
            dir.display()
        )
    })?;

    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("cannot list `{}`", dir.display()))?
            .path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            paths.push(path);
        }
    }
    // Sort paths so loading (and therefore codegen and error messages) is deterministic.
    paths.sort();

    let mut functions = paths
        .iter()
        .map(load_export_file)
        .collect::<Result<Vec<_>>>()?;

    if functions.is_empty() {
        bail!(
            "no export artifacts (*.json) found in `{}`; run `nargo export` in your Noir package \
             and make sure functions are annotated with `#[export]`",
            dir.display()
        );
    }

    functions.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(pair) = functions.windows(2).find(|w| w[0].name == w[1].name) {
        bail!(
            "duplicate export `{}` in `{}` and `{}`",
            pair[0].name,
            pair[0].source_path.display(),
            pair[1].source_path.display()
        );
    }

    Ok(functions)
}

/// Loads a single `nargo export` artifact.
///
/// The function name is taken from the file stem, matching how `nargo export` names its output.
///
/// # Errors
///
/// Fails if the file cannot be read or does not contain a Noir program artifact with an `abi`.
pub fn load_export_file(path: impl AsRef<Path>) -> Result<ExportFunction> {
    let path = path.as_ref();
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            anyhow!(
                "export artifact `{}` has no UTF-8 file name",
                path.display()
            )
        })?
        .to_owned();
    let contents = fs::read_to_string(path)?;
    parse_export(&name, &contents)
        .with_context(|| format!("invalid export artifact `{}`", path.display()))
        .map(|mut function| {
            function.source_path = path.to_path_buf();
            function
        })
}

/// Parses an artifact's JSON text. `source_path` is left empty.
pub(crate) fn parse_export(name: &str, json: &str) -> Result<ExportFunction> {
    let raw: RawExport = serde_json::from_str(json)?;
    let function = raw.abi.into_function(name, raw.noir_version);
    validate(&function)?;
    Ok(function)
}

/// Rejects ABIs no Noir compiler produces, so later arithmetic on them cannot overflow or panic:
/// integer widths outside Noir's set, and encodings of more than `u32::MAX` field elements (Noir
/// uses `u32` lengths and witness indices).
fn validate(function: &ExportFunction) -> Result<()> {
    let mut total: u64 = 0;
    for ty in function
        .parameters
        .iter()
        .map(|p| &p.ty)
        .chain(&function.return_type)
    {
        total = total
            .checked_add(checked_field_count(ty)?)
            .filter(|t| *t <= u64::from(u32::MAX))
            .ok_or_else(|| {
                anyhow!(
                    "the ABI of `{}` encodes too many field elements",
                    function.name
                )
            })?;
    }
    Ok(())
}

/// Number of field elements `ty` occupies once encoded, or an error if it is not a valid Noir type
/// or exceeds `u32::MAX`.
pub(crate) fn checked_field_count(ty: &TypeRepr) -> Result<u64> {
    let limit = |n: Option<u64>| {
        n.filter(|n| *n <= u64::from(u32::MAX))
            .ok_or_else(|| anyhow!("type `{ty}` encodes too many field elements"))
    };
    match ty {
        TypeRepr::Bool | TypeRepr::Field => Ok(1),
        TypeRepr::Unsigned(1 | 8 | 16 | 32 | 64 | 128)
        | TypeRepr::Signed(8 | 16 | 32 | 64 | 128) => Ok(1),
        TypeRepr::Unsigned(_) | TypeRepr::Signed(_) => bail!("`{ty}` is not a Noir integer type"),
        TypeRepr::String(len) => limit(Some(*len as u64)),
        TypeRepr::Array(inner, len) => limit(checked_field_count(inner)?.checked_mul(*len as u64)),
        TypeRepr::Tuple(items) => {
            let mut sum = 0u64;
            for item in items {
                sum = limit(sum.checked_add(checked_field_count(item)?))?;
            }
            Ok(sum)
        }
        TypeRepr::Struct(st) => {
            let mut sum = 0u64;
            for field in &st.fields {
                sum = limit(sum.checked_add(checked_field_count(&field.ty)?))?;
            }
            Ok(sum)
        }
    }
}

/// How the ABI describes the payload of a failed assertion (`noirc_abi::AbiErrorType`).
#[cfg_attr(not(feature = "runner"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorType {
    /// `assert(cond, "literal")`: the message is known statically.
    String(String),
    /// `assert(cond, f"... {x} ...")`: a format string of `length` bytes followed by the item count
    /// and the items.
    FmtString { length: usize, items: Vec<TypeRepr> },
    /// `assert(cond, value)` with a non-string value.
    Custom(TypeRepr),
}

/// The ABI as the runtime needs it: function signature plus error descriptors keyed by selector.
#[cfg_attr(not(feature = "runner"), allow(dead_code))]
pub(crate) struct ParsedAbi {
    pub(crate) function: ExportFunction,
    pub(crate) error_types: BTreeMap<u64, ErrorType>,
}

impl<'de> Deserialize<'de> for ParsedAbi {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawAbi::deserialize(deserializer)?;
        let error_types = raw
            .error_types
            .iter()
            .map(|(selector, ty)| {
                let selector = selector.parse::<u64>().map_err(serde::de::Error::custom)?;
                let ty: ErrorType = ty.clone().into();
                let types: Vec<&TypeRepr> = match &ty {
                    ErrorType::String(_) => Vec::new(),
                    ErrorType::FmtString { items, .. } => items.iter().collect(),
                    ErrorType::Custom(t) => vec![t],
                };
                for t in types {
                    checked_field_count(t).map_err(serde::de::Error::custom)?;
                }
                Ok((selector, ty))
            })
            .collect::<Result<_, D::Error>>()?;
        let function = raw.into_function("", None);
        validate(&function).map_err(serde::de::Error::custom)?;
        Ok(ParsedAbi {
            function,
            error_types,
        })
    }
}

impl RawAbi {
    fn into_function(self, name: &str, noir_version: Option<String>) -> ExportFunction {
        let parameters = self
            .parameters
            .into_iter()
            .map(|p| Param {
                name: p.name,
                ty: p.ty.into(),
                visibility: p.visibility,
            })
            .collect();
        let (return_type, return_visibility) = match self.return_type {
            Some(ret) => (Some(ret.abi_type.into()), ret.visibility),
            None => (None, Visibility::Private),
        };
        ExportFunction {
            name: name.to_owned(),
            parameters,
            return_type,
            return_visibility,
            noir_version,
            source_path: PathBuf::new(),
        }
    }
}

// Serde mirrors of `noirc_abi` types. Kept private and decoupled from the Noir crates so the macro
// and build helpers do not pull in the compiler. Unknown fields (bytecode, debug symbols, …) are
// ignored.

#[derive(Deserialize)]
struct RawExport {
    #[serde(default)]
    noir_version: Option<String>,
    abi: RawAbi,
}

#[derive(Deserialize)]
struct RawAbi {
    #[serde(default)]
    parameters: Vec<RawParam>,
    #[serde(default)]
    return_type: Option<RawReturn>,
    /// Keyed by the error selector, a `u64` serialized as a decimal string.
    #[cfg_attr(not(feature = "runner"), allow(dead_code))]
    #[serde(default)]
    error_types: BTreeMap<String, RawErrorType>,
}

// Read only by `From<RawErrorType> for ErrorType`, which only `runner` code reaches.
#[cfg_attr(not(feature = "runner"), allow(dead_code))]
#[derive(Clone, Deserialize)]
#[serde(tag = "error_kind", rename_all = "lowercase")]
enum RawErrorType {
    String {
        string: String,
    },
    #[serde(rename = "fmtstring")]
    FmtString {
        length: usize,
        item_types: Vec<RawType>,
    },
    // `Custom(AbiType)` is internally tagged, so the type's own fields sit next to `error_kind`.
    Custom(RawType),
}

impl From<RawErrorType> for ErrorType {
    fn from(raw: RawErrorType) -> Self {
        match raw {
            RawErrorType::String { string } => ErrorType::String(string),
            RawErrorType::FmtString { length, item_types } => ErrorType::FmtString {
                length,
                items: item_types.into_iter().map(Into::into).collect(),
            },
            RawErrorType::Custom(ty) => ErrorType::Custom(ty.into()),
        }
    }
}

#[derive(Deserialize)]
struct RawParam {
    name: String,
    #[serde(rename = "type")]
    ty: RawType,
    #[serde(default)]
    visibility: Visibility,
}

#[derive(Deserialize)]
struct RawReturn {
    abi_type: RawType,
    #[serde(default)]
    visibility: Visibility,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum RawType {
    Field,
    Boolean,
    Integer {
        sign: RawSign,
        width: u16,
    },
    Array {
        length: usize,
        #[serde(rename = "type")]
        ty: Box<RawType>,
    },
    String {
        length: usize,
    },
    Tuple {
        fields: Vec<RawType>,
    },
    Struct {
        path: String,
        fields: Vec<RawStructField>,
    },
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawSign {
    Unsigned,
    Signed,
}

#[derive(Clone, Deserialize)]
struct RawStructField {
    name: String,
    #[serde(rename = "type")]
    ty: RawType,
}

impl From<RawType> for TypeRepr {
    fn from(raw: RawType) -> Self {
        match raw {
            RawType::Field => TypeRepr::Field,
            RawType::Boolean => TypeRepr::Bool,
            RawType::Integer {
                sign: RawSign::Unsigned,
                width,
            } => TypeRepr::Unsigned(width),
            RawType::Integer {
                sign: RawSign::Signed,
                width,
            } => TypeRepr::Signed(width),
            RawType::Array { length, ty } => TypeRepr::Array(Box::new((*ty).into()), length),
            RawType::String { length } => TypeRepr::String(length),
            RawType::Tuple { fields } => {
                TypeRepr::Tuple(fields.into_iter().map(Into::into).collect())
            }
            RawType::Struct { path, fields } => TypeRepr::Struct(StructType {
                path,
                fields: fields
                    .into_iter()
                    .map(|f| StructField {
                        name: f.name,
                        ty: f.ty.into(),
                    })
                    .collect(),
            }),
        }
    }
}

/// Every distinct struct reachable from `functions`, in a deterministic order.
///
/// Code generators need one Rust type per Noir struct *instantiation*: the ABI names a struct by
/// its path only, without generic arguments, so `Option<u8>` and `Option<u32>` share the path
/// `std::option::Option` but have different fields. Structs are therefore distinct by path **and**
/// layout. The result is sorted by path, and instantiations of the same path keep the order in
/// which they are first reached (functions are visited in name order, parameters before the
/// return type, outer types before inner ones).
pub fn collect_structs(functions: &[ExportFunction]) -> Vec<&StructType> {
    let mut seen: Vec<&StructType> = Vec::new();
    for function in functions {
        let types = function
            .parameters
            .iter()
            .map(|p| &p.ty)
            .chain(function.return_type.as_ref());
        for ty in types {
            ty.visit_structs(&mut |st| {
                if !seen.contains(&st) {
                    seen.push(st);
                }
            });
        }
    }
    // Stable sort: instantiations of one path stay in first-seen order.
    seen.sort_by(|a, b| a.path.cmp(&b.path));
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_abi_kinds() {
        let json = r#"{
            "noir_version": "1.0.0",
            "abi": {
                "abi_version": 1,
                "parameters": [
                    {"name": "s", "type": {"kind": "string", "length": 4}, "visibility": "private"},
                    {"name": "d", "type": {"kind": "integer", "sign": "unsigned", "width": 1}, "visibility": "databus"}
                ],
                "return_type": {"abi_type": {"kind": "field"}, "visibility": "public"},
                "error_types": {}
            }
        }"#;
        let f = parse_export("f", json).unwrap();
        assert_eq!(f.parameters[0].ty, TypeRepr::String(4));
        assert_eq!(f.parameters[1].visibility, Visibility::DataBus);
        assert_eq!(f.return_visibility, Visibility::Public);
        assert_eq!(
            f.signature(),
            "fn f(s: str<4>, call_data d: u1) -> pub Field"
        );
        assert_eq!(f.public_parameters().count(), 1);
    }

    #[test]
    fn unknown_kind_is_reported() {
        let json = r#"{"abi":{"parameters":[{"name":"x","type":{"kind":"slice"}}]}}"#;
        let err = parse_export("f", json).unwrap_err();
        assert!(format!("{err:#}").contains("slice"), "{err:#}");
    }

    #[test]
    fn struct_short_name() {
        let st = StructType {
            path: "a::b::Order".into(),
            fields: vec![],
        };
        assert_eq!(st.short_name(), "Order");
    }
}
