//! ToolDef derive: MCP tool definitions from struct field metadata.

use proc_macro2::TokenStream;
use quote::quote;
use std::collections::HashSet;
use syn::{Data, DeriveInput, Fields};

use crate::attr_utils::{has_serde_default, parse_meta_bool, parse_meta_string};
use crate::type_mapping::{JsonTypeKind, is_option_type, json_type_kind};

/// Extracts #`tool(name = "...", description = "...")` from the struct.
///
/// Extracts fields from the struct.
///
pub(crate) fn impl_tool_def(input: &DeriveInput) -> syn::Result<TokenStream> {
    let struct_name = &input.ident;

    let (tool_name, tool_description) = extract_tool_attrs(input)?;

    let fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => fields,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "ToolDef can only be derived on structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "ToolDef can only be derived on structs",
            ));
        }
    };

    let (property_inserts, definition_inserts, required_tokens) = collect_field_schema(fields)?;

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

            /// Returns the full MCP tool definition in the canonical serde shape
            /// consumed by `ToolDefinition` (snake_case `input_schema`, not
            /// camelCase `inputSchema`). `parameters` is always empty: the full
            /// input schema is the single source of truth for consumers.
            pub fn definition() -> serde_json::Value {
                serde_json::json!({
                    "name": Self::TOOL_NAME,
                    "description": Self::TOOL_DESCRIPTION,
                    "parameters": [],
                    "input_schema": Self::json_schema()
                })
            }
        }
    };

    Ok(expanded)
}

/// Per-field metadata collected from a `#[tool_param(...)]` field, shared by
/// `ToolDef` and `ToolPlugin` so both derives emit identical schema shapes.
pub(crate) struct ToolFieldMeta {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) kind: JsonTypeKind,
}

/// Build the schema-generation statement groups from the struct's named fields.
///
/// Returns `(property_inserts, definition_inserts, required_tokens)` — the
/// statement fragments that populate the `properties`, `definitions`, and
/// `required` members of the generated JSON Schema. Shared by `ToolDef` and
/// `ToolPlugin`; the uniform required rule (Option / serde(default) /
/// explicit `required = ...`) lives here so it cannot diverge between the two
/// derives.
/// Uniform required rule (shared with JsonSchema): a field is NOT required when it is `Option<T>`,
/// carries `#[serde(default)]`, or is explicitly marked `required = false`. An explicit
/// `#[tool_param(required = ...)]` always overrides the inference.
///
/// Builds property insertions. Custom types map to `$ref` so ToolDef and JsonSchema emit the same
/// schema shape for the same type.
///
/// Placeholder definitions for every custom type, deduplicated by name.
///
pub(crate) fn collect_field_schema(
    fields: &syn::FieldsNamed,
) -> syn::Result<(Vec<TokenStream>, Vec<TokenStream>, Vec<TokenStream>)> {
    let mut field_meta = Vec::new();
    let mut required_names = Vec::new();

    for field in &fields.named {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        let (param_description, param_required) = extract_tool_param_attrs(field)?;
        let has_default = has_serde_default(&field.attrs)?;
        let is_option = is_option_type(&field.ty);
        let type_kind = json_type_kind(&field.ty);

        let required = if is_option || has_default {
            param_required.unwrap_or(false)
        } else {
            param_required.unwrap_or(true)
        };

        let description = param_description.unwrap_or_default();

        if required {
            required_names.push(field_name_str.clone());
        }

        field_meta.push(ToolFieldMeta {
            name: field_name_str,
            description,
            kind: type_kind,
        });
    }

    let property_inserts: Vec<TokenStream> = field_meta
        .iter()
        .map(|meta| match &meta.kind {
            JsonTypeKind::Primitive(keyword) => {
                let name = &meta.name;
                let desc = &meta.description;
                quote! {
                    properties.insert(#name.to_string(), serde_json::json!({
                        "type": #keyword,
                        "description": #desc
                    }));
                }
            }
            JsonTypeKind::Array => {
                let name = &meta.name;
                let desc = &meta.description;
                quote! {
                    properties.insert(#name.to_string(), serde_json::json!({
                        "type": "array",
                        "description": #desc
                    }));
                }
            }
            JsonTypeKind::Custom(type_name) => {
                let name = &meta.name;
                let desc = &meta.description;
                let ref_str = format!("#/definitions/{}", type_name);
                quote! {
                    properties.insert(#name.to_string(), serde_json::json!({
                        "$ref": #ref_str,
                        "description": #desc
                    }));
                }
            }
        })
        .collect();

    let mut seen_defs = HashSet::new();
    let definition_inserts: Vec<TokenStream> = field_meta
        .iter()
        .filter_map(|meta| match &meta.kind {
            JsonTypeKind::Custom(type_name) => Some(type_name.clone()),
            _ => None,
        })
        .filter(|name| seen_defs.insert(name.clone()))
        .map(|name| {
            quote! {
                definitions.insert(#name.to_string(), serde_json::json!({"type": "object"}));
            }
        })
        .collect();

    let required_tokens: Vec<TokenStream> = required_names
        .iter()
        .map(|name| quote! { #name.to_string() })
        .collect();

    Ok((property_inserts, definition_inserts, required_tokens))
}

/// Extract tool name and description from `#[tool(name = "...", description = "...")]`.
pub(crate) fn extract_tool_attrs(input: &DeriveInput) -> syn::Result<(String, String)> {
    let mut name = None;
    let mut description = None;

    for attr in &input.attrs {
        if !attr.path().is_ident("tool") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                name = Some(parse_meta_string(&meta, "name")?);
            } else if meta.path.is_ident("description") {
                description = Some(parse_meta_string(&meta, "description")?);
            } else {
                return Err(meta.error("unknown tool attribute, expected `name` or `description`"));
            }
            Ok(())
        })?;
    }

    let name = name.ok_or_else(|| {
        syn::Error::new_spanned(
            input,
            "ToolDef requires `#[tool(name = \"...\")]` attribute",
        )
    })?;
    let description = description.unwrap_or_default();

    Ok((name, description))
}

/// Extract parameter description and required flag from `#[tool_param(...)]`.
pub(crate) fn extract_tool_param_attrs(
    field: &syn::Field,
) -> syn::Result<(Option<String>, Option<bool>)> {
    let mut description = None;
    let mut required = None;

    for attr in &field.attrs {
        if !attr.path().is_ident("tool_param") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("description") {
                description = Some(parse_meta_string(&meta, "description")?);
            } else if meta.path.is_ident("required") {
                required = Some(parse_meta_bool(&meta, "required")?);
            } else {
                return Err(meta
                    .error("unknown tool_param attribute, expected `description` or `required`"));
            }
            Ok(())
        })?;
    }

    Ok((description, required))
}
