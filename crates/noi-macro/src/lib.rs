use std::{env, fs, path::PathBuf};

use heck::{ToSnakeCase, ToUpperCamelCase};
use indexmap::IndexMap;
use noi_core::{
    export::{ExportFunction, Param, StructField, StructType, TypeRepr},
    load_export_dir,
};
use proc_macro::TokenStream;
use proc_macro_error::proc_macro_error;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    Ident, LitStr, Token,
};

/// Generates strongly typed Rust bindings for Noir `#[export]` functions.
///
/// The macro consumes `nargo export` artifacts (either via `export_dir = "..."` or the
/// `NOI_EXPORT_DIR` environment variable) and emits per-function modules that expose
/// typed `Args`, `Inputs`, and stubbed execution helpers.
#[proc_macro]
#[proc_macro_error]
pub fn nrg(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as MacroInput);
    match expand(input) {
        Ok(tokens) => {
            maybe_dump(&tokens);
            tokens.into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(input: MacroInput) -> Result<proc_macro2::TokenStream, syn::Error> {
    let export_dir = resolve_export_dir(input.export_dir)?;
    let functions =
        load_export_dir(&export_dir).map_err(|err| syn::Error::new(input.module.span(), err))?;

    let mut registry = TypeRegistry::new(&input.module);
    let mut function_modules = Vec::new();

    for function in &functions {
        function_modules.push(generate_function_module(function, &mut registry)?);
    }

    let struct_defs = registry.struct_defs();
    let module_ident = &input.module;
    let client = generate_client();

    let tokens = quote! {
        pub mod #module_ident {
            use ::std::path::{Path, PathBuf};

            #client

            #(#struct_defs)*

            #(#function_modules)*
        }
    };

    Ok(tokens)
}

fn generate_client() -> proc_macro2::TokenStream {
    quote! {
        #[derive(Clone, Debug)]
        pub struct Client {
            program_dir: PathBuf,
        }

        impl Client {
            pub fn new<P: Into<PathBuf>>(program_dir: P) -> Self {
                Self { program_dir: program_dir.into() }
            }

            pub fn program_dir(&self) -> &Path {
                &self.program_dir
            }
        }
    }
}

fn generate_function_module(
    function: &ExportFunction,
    registry: &mut TypeRegistry,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let module_ident = format_ident!("{}", sanitize_snake(&function.name));
    let doc = function.signature();

    let params = build_param_specs(function, registry);

    let args_struct = build_args_struct(&params, &doc);
    let (public_struct, private_struct) = build_visibility_structs(&params);
    let inputs_struct = build_inputs_struct();
    let converters = build_converters(&params);
    let output_ty = match &function.return_type {
        Some(ty) => registry.ty_tokens(ty, TypeUsage::Reference),
        None => quote!(()),
    };

    let artifact_path = canonical_path(&function.source_path);
    let artifact_lit = LitStr::new(&artifact_path, proc_macro2::Span::call_site());

    let simulate_fn = quote! {
        pub fn simulate(_client: &super::Client, _args: Args) -> ::anyhow::Result<Output> {
            Err(::anyhow::anyhow!("`noi` runner integration is not implemented yet"))
        }
    };

    let module = quote! {
        #[doc = #doc]
        pub mod #module_ident {
            use super::Client;

            pub const ARTIFACT_JSON: &str = include_str!(#artifact_lit);

            #args_struct
            #public_struct
            #private_struct
            #inputs_struct
            #converters

            pub type Output = #output_ty;

            #simulate_fn
        }
    };

    Ok(module)
}

fn build_param_specs(function: &ExportFunction, registry: &mut TypeRegistry) -> Vec<ParamSpec> {
    function
        .parameters
        .iter()
        .map(|param| ParamSpec::new(param, registry))
        .collect()
}

struct ParamSpec {
    ident: Ident,
    ty: proc_macro2::TokenStream,
    visibility: VisibilityClass,
}

impl ParamSpec {
    fn new(param: &Param, registry: &mut TypeRegistry) -> Self {
        let ident = format_ident!("{}", sanitize_snake(&param.name));
        let ty = registry.ty_tokens(&param.ty, TypeUsage::Reference);
        let visibility = match param.visibility {
            noi_core::export::Visibility::Public => VisibilityClass::Public,
            noi_core::export::Visibility::Private => VisibilityClass::Private,
        };
        Self {
            ident,
            ty,
            visibility,
        }
    }
}

enum VisibilityClass {
    Public,
    Private,
}

fn build_args_struct(params: &[ParamSpec], doc: &str) -> proc_macro2::TokenStream {
    let fields = params.iter().map(|param| {
        let ident = &param.ident;
        let ty = &param.ty;
        quote!(pub #ident: #ty,)
    });
    quote! {
        #[doc = #doc]
        #[derive(Clone, Debug, PartialEq)]
        pub struct Args {
            #(#fields)*
        }
    }
}

fn build_visibility_structs(
    params: &[ParamSpec],
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let mut public = Vec::new();
    let mut private = Vec::new();
    for param in params {
        match param.visibility {
            VisibilityClass::Public => public.push(param),
            VisibilityClass::Private => private.push(param),
        }
    }

    (
        visibility_struct_tokens("PublicInputs", &public),
        visibility_struct_tokens("PrivateInputs", &private),
    )
}

fn visibility_struct_tokens(name: &str, fields: &[&ParamSpec]) -> proc_macro2::TokenStream {
    let ident = format_ident!("{name}");
    let field_tokens = fields.iter().map(|param| {
        let ident = &param.ident;
        let ty = &param.ty;
        quote!(pub #ident: #ty,)
    });

    quote! {
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct #ident {
            #(#field_tokens)*
        }
    }
}

fn build_inputs_struct() -> proc_macro2::TokenStream {
    quote! {
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct Inputs {
            pub public: PublicInputs,
            pub private: PrivateInputs,
        }
    }
}

fn build_converters(params: &[ParamSpec]) -> proc_macro2::TokenStream {
    let public_init: Vec<_> = params
        .iter()
        .filter(|param| matches!(param.visibility, VisibilityClass::Public))
        .map(|param| {
            let ident = &param.ident;
            quote!(#ident: args.#ident,)
        })
        .collect();
    let private_init: Vec<_> = params
        .iter()
        .filter(|param| matches!(param.visibility, VisibilityClass::Private))
        .map(|param| {
            let ident = &param.ident;
            quote!(#ident: args.#ident,)
        })
        .collect();

    let args_from_inputs_fields = params.iter().map(|param| {
        let ident = &param.ident;
        match param.visibility {
            VisibilityClass::Public => quote!(#ident: inputs.public.#ident),
            VisibilityClass::Private => quote!(#ident: inputs.private.#ident),
        }
    });

    quote! {
        impl From<Args> for Inputs {
            fn from(args: Args) -> Self {
                Self {
                    public: PublicInputs {
                        #(#public_init)*
                    },
                    private: PrivateInputs {
                        #(#private_init)*
                    },
                }
            }
        }

        impl From<Inputs> for Args {
            fn from(inputs: Inputs) -> Self {
                Self {
                    #(#args_from_inputs_fields,)*
                }
            }
        }
    }
}

fn sanitize_snake(name: &str) -> String {
    sanitize(name).to_snake_case()
}

fn sanitize_pascal(name: &str) -> String {
    sanitize(name).to_upper_camel_case()
}

fn sanitize(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('x');
    }
    if out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

fn canonical_path(path: &PathBuf) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.clone())
        .to_string_lossy()
        .into_owned()
}

fn resolve_export_dir(explicit: Option<LitStr>) -> Result<PathBuf, syn::Error> {
    if let Some(lit) = explicit {
        return Ok(PathBuf::from(lit.value()));
    }

    match env::var("NOI_EXPORT_DIR") {
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "`NOI_EXPORT_DIR` is not set and `export_dir` was not provided",
        )),
    }
}

fn maybe_dump(tokens: &proc_macro2::TokenStream) {
    if env::var("NOI_DEBUG").as_deref() != Ok("1") {
        return;
    }
    let target_dir = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target"));
    let dump_path = target_dir.join("noi").join("expanded.rs");
    if let Some(parent) = dump_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(dump_path, tokens.to_string());
}

struct MacroInput {
    module: Ident,
    export_dir: Option<LitStr>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut module = None;
        let mut export_dir = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "module" => {
                    module = Some(input.parse()?);
                }
                "export_dir" => {
                    export_dir = Some(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown argument `{other}`"),
                    ))
                }
            }
            if input.is_empty() {
                break;
            }
            let _ = input.parse::<Token![,]>();
        }

        let module = module.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "`module = <ident>` is required",
            )
        })?;

        Ok(Self { module, export_dir })
    }
}

