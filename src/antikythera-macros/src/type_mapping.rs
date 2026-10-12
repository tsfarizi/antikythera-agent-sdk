//! Shared Rust type → JSON Schema type mapping helpers.
//!
//! Used by `tool_def` (MCP tool schemas) and `json_schema` (draft-07 schemas).
//! Both macros classify field types through [`json_type_kind`], so the same
//! Rust type always produces the same JSON Schema shape in both outputs.

/// How a Rust field type is represented in a JSON Schema.
pub(crate) enum JsonTypeKind {
    /// A primitive JSON Schema type keyword ("string", "integer", "number", "boolean").
    Primitive(&'static str),
    /// A collection (`Vec`, `VecDeque`, `BTreeSet`, `HashSet`, `LinkedList`) — keyword "array".
    Array,
    /// A custom named type — `$ref` to `#/definitions/<name>` plus a placeholder
    /// definition `{"type": "object"}`.
    Custom(String),
}

/// Check if a type is `Option<T>`.
pub(crate) fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident == "Option";
    }
    false
}

/// Classify a Rust type for JSON Schema generation, transparently unwrapping
/// `Option<T>` so the inner type drives the schema.
///
/// Primitive mappings are stable: string/integer/number/boolean, with `array`
/// for collections. Unknown named types (structs, enums, aliases, ...) map to
/// a `$ref` placeholder instead of a lossy "string" default.
/// `Option<T>` is transparent: classify the inner type.
///
/// Malformed `Option` without a type argument.
///
/// Non-path types (tuples, references, function pointers, ...) carry no resolvable name; keep the
/// historical "string" default for them.
///
pub(crate) fn json_type_kind(ty: &syn::Type) -> JsonTypeKind {
    if let syn::Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        let ident = segment.ident.to_string();
        match ident.as_str() {
            "String" | "str" => JsonTypeKind::Primitive("string"),
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize" => {
                JsonTypeKind::Primitive("integer")
            }
            "f32" | "f64" => JsonTypeKind::Primitive("number"),
            "bool" => JsonTypeKind::Primitive("boolean"),
            "Vec" | "VecDeque" | "BTreeSet" | "HashSet" | "LinkedList" => JsonTypeKind::Array,
            "Option" => {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                {
                    return json_type_kind(inner);
                }
                JsonTypeKind::Primitive("string")
            }
            _ => JsonTypeKind::Custom(ident),
        }
    } else {
        JsonTypeKind::Primitive("string")
    }
}

/// If `ty` is `Option<T>`, return the inner type `T`; otherwise return `ty`.
///
/// Rendering code that needs the *generic arguments* of the classified type
/// (e.g. `items` for collections) must operate on the unwrapped type, since
/// `Option<Vec<T>>` is classified as `array` but its own first generic
/// argument is `Vec<T>`, not `T`.
pub(crate) fn unwrap_option_type(ty: &syn::Type) -> &syn::Type {
    if let syn::Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
    {
        return inner;
    }
    ty
}
