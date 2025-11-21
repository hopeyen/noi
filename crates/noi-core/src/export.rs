use std::{
    fmt,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use fs_err as fs;
use serde::Deserialize;
use serde_json::Value;
use walkdir::WalkDir;

/// Parsed metadata for a single Noir `#[export]` function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFunction {
    pub name: String,
    pub parameters: Vec<Param>,
    pub return_type: Option<TypeRepr>,
    pub visibility: Visibility,
    pub source_path: PathBuf,
}

impl ExportFunction {
    pub fn signature(&self) -> String {
        let params = self
            .parameters
            .iter()
            .map(|param| format!("{}: {}", param.name, param.ty))
            .collect::<Vec<_>>()
            .join(", ");
        match &self.return_type {
            Some(ret) => format!("fn {}({}) -> {}", self.name, params, ret),
            None => format!("fn {}({})", self.name, params),
        }
    }
}

/// Function visibility marker as reported by the Noir compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Private
    }
}

/// A single parameter entry in the exported ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: TypeRepr,
    pub visibility: Visibility,
}

/// Structured representation of Noir ABI types supported by `noi`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeRepr {
    Bool,
    Field,
    Unsigned(u16),
    Signed(u16),
    Array(Box<TypeRepr>, usize),
    Tuple(Vec<TypeRepr>),
    Struct(StructType),
}

impl TypeRepr {
    pub fn is_struct(&self) -> bool {
        matches!(self, TypeRepr::Struct(_))
    }
}

impl fmt::Display for TypeRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRepr::Bool => write!(f, "bool"),
            TypeRepr::Field => write!(f, "Field"),
            TypeRepr::Unsigned(bits) => write!(f, "u{}", bits),
            TypeRepr::Signed(bits) => write!(f, "i{}", bits),
            TypeRepr::Array(elem, len) => write!(f, "[{}; {}]", elem, len),
            TypeRepr::Tuple(values) => {
                let repr = values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({repr})")
            }
            TypeRepr::Struct(struct_ty) => struct_ty.fmt(f),
        }
    }
}

/// Definition of a Noir struct type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructType {
    pub name: Option<String>,
    pub fields: Vec<StructField>,
}

impl fmt::Display for StructType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "struct {}", name),
            None => write!(f, "struct"),
        }
    }
}

/// A single field inside a struct type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructField {
    pub name: String,
    pub ty: TypeRepr,
}

/// Load every JSON artifact inside `path` and parse it into [`ExportFunction`]s.
pub fn load_export_dir(path: impl AsRef<Path>) -> Result<Vec<ExportFunction>> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(anyhow!(
            "export directory `{}` does not exist",
            path.display()
        ));
    }

    let mut functions = Vec::new();

    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext != "json")
            .unwrap_or(true)
        {
            continue;
        }

        let contents = fs::read_to_string(entry.path())
            .with_context(|| format!("failed to read export JSON `{}`", entry.path().display()))?;

        let artifact: RawExport = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse export JSON `{}`", entry.path().display()))?;

        if artifact.abi.parameters.is_empty() && artifact.abi.return_type.is_none() {
            continue;
        }

        let params = artifact
            .abi
            .parameters
            .into_iter()
            .map(|param| -> Result<_> {
                let ty = parse_type(&param.r#type).with_context(|| {
                    format!("failed to parse type for parameter `{}`", param.name)
                })?;
                Ok(Param {
                    name: param.name,
                    ty,
                    visibility: param.visibility.unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let return_type = match artifact.abi.return_type {
            Some(ret) => {
                let ty_value = ret
                    .abi_type
                    .as_ref()
                    .ok_or_else(|| anyhow!("missing `abi_type` for return type"))?;
                Some(parse_type(ty_value)?)
            }
            None => None,
        };

        let name = artifact
            .name
            .clone()
            .or_else(|| artifact.function.clone())
            .unwrap_or_else(|| {
                entry
                    .path()
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_string()
            });

        let visibility = artifact.visibility.unwrap_or_default();

        functions.push(ExportFunction {
            name,
            parameters: params,
            return_type,
            visibility,
            source_path: entry.path().to_path_buf(),
        });
    }

    functions.sort_by(|a, b| a.name.cmp(&b.name));

    if functions.is_empty() {
        return Err(anyhow!(
            "no export artifacts found in `{}`; run `nargo export` first",
            path.display()
        ));
    }

    Ok(functions)
}

#[derive(Debug, Deserialize)]
struct RawExport {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "function_name")]
    function: Option<String>,
    #[serde(default)]
    visibility: Option<Visibility>,
    abi: RawAbi,
}

