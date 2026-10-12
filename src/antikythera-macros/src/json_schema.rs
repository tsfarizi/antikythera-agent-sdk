//! JsonSchema derive: JSON Schema from struct definitions.
//!
//! Produces a `json_schema()` method returning the schema as a `serde_json::Value`,
//! useful for runtime type validation and tool parameter documentation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

use crate::attr_utils::has_serde_default;
use crate::type_mapping::{JsonTypeKind, is_option_type, json_type_kind, unwrap_option_type};

/// Collect property insertions, required field names, and definition insertions.
///
/// Deduplicate definitions: keep only the first occurrence of each type name.
///
pub(crate) fn impl_json_schema(input: &DeriveInput) -> syn::Result<TokenStream> {
    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "JsonSchema can only be derived on structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "JsonSchema can only be derived on structs",
            ));
        }
    };

    let mut prop_inserts: Vec<TokenStream> = Vec::new();
    let mut required_names: Vec<String> = Vec::new();
    let mut def_inserts: Vec<(String, TokenStream)> = Vec::new();

    for field in fields {
        let field_name_str = field.ident.as_ref().unwrap().to_string();

        let has_default = has_serde_default(&field.attrs)?;
        let is_option = is_option_type(&field.ty);

        let required = !has_default && !is_option;
        if required {
            required_names.push(field_name_str.clone());
        }

        let (schema_expr, nested_defs) = field_type_schema(&field.ty)?;
        prop_inserts.push(quote! {
            properties.insert(#field_name_str.to_string(), #schema_expr);
        });
        for (name, def_schema) in nested_defs {
            def_inserts.push((name, def_schema));
        }
    }

    let mut seen_defs = std::collections::HashSet::new();
    let unique_def_inserts: Vec<TokenStream> = def_inserts
        .into_iter()
        .filter_map(|(name, schema)| {
            if seen_defs.insert(name.clone()) {
                Some(quote! {
                    definitions.insert(#name.to_string(), #schema);
                })
            } else {
                None
            }
        })
        .collect();

    let required_tokens: Vec<TokenStream> = required_names
        .iter()
        .map(|n| quote! { #n.to_string() })
        .collect();

    let expanded = quote! {
        impl #struct_name {
            /// Returns the JSON Schema for this type (draft-07).
            pub fn json_schema() -> serde_json::Value {
                let mut properties = serde_json::Map::new();
                #(#prop_inserts)*

                let mut definitions = serde_json::Map::new();
                #(#unique_def_inserts)*

                let required: Vec<String> = vec![#(#required_tokens),*];

                let mut schema = serde_json::Map::new();
                schema.insert("type".to_string(), serde_json::Value::String("object".to_string()));
                schema.insert("properties".to_string(), serde_json::Value::Object(properties));
                if !required.is_empty() {
                    schema.insert(
                        "required".to_string(),
                        serde_json::to_value(&required).unwrap(),
                    );
                }
                schema.insert("definitions".to_string(), serde_json::Value::Object(definitions));

                serde_json::Value::Object(schema)
            }
        }
    };

    Ok(expanded)
}

/// Generate JSON Schema expression for a field type.
/// Returns (schema_expr, nested_type_definitions).
/// Each nested definition is (type_name, definition_schema_expr).
///
/// Classification is delegated to [`json_type_kind`] so the primitive/custom
/// decision table lives in one place shared with `ToolDef`; this function only
/// renders the richer shapes (`items` for collections, definitions for nested
/// custom types).
/// `Option<T>` is transparent for rendering: `Option<Vec<T>>` is an array of T, so the unwrapped
/// type owns the generic arguments.
///
/// Nested struct — generate $ref and a placeholder definition.
///
fn field_type_schema(ty: &syn::Type) -> syn::Result<(TokenStream, Vec<(String, TokenStream)>)> {
    let target = unwrap_option_type(ty);
    match json_type_kind(target) {
        JsonTypeKind::Primitive(keyword) => {
            Ok((quote! { serde_json::json!({"type": #keyword}) }, Vec::new()))
        }
        JsonTypeKind::Array => {
            if let syn::Type::Path(type_path) = target
                && let Some(segment) = type_path.path.segments.last()
                && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
            {
                let (items_schema, nested) = field_type_schema(inner)?;
                Ok((
                    quote! { serde_json::json!({"type": "array", "items": #items_schema}) },
                    nested,
                ))
            } else {
                Ok((quote! { serde_json::json!({"type": "array"}) }, Vec::new()))
            }
        }
        JsonTypeKind::Custom(type_name) => {
            let ref_str = format!("#/definitions/{}", type_name);
            let schema = quote! { serde_json::json!({"$ref": #ref_str}) };
            let definition = quote! { serde_json::json!({"type": "object"}) };
            Ok((schema, vec![(type_name.clone(), definition)]))
        }
    }
}
