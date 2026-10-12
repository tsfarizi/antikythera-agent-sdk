//! FsmComplete derive: compile-time FSM transition completeness validation.
//!
//! Ensures every state has a defined transition to all other required states,
//! preventing runtime errors from missing state machine paths.
//!
//! Beyond the compile-time assertions, generates an associated const
//! `TRANSITION_MATRIX: &'static [(&'static str, &'static [&'static str])]`
//! on the enum so the matrix is comparable as data (e.g. parity tests).

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput};

/// Collect enum variant names in declaration order.
///
/// Parses the #`fsm_transitions(...)` attribute, preserving attribute order so the generated
/// const is deterministic (variant order, not map order).
///
/// Validate: all states in the matrix exist in the enum.
///
/// Validate: all enum variants appear in the transition matrix.
///
/// Validate: every state has at least one outgoing transition.
///
/// Generate a const block with compile-time assertions.
///
/// Generate the associated const exposing the matrix in variant order.
///
pub(crate) fn impl_fsm_complete(input: &DeriveInput) -> syn::Result<TokenStream> {
    let enum_name = &input.ident;

    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "FsmComplete can only be derived on enums",
            ));
        }
    };

    let enum_variants: Vec<String> = variants.iter().map(|v| v.ident.to_string()).collect();

    let transitions = parse_fsm_transitions(input)?;

    let mut transition_matrix = std::collections::HashMap::new();
    for (source, targets) in &transitions {
        transition_matrix.insert(source.clone(), targets.clone());
    }

    for source_state in transition_matrix.keys() {
        if !enum_variants.contains(source_state) {
            return Err(syn::Error::new_spanned(
                input,
                format!(
                    "FSM transition matrix references unknown state '{}'",
                    source_state
                ),
            ));
        }
        for target_state in transition_matrix.get(source_state).unwrap() {
            if !enum_variants.contains(target_state) {
                return Err(syn::Error::new_spanned(
                    input,
                    format!(
                        "FSM transition matrix references unknown state '{}'",
                        target_state
                    ),
                ));
            }
        }
    }

    for variant in &enum_variants {
        if !transition_matrix.contains_key(variant) {
            return Err(syn::Error::new_spanned(
                input,
                format!(
                    "FSM state '{}' has no outgoing transitions defined",
                    variant
                ),
            ));
        }
    }

    for (source, targets) in &transition_matrix {
        if targets.is_empty() {
            return Err(syn::Error::new_spanned(
                input,
                format!("FSM state '{}' has no outgoing transitions", source),
            ));
        }
    }

    let assertions = generate_fsm_assertions(enum_name, &enum_variants, &transition_matrix);

    let matrix_const = generate_transition_matrix(enum_name, &enum_variants, &transition_matrix);

    let expanded = quote! {
        const _: () = {
            #assertions
        };
        #matrix_const
    };

    Ok(expanded)
}

/// A single transition: source state => list of target states.
struct FsmTransition {
    source: syn::Ident,
    targets: Vec<syn::Ident>,
}

impl syn::parse::Parse for FsmTransition {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let source: syn::Ident = input.parse()?;
        let _arrow: syn::Token![=>] = input.parse()?;
        let content;
        syn::bracketed!(content in input);
        let targets = content.parse_terminated(syn::Ident::parse, syn::Token![,])?;
        Ok(FsmTransition {
            source,
            targets: targets.into_iter().collect(),
        })
    }
}

struct FsmTransitionsAttr {
    transitions: Vec<FsmTransition>,
}

impl syn::parse::Parse for FsmTransitionsAttr {
    /// Optionally consume a trailing comma.
    ///
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut transitions = Vec::new();
        while !input.is_empty() {
            transitions.push(input.parse::<FsmTransition>()?);
            if input.peek(syn::Token![,]) {
                let _: syn::Token![,] = input.parse()?;
            }
        }
        Ok(FsmTransitionsAttr { transitions })
    }
}