struct TypeRegistry {
    structs: IndexMap<StructType, Ident>,
    defs: Vec<proc_macro2::TokenStream>,
    module_name: String,
    counter: usize,
}

#[derive(Clone, Copy)]
enum TypeUsage {
    Definition,
    Reference,
}

impl TypeRegistry {
    fn new(module: &Ident) -> Self {
        Self {
            structs: IndexMap::new(),
            defs: Vec::new(),
            module_name: module.to_string(),
            counter: 0,
        }
    }

    fn ty_tokens(&mut self, repr: &TypeRepr, usage: TypeUsage) -> proc_macro2::TokenStream {
        match repr {
            TypeRepr::Bool => quote!(bool),
            TypeRepr::Field => quote!(::noi_core::types::FieldElement),
            TypeRepr::Unsigned(bits) => {
                let ident = format_ident!("u{}", bits);
                quote!(#ident)
            }
            TypeRepr::Signed(bits) => {
                let ident = format_ident!("i{}", bits);
                quote!(#ident)
            }
            TypeRepr::Array(inner, len) => {
                let inner_tokens = self.ty_tokens(inner, usage);
                let len_lit = proc_macro2::Literal::usize_unsuffixed(*len);
                quote!([#inner_tokens; #len_lit])
            }
            TypeRepr::Tuple(values) => {
                let tokens = values
                    .iter()
                    .map(|value| self.ty_tokens(value, usage))
                    .collect::<Vec<_>>();
                match tokens.len() {
                    0 => quote!(()),
                    1 => {
                        let ty = &tokens[0];
                        quote!((#ty,))
                    }
                    _ => quote!((#(#tokens),*)),
                }
            }
            TypeRepr::Struct(struct_ty) => {
                let ident = self.ensure_struct(struct_ty);
                match usage {
                    TypeUsage::Definition => quote!(#ident),
                    TypeUsage::Reference => quote!(super::#ident),
                }
            }
        }
    }

    fn ensure_struct(&mut self, struct_ty: &StructType) -> Ident {
        if let Some(existing) = self.structs.get(struct_ty) {
            return existing.clone();
        }

        let ident = self.next_struct_ident(struct_ty);
        self.structs.insert(struct_ty.clone(), ident.clone());

        let fields = struct_ty
            .fields
            .iter()
            .map(|field| self.struct_field_tokens(field))
            .collect::<Vec<_>>();

        let def = quote! {
            #[derive(Clone, Debug, PartialEq, Default)]
            pub struct #ident {
                #(#fields)*
            }
        };
        self.defs.push(def);

        ident
    }

    fn struct_field_tokens(&mut self, field: &StructField) -> proc_macro2::TokenStream {
        let ident = format_ident!("{}", sanitize_snake(&field.name));
        let ty = self.ty_tokens(&field.ty, TypeUsage::Definition);
        quote!(pub #ident: #ty,)
    }

    fn next_struct_ident(&mut self, struct_ty: &StructType) -> Ident {
        let base = struct_ty
            .name
            .as_deref()
            .map(sanitize_pascal)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "{}Struct{}",
                    self.module_name.to_upper_camel_case(),
                    self.counter
                )
            });
        self.counter += 1;
        format_ident!("{base}")
    }

    fn struct_defs(&self) -> Vec<proc_macro2::TokenStream> {
        self.defs.clone()
    }
}