#[derive(Debug, Deserialize)]
struct RawAbi {
    #[serde(default)]
    parameters: Vec<RawParam>,
    #[serde(default)]
    return_type: Option<RawReturn>,
}

#[derive(Debug, Deserialize)]
struct RawParam {
    name: String,
    #[serde(rename = "type")]
    r#type: Value,
    #[serde(default)]
    visibility: Option<Visibility>,
}

#[derive(Debug, Deserialize)]
struct RawReturn {
    #[serde(default, rename = "abi_type")]
    abi_type: Option<Value>,
}

fn parse_type(node: &Value) -> Result<TypeRepr> {
    let kind = node
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing `kind` in ABI type"))?;

    match kind {
        "field" => Ok(TypeRepr::Field),
        "boolean" => Ok(TypeRepr::Bool),
        "unsigned" | "integer" => {
            let width = node
                .get("width")
                .or_else(|| node.get("bit_size"))
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow!("missing integer width in abi type"))?
                as u16;
            let sign = node
                .get("sign")
                .and_then(Value::as_str)
                .unwrap_or("unsigned");
            match sign {
                "unsigned" => Ok(TypeRepr::Unsigned(width)),
                "signed" => Ok(TypeRepr::Signed(width)),
                other => Err(anyhow!("unknown integer sign `{other}`")),
            }
        }
        "signed" => {
            let width = node
                .get("width")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow!("missing signed width"))? as u16;
            Ok(TypeRepr::Signed(width))
        }
        "array" => {
            let len = node
                .get("length")
                .or_else(|| node.get("len"))
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow!("missing array length"))? as usize;
            let inner = node
                .get("type")
                .or_else(|| node.get("r#type"))
                .ok_or_else(|| anyhow!("missing array element type"))?;
            Ok(TypeRepr::Array(Box::new(parse_type(inner)?), len))
        }
        "tuple" => {
            let fields = node
                .get("fields")
                .or_else(|| node.get("types"))
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("tuple `fields` must be an array"))?;
            let parsed = fields.iter().map(parse_type).collect::<Result<Vec<_>>>()?;
            Ok(TypeRepr::Tuple(parsed))
        }
        "struct" => parse_struct(node),
        other => Err(anyhow!("unsupported type kind `{other}`")),
    }
}

fn parse_struct(node: &Value) -> Result<TypeRepr> {
    let name = node
        .get("path")
        .or_else(|| node.get("name"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let mut fields = Vec::new();

    if let Some(array) = node.get("fields").and_then(Value::as_array) {
        for field in array {
            let field_name = field
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("struct field missing name"))?;
            let ty_node = field
                .get("type")
                .or_else(|| field.get("abi_type"))
                .ok_or_else(|| anyhow!("struct field missing type"))?;
            fields.push(StructField {
                name: field_name.to_string(),
                ty: parse_type(ty_node)?,
            });
        }
    } else if let Some(map) = node.get("fields").and_then(Value::as_object) {
        for (field_name, value) in map {
            let ty_node = value
                .get("type")
                .or_else(|| value.get("abi_type"))
                .ok_or_else(|| anyhow!("struct field missing type"))?;
            fields.push(StructField {
                name: field_name.clone(),
                ty: parse_type(ty_node)?,
            });
        }
    } else {
        return Err(anyhow!("struct `fields` must be an array or object"));
    }

    Ok(TypeRepr::Struct(StructType { name, fields }))
}