/// Parse the `#[fsm_transitions(...)]` attribute and return an ordered list of
/// (source, targets) pairs in the order they appear in the attribute.
fn parse_fsm_transitions(input: &DeriveInput) -> syn::Result<Vec<(String, Vec<String>)>> {
    let mut attr_iter = input
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("fsm_transitions"));

    let attr = attr_iter.next().ok_or_else(|| {
        syn::Error::new_spanned(
            input,
            "FsmComplete requires #[fsm_transitions(...)] attribute",
        )
    })?;

    if attr_iter.next().is_some() {
        return Err(syn::Error::new_spanned(
            input,
            "FsmComplete allows only one #[fsm_transitions(...)] attribute",
        ));
    }

    let parsed: FsmTransitionsAttr = attr.parse_args()?;

    let mut transitions = Vec::new();
    for transition in &parsed.transitions {
        let source = transition.source.to_string();
        let targets: Vec<String> = transition.targets.iter().map(|t| t.to_string()).collect();
        transitions.push((source, targets));
    }

    if transitions.is_empty() {
        return Err(syn::Error::new_spanned(
            input,
            "FsmComplete requires at least one transition in #[fsm_transitions(...)]",
        ));
    }

    Ok(transitions)
}

/// Generate an associated const exposing the transition matrix.
///
/// Source states are ordered by enum variant declaration order (deterministic,
/// independent of any hash-map iteration). Target lists keep attribute order.
/// Validation above guarantees every variant is a matrix source with at least one target, so the
/// lookup cannot miss.
///
fn generate_transition_matrix(
    enum_name: &syn::Ident,
    enum_variants: &[String],
    transition_matrix: &std::collections::HashMap<String, Vec<String>>,
) -> TokenStream {
    let entries: Vec<TokenStream> = enum_variants
        .iter()
        .map(|variant| {
            let targets = &transition_matrix[variant];
            let target_lits: Vec<&str> = targets.iter().map(|t| t.as_str()).collect();
            quote! {
                (#variant, &[#(#target_lits),*])
            }
        })
        .collect();

    quote! {
        impl #enum_name {
            /// Transition matrix in enum variant declaration order:
            /// `(source state, [target states])`.
            pub const TRANSITION_MATRIX: &'static [(&'static str, &'static [&'static str])] = &[
                #(#entries),*
            ];
        }
    }
}

/// Generate compile-time assertions for FSM validation.
/// Builds a set of all states that are targets of transitions (reachable states).
///
/// Generate error messages for unreachable states (except the first state which is the initial
/// state).
///
/// Generate error messages for states with no outgoing transitions.
///
/// Generate error messages for states in matrix but not in enum.
///
/// Verify that the enum type exists (ensures the macro is used on a valid type).
///
fn generate_fsm_assertions(
    enum_name: &syn::Ident,
    enum_variants: &[String],
    transition_matrix: &std::collections::HashMap<String, Vec<String>>,
) -> TokenStream {
    let mut reachable = std::collections::HashSet::new();
    for targets in transition_matrix.values() {
        for target in targets {
            reachable.insert(target.clone());
        }
    }

    let mut unreachable_errors = Vec::new();
    if let Some(first_variant) = enum_variants.first() {
        for variant in enum_variants {
            if variant != first_variant && !reachable.contains(variant) {
                let msg = format!(
                    "FSM state '{}' is not reachable from any other state",
                    variant
                );
                unreachable_errors.push(quote! {
                    compile_error!(#msg);
                });
            }
        }
    }

    let mut no_outgoing_errors = Vec::new();
    for variant in enum_variants {
        if !transition_matrix.contains_key(variant) {
            let msg = format!(
                "FSM state '{}' has no outgoing transitions defined",
                variant
            );
            no_outgoing_errors.push(quote! {
                compile_error!(#msg);
            });
        } else if transition_matrix.get(variant).unwrap().is_empty() {
            let msg = format!("FSM state '{}' has no outgoing transitions", variant);
            no_outgoing_errors.push(quote! {
                compile_error!(#msg);
            });
        }
    }

    let mut unknown_state_errors = Vec::new();
    for source in transition_matrix.keys() {
        if !enum_variants.contains(source) {
            let msg = format!(
                "FSM transition matrix references unknown state '{}'",
                source
            );
            unknown_state_errors.push(quote! {
                compile_error!(#msg);
            });
        }
    }
    for targets in transition_matrix.values() {
        for target in targets {
            if !enum_variants.contains(target) {
                let msg = format!(
                    "FSM transition matrix references unknown state '{}'",
                    target
                );
                unknown_state_errors.push(quote! {
                    compile_error!(#msg);
                });
            }
        }
    }

    quote! {
        #( #unreachable_errors )*
        #( #no_outgoing_errors )*
        #( #unknown_state_errors )*

        let _: #enum_name;
    }
}
