//! Shared helpers for parsing attribute literal values and inspecting
//! attribute presence.
//!
//! Deduplicates the `key = "value"` / `key = true` parsing used by the
//! `tool`, `tool_param`, and `bridge` attribute parsers, plus the
//! `#[serde(default)]` detection shared by `ToolDef` and `JsonSchema`.

/// Parse a string literal value from a nested meta (`key = "value"`).
///
/// Fails with a `syn::Error` when the meta has no value or the value is not
/// a string literal — a declared-but-malformed attribute must not silently
/// degrade to a default.
pub(crate) fn parse_meta_string(
    meta: &syn::meta::ParseNestedMeta<'_>,
    attr_name: &str,
) -> syn::Result<String> {
    let value = meta.value()?;
    let lit: syn::Lit = value.parse()?;
    match lit {
        syn::Lit::Str(s) => Ok(s.value()),
        _ => Err(meta.error(format!("expected string literal for `{attr_name}`"))),
    }
}

/// Parse a boolean literal value from a nested meta (`key = true`).
///
/// Fails with a `syn::Error` when the meta has no value or the value is not
/// a boolean literal.
pub(crate) fn parse_meta_bool(
    meta: &syn::meta::ParseNestedMeta<'_>,
    attr_name: &str,
) -> syn::Result<bool> {
    let value = meta.value()?;
    let lit: syn::Lit = value.parse()?;
    match lit {
        syn::Lit::Bool(b) => Ok(b.value()),
        _ => Err(meta.error(format!("expected boolean literal for `{attr_name}`"))),
    }
}

/// Check if a field has the `#[serde(default)]` attribute.
///
/// The `serde` attribute namespace is owned by serde's own derive; this
/// helper only inspects `default` and tolerates every other valid serde
/// form (`rename_all`, `with`, `bound(...)`, ...) by skipping it. A
/// malformed `default` value (`#[serde(default = 123)]`) is a `syn::Error`
/// instead of a silent interpretation.
/// #`serde(default)` or #`serde(default = "path")`.
///
/// Unknown serde meta with a value: consume it so parsing can continue. Its semantics belong to
/// serde's own derive.
///
/// Unknown serde meta with a nested list (e.g. `bound(...)`): consume its content; serde owns its
/// semantics.
///
/// Bare unknown serde metas (`skip`, `flatten`, ...) consume nothing.
///
pub(crate) fn has_serde_default(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        let mut found_default = false;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                found_default = true;
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    let _: syn::LitStr = value.parse()?;
                }
            } else if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let _: proc_macro2::TokenStream = content.parse()?;
            }
            Ok(())
        })?;

        if found_default {
            return Ok(true);
        }
    }

    Ok(false)
}
