//! ToolPlugin derive: complete plugin-facing replacement for `ToolDef`.
//!
//! A `ToolPlugin` struct is a tool that ships with its own executable
//! handler. Unlike `ToolDef` (metadata only), this derive additionally
//! generates `invoke(args)` bound to a handler path and a typed
//! `definition()` deserialized into a host-facing definition type.
//!
//! Reuses `ToolDef`'s attribute parsing (`tool`, `tool_param`) and adds the
//! struct-level `plugin` attribute:
//!
//! ```ignore
//! #[derive(ToolPlugin)]
//! #[tool(name = "multiply", description = "Multiply two numbers")]
//! #[plugin(handler = "my_module::multiply_handler", definition = "antikythera_toolrunner::ToolDefinition")]
//! struct MultiplyTool { ... }
//! ```
//!
//! Because both derives emit the same constants and methods (`TOOL_NAME`,
//! `TOOL_DESCRIPTION`, `json_schema()`, `definition()`), a struct must not
//! derive `ToolDef` and `ToolPlugin` simultaneously — `ToolPlugin` is a full
//! replacement for `ToolDef` when the tool has an in-process handler.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

use crate::attr_utils::parse_meta_string;
use crate::tool_def::{collect_field_schema, extract_tool_attrs};

/// Extracts #`tool(name = "...", description = "...")` from the struct.
///
/// Extracts #`plugin(handler = "...", definition = "...")` from the struct.
///
/// Extracts fields from the struct.
///
/// `definition()` mirrors `definition_json()` when the configured type is serde_json::Value (the
/// default); otherwise it deserializes through serde, failing loudly if the canonical shape no
/// longer fits the type.
///
pub(crate) fn impl_tool_plugin(input: &DeriveInput) -> syn::Result<TokenStream> {
    let struct_name = &input.ident;

    let (tool_name, tool_description) = extract_tool_attrs(input)?;

    let (handler, definition_type, definition_is_json_value) = extract_plugin_attrs(input)?;

    let fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => fields,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "ToolPlugin can only be derived on structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "ToolPlugin can only be derived on structs",
            ));
        }
    };

    let (property_inserts, definition_inserts, required_tokens) = collect_field_schema(fields)?;

    let definition_fn = if definition_is_json_value {
        quote! {
            /// Returns the full tool definition in the canonical serde shape.
            pub fn definition() -> serde_json::Value {
                Self::definition_json()
            }
        }
    } else {
        let def_ty = &definition_type;
        quote! {
            /// Returns the full tool definition deserialized into `#def_ty`.
            pub fn definition() -> #def_ty {
                serde_json::from_value(Self::definition_json()).unwrap_or_else(|e| {
                    panic!(
                        "ToolPlugin definition_json must deserialize into the configured definition type: {}",
                        e
                    )
                })
            }
        }
    };

    let expanded = quote! {
        impl #struct_name {
            /// The MCP tool name.
            pub const TOOL_NAME: &'static str = #tool_name;

            /// The MCP tool description.
            pub const TOOL_DESCRIPTION: &'static str = #tool_description;

            /// Returns the JSON Schema for this tool's input parameters.
            pub fn json_schema() -> serde_json::Value {
                let mut properties = serde_json::Map::new();
                #(#property_inserts)*

                let mut definitions = serde_json::Map::new();
                #(#definition_inserts)*

                let required: Vec<String> = vec![#(#required_tokens),*];

                let mut schema = serde_json::Map::new();
                schema.insert("type".to_string(), serde_json::Value::String("object".to_string()));
                schema.insert("properties".to_string(), serde_json::Value::Object(properties));
                schema.insert("required".to_string(), serde_json::to_value(&required).unwrap());
                if !definitions.is_empty() {
                    schema.insert("definitions".to_string(), serde_json::Value::Object(definitions));
                }

                serde_json::Value::Object(schema)
            }

            /// Returns the full tool definition in the canonical serde shape
            /// consumed by host-facing `ToolDefinition` types.
            pub fn definition_json() -> serde_json::Value {
                serde_json::json!({
                    "name": Self::TOOL_NAME,
                    "description": Self::TOOL_DESCRIPTION,
                    "parameters": [],
                    "input_schema": Self::json_schema()
                })
            }

            #definition_fn

            /// Invokes the configured handler with JSON arguments.
            pub fn invoke(args: serde_json::Value) -> Result<serde_json::Value, String> {
                #handler(&args)
            }

            /// Names of the tools exported by this plugin.
            pub const PLUGIN_TOOLS: &[&str] = &[Self::TOOL_NAME];
        }
    };

    Ok(expanded)
}

/// Extract `handler` and optional `definition` from `#[plugin(...)]`.
///
/// `handler` is a required string literal containing a Rust function path
/// matching `fn(&serde_json::Value) -> Result<serde_json::Value, String>`.
/// `definition` is an optional string literal containing the type path of the
/// host-facing definition; it defaults to `serde_json::Value`.
///
/// Returns `(handler, definition_type, definition_is_json_value)`. Every
/// malformed form — missing handler, unknown key, duplicate key or attribute,
/// or a value that is not a valid Rust path/type — is a `syn::Error`.
fn extract_plugin_attrs(input: &DeriveInput) -> syn::Result<(syn::Path, syn::Type, bool)> {
    let mut handler: Option<syn::Path> = None;
    let mut definition: Option<syn::Type> = None;
    let mut attr_seen = false;

    for attr in &input.attrs {
        if !attr.path().is_ident("plugin") {
            continue;
        }
        if attr_seen {
            return Err(syn::Error::new_spanned(
                attr,
                "multiple #[plugin] attributes are not allowed",
            ));
        }
        attr_seen = true;

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("handler") {
                if handler.is_some() {
                    return Err(meta.error("duplicate `handler` key in #[plugin]"));
                }
                let raw = parse_meta_string(&meta, "handler")?;
                handler = Some(syn::parse_str::<syn::Path>(&raw).map_err(|_| {
                    meta.error(format!(
                        "`handler` must be a valid Rust function path, got `{raw}`"
                    ))
                })?);
            } else if meta.path.is_ident("definition") {
                if definition.is_some() {
                    return Err(meta.error("duplicate `definition` key in #[plugin]"));
                }
                let raw = parse_meta_string(&meta, "definition")?;
                definition = Some(syn::parse_str::<syn::Type>(&raw).map_err(|_| {
                    meta.error(format!(
                        "`definition` must be a valid Rust type path, got `{raw}`"
                    ))
                })?);
            } else {
                return Err(
                    meta.error("unknown plugin attribute, expected `handler` or `definition`")
                );
            }
            Ok(())
        })?;
    }

    let handler = handler.ok_or_else(|| {
        syn::Error::new_spanned(
            input,
            "ToolPlugin requires `#[plugin(handler = \"path::to::handler\")]` attribute",
        )
    })?;

    let definition = definition.unwrap_or_else(|| {
        syn::parse_str::<syn::Type>("serde_json::Value")
            .expect("static parsing of the serde_json::Value type cannot fail")
    });

    let default_type: syn::Type = syn::parse_str("serde_json::Value")
        .expect("static parsing of the serde_json::Value type cannot fail");
    let definition_is_json_value =
        quote!(#definition).to_string() == quote!(#default_type).to_string();

    Ok((handler, definition, definition_is_json_value))
}
