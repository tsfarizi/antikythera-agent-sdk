//! WIT Conformance Validator
//!
//! Validates that the checked-in `wit/antikythera.wit` file remains in sync
//! with the Rust source types that it represents. Reports drift between WIT
//! record fields and Rust struct fields, and between WIT interface functions
//! and Rust public function signatures.
//!
//! Exit 0: conformant (no drift)
//! Exit 1: drift detected or WIT file missing/empty
//!
//! Usage: `cargo run -p build-scripts --release -- validate`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Interfaces whose implementation lives outside the scanned crates (e.g.
/// `plugin/antikythera-toolrunner`, host-authored hooks components, host-supplied runtime-hooks
/// wired at runtime) are not subject to this conformance check; their drift is caught by the real
/// wit-parser during `cargo component build`.
///
const SKIPPED_INTERFACES: &[&str] = &["tool-registry", "logic-hooks", "runtime-hooks"];

#[derive(Debug, Clone)]
struct WitRecord {
    name: String,
    fields: Vec<WitField>,
}

#[derive(Debug, Clone)]
struct WitField {
    name: String,
    wit_type: String,
}

#[derive(Debug, Clone)]
struct WitFunction {
    name: String,
    params: Vec<WitField>,
    return_type: Option<String>,
}

#[derive(Debug, Clone)]
struct WitInterface {
    name: String,
    functions: Vec<WitFunction>,
}

#[derive(Debug, Clone)]
struct RustStruct {
    name: String,
    fields: Vec<RustField>,
}

#[derive(Debug, Clone)]
struct RustField {
    name: String,
    rust_type: String,
}

#[derive(Debug, Clone)]
struct RustFunction {
    name: String,
    params: Vec<RustField>,
    return_type: Option<String>,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match args.as_slice() {
        ["validate"] => run_validate(),
        _ => {
            eprintln!("{BOLD}WIT Conformance Validator{RESET}\n");
            eprintln!("Usage: component-builder validate");
            eprintln!("  validate   Check WIT file against Rust source types");
            exit(1);
        }
    }
}

fn run_validate() {
    println!("{BOLD}=== WIT Conformance Validator ==={RESET}\n");

    let root = project_root();
    let wit_path = root.join("wit").join("antikythera.wit");

    if !wit_path.exists() {
        eprintln!("{RED}✗ WIT file not found: {}{RESET}", wit_path.display());
        exit(1);
    }

    let wit_content = match fs::read_to_string(&wit_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}✗ Failed to read WIT file: {e}{RESET}");
            exit(1);
        }
    };

    if wit_content.trim().is_empty() {
        eprintln!("{RED}✗ WIT file is empty{RESET}");
        exit(1);
    }

    println!("{YELLOW}Reading WIT file:{RESET} {}", wit_path.display());

    let wit_records = parse_wit_records(&wit_content);
    let wit_interfaces = parse_wit_interfaces(&wit_content);

    println!(
        "  Found {} record(s), {} interface(s)\n",
        wit_records.len(),
        wit_interfaces.len()
    );

    let sdk_src = root.join("antikythera-sdk").join("src");
    let core_src = root.join("antikythera-core").join("src");

    let rust_structs = scan_rust_structs(&[&sdk_src, &core_src]);
    let rust_fns = scan_rust_functions(&[&sdk_src, &core_src]);

    println!(
        "{YELLOW}Scanned Rust sources:{RESET} {} struct(s), {} function(s)\n",
        rust_structs.len(),
        rust_fns.len()
    );

    let mut drift_count: usize = 0;

    println!("{BOLD}--- Record Validation ---{RESET}");
    for wit_rec in &wit_records {
        let rust_name = kebab_to_camel(&wit_rec.name);
        match rust_structs.iter().find(|s| s.name == rust_name) {
            Some(rust_struct) => {
                let diffs = compare_record_fields(wit_rec, rust_struct);
                if diffs.is_empty() {
                    println!("  {GREEN}✓{RESET} {} — conformant", wit_rec.name);
                } else {
                    println!("  {RED}✗{RESET} {} — DRIFT", wit_rec.name);
                    for d in &diffs {
                        eprintln!("    {RED}•{RESET} {d}");
                    }
                    drift_count += 1;
                }
            }
            None => {
                println!(
                    "  {YELLOW}○{RESET} {} — no matching Rust struct found (skipped)",
                    wit_rec.name
                );
            }
        }
    }
    println!();

    println!("{BOLD}--- Interface Validation ---{RESET}");
    for wit_iface in &wit_interfaces {
        if SKIPPED_INTERFACES.contains(&wit_iface.name.as_str()) {
            println!(
                "  {YELLOW}○{RESET} {} - implementation outside scanned crates (skipped)",
                wit_iface.name
            );
            continue;
        }

        let expected_fns: Vec<&str> = wit_iface
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        let matching_fns: Vec<&RustFunction> = rust_fns
            .iter()
            .filter(|f| expected_fns.contains(&snake_to_kebab(&f.name).as_str()))
            .collect();

        if matching_fns.is_empty() {
            println!(
                "  {YELLOW}○{RESET} {} — no matching Rust functions found (skipped)",
                wit_iface.name
            );
            continue;
        }

        let mut iface_drift = false;
        for wit_fn in &wit_iface.functions {
            match matching_fns
                .iter()
                .find(|f| snake_to_kebab(&f.name) == wit_fn.name)
            {
                Some(rust_fn) => {
                    let diffs = compare_function(wit_fn, rust_fn);
                    if diffs.is_empty() {
                        println!(
                            "  {GREEN}✓{RESET} {}.{} — conformant",
                            wit_iface.name, wit_fn.name
                        );
                    } else {
                        println!("  {RED}✗{RESET} {}.{} — DRIFT", wit_iface.name, wit_fn.name);
                        for d in &diffs {
                            eprintln!("    {RED}•{RESET} {d}");
                        }
                        drift_count += 1;
                        iface_drift = true;
                    }
                }
                None => {
                    println!(
                        "  {YELLOW}○{RESET} {}.{} — no matching Rust function (skipped)",
                        wit_iface.name, wit_fn.name
                    );
                }
            }
        }
        if !iface_drift {}
    }
    println!();

    if drift_count == 0 {
        println!("{GREEN}{BOLD}✓ WIT file is conformant with Rust sources.{RESET}");
        exit(0);
    } else {
        println!("{RED}{BOLD}✗ Drift detected: {drift_count} issue(s).{RESET}",);
        exit(1);
    }
}

fn parse_wit_records(content: &str) -> Vec<WitRecord> {
    let mut records = Vec::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("record ") && trimmed.contains('{') {
            let name = trimmed
                .trim_start_matches("record ")
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();
            let mut fields = Vec::new();

            for field_line in lines.by_ref() {
                let ft = field_line.trim();
                if ft == "}" {
                    break;
                }
                if ft.is_empty() || ft.starts_with("//") || ft.starts_with("///") {
                    continue;
                }
                if let Some(colon) = ft.find(':') {
                    let fname = ft[..colon].trim().to_string();
                    let ftype = ft[colon + 1..]
                        .trim()
                        .trim_end_matches(',')
                        .trim()
                        .to_string();
                    fields.push(WitField {
                        name: fname,
                        wit_type: ftype,
                    });
                }
            }

            records.push(WitRecord { name, fields });
        }
    }

    records
}

fn parse_wit_interfaces(content: &str) -> Vec<WitInterface> {
    let mut interfaces = Vec::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("interface ") && trimmed.contains('{') {
            let name = trimmed
                .trim_start_matches("interface ")
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();
            let mut functions = Vec::new();

            for fn_line in lines.by_ref() {
                let ft = fn_line.trim();
                if ft == "}" {
                    break;
                }
                if ft.is_empty() || ft.starts_with("//") || ft.starts_with("///") {
                    continue;
                }
                if let Some(func) = parse_wit_function_sig(ft) {
                    functions.push(func);
                }
            }

            interfaces.push(WitInterface { name, functions });
        }
    }

    interfaces
}

/// Canonical WIT declares functions as `name: func(params) -> ret;`; normalize that form to the
/// legacy `name(params)` shape so the rest of the parser is unchanged.
///
fn parse_wit_function_sig(line: &str) -> Option<WitFunction> {
    let line = line.trim().trim_end_matches(';');
    let line = line.replace(": func(", "(");
    let paren_open = line.find('(')?;
    let name = line[..paren_open].trim().to_string();
    let after_paren = &line[paren_open + 1..];
    let paren_close = after_paren.find(')')?;
    let params_str = &after_paren[..paren_close];
    let rest = after_paren[paren_close + 1..].trim();

    let params = parse_wit_params(params_str);

    let return_type = rest
        .strip_prefix("->")
        .map(|stripped| stripped.trim().to_string());

    Some(WitFunction {
        name,
        params,
        return_type,
    })
}

fn parse_wit_params(params_str: &str) -> Vec<WitField> {
    if params_str.trim().is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    for param in params_str.split(',') {
        let param = param.trim();
        if let Some(colon) = param.find(':') {
            let pname = param[..colon].trim().to_string();
            let ptype = param[colon + 1..].trim().to_string();
            result.push(WitField {
                name: pname,
                wit_type: ptype,
            });
        }
    }
    result
}

fn scan_rust_structs(dirs: &[&Path]) -> Vec<RustStruct> {
    let mut structs = Vec::new();
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        scan_structs_recursive(dir, &mut structs);
    }
    structs
}

fn scan_structs_recursive(dir: &Path, structs: &mut Vec<RustStruct>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && (name == "target" || name == ".git" || name == "node_modules")
                {
                    continue;
                }
                scan_structs_recursive(&path, structs);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                parse_rust_structs(&content, structs);
            }
        }
    }
}

fn parse_rust_structs(content: &str, structs: &mut Vec<RustStruct>) {
    let mut pos = 0;
    while pos < content.len() {
        if let Some(struct_start) = content[pos..].find("pub struct ") {
            let struct_start = pos + struct_start;
            let name_start = struct_start + "pub struct ".len();
            if let Some(brace_pos) = content[name_start..].find('{') {
                let struct_name = content[name_start..name_start + brace_pos]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();
                let brace_start = name_start + brace_pos;

                if let Some((brace_end, fields_content)) =
                    find_matching_brace(&content[brace_start..])
                {
                    let fields_content = &fields_content[..fields_content.len() - 1];
                    let fields = parse_rust_fields(fields_content);
                    structs.push(RustStruct {
                        name: struct_name,
                        fields,
                    });
                    pos = brace_start + brace_end;
                } else {
                    pos = name_start + 1;
                }
            } else {
                pos = name_start + 1;
            }
        } else {
            break;
        }
    }
}

fn parse_rust_fields(content: &str) -> Vec<RustField> {
    let mut fields = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        let trimmed = trimmed
            .strip_prefix("pub(crate)")
            .or_else(|| trimmed.strip_prefix("pub"))
            .unwrap_or(trimmed)
            .trim();

        if let Some(colon_pos) = trimmed.find(':') {
            let fname = trimmed[..colon_pos].trim().to_string();
            let ftype = trimmed[colon_pos + 1..].trim().to_string();
            if !fname.is_empty() && !fname.starts_with('#') {
                fields.push(RustField {
                    name: fname,
                    rust_type: ftype,
                });
            }
        }
    }
    fields
}

fn scan_rust_functions(dirs: &[&Path]) -> Vec<RustFunction> {
    let mut fns = Vec::new();
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        scan_fns_recursive(dir, &mut fns);
    }
    fns
}

fn scan_fns_recursive(dir: &Path, fns: &mut Vec<RustFunction>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && (name == "target" || name == ".git" || name == "node_modules")
                {
                    continue;
                }
                scan_fns_recursive(&path, fns);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                parse_rust_functions(&content, fns);
            }
        }
    }
}

fn parse_rust_functions(content: &str, fns: &mut Vec<RustFunction>) {
    let mut pos = 0;
    while let Some(rel) = content[pos..].find("pub fn ") {
        let start = pos + rel;
        let after_pub = &content[start + "pub fn ".len()..];
        let Some(paren_rel) = after_pub.find('(') else {
            pos = start + 1;
            continue;
        };
        let fn_name = after_pub[..paren_rel].trim().to_string();
        let body = &after_pub[paren_rel..];
        let Some((close_offset, params_str)) = find_fn_params(body) else {
            pos = start + 1;
            continue;
        };
        let params = parse_rust_params(params_str);

        let rest = body[close_offset..].trim();
        let return_type = rest.strip_prefix("->").map(|stripped| {
            stripped
                .trim()
                .split_once('{')
                .map(|(head, _)| head)
                .unwrap_or(stripped.trim())
                .trim_end_matches(';')
                .trim()
                .to_string()
        });

        fns.push(RustFunction {
            name: fn_name,
            params,
            return_type,
        });
        pos = start + 1;
    }
}

fn find_fn_params(body: &str) -> Option<(usize, &str)> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '"' => in_string = !in_string,
            '\\' if in_string => escaped = true,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some((i + 1, &body[1..i]));
                }
            }
            _ => {}
        }
    }
    None
}
fn parse_rust_params(params_str: &str) -> Vec<RustField> {
    if params_str.trim().is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();

    for c in params_str.chars() {
        match c {
            '<' | '(' => {
                depth += 1;
                current.push(c);
            }
            '>' | ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                if let Some(field) = parse_single_rust_param(&current) {
                    result.push(field);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty()
        && let Some(field) = parse_single_rust_param(&current)
    {
        result.push(field);
    }
    result
}

fn parse_single_rust_param(param: &str) -> Option<RustField> {
    let param = param.trim();
    if param.is_empty() || param == "self" || param == "&self" || param == "&mut self" {
        return None;
    }
    if let Some(colon_pos) = param.find(':') {
        let pname = param[..colon_pos].trim().to_string();
        let ptype = param[colon_pos + 1..].trim().to_string();
        Some(RustField {
            name: pname,
            rust_type: ptype,
        })
    } else {
        None
    }
}

/// Checks each WIT field exists in Rust struct.
///
/// Checks for extra Rust fields not in WIT.
///
fn compare_record_fields(wit: &WitRecord, rust: &RustStruct) -> Vec<String> {
    let mut diffs = Vec::new();

    for wit_field in &wit.fields {
        let rust_field_name = kebab_to_snake(&wit_field.name);
        match rust.fields.iter().find(|f| f.name == rust_field_name) {
            Some(rust_field) => {
                let wit_as_rust = wit_type_to_rust(&wit_field.wit_type);
                if !type_equivalent(&wit_as_rust, &rust_field.rust_type) {
                    diffs.push(format!(
                        "field '{}': WIT type '{}' ≠ Rust type '{}'",
                        wit_field.name, wit_field.wit_type, rust_field.rust_type
                    ));
                }
            }
            None => {
                diffs.push(format!(
                    "field '{}' exists in WIT but not in Rust struct",
                    wit_field.name
                ));
            }
        }
    }

    for rust_field in &rust.fields {
        let wit_name = snake_to_kebab(&rust_field.name);
        if !wit.fields.iter().any(|f| f.name == wit_name) {
            diffs.push(format!(
                "field '{}' exists in Rust struct but not in WIT (extra)",
                rust_field.name
            ));
        }
    }

    diffs
}

/// Runner functions report `AgentRunnerError` in Rust but the WIT boundary flattens it to
/// `string` (`e.to_string()` in the export shim); treat the two as equivalent for conformance.
///
fn compare_function(wit: &WitFunction, rust: &RustFunction) -> Vec<String> {
    let mut diffs = Vec::new();

    if wit.params.len() != rust.params.len() {
        diffs.push(format!(
            "parameter count: WIT has {}, Rust has {}",
            wit.params.len(),
            rust.params.len()
        ));
    }

    match (&wit.return_type, &rust.return_type) {
        (Some(wit_ret), Some(rust_ret)) => {
            let wit_as_rust = wit_type_to_rust(wit_ret);
            let rust_normalized = rust_ret.replace("AgentRunnerError", "String");
            if !type_equivalent(&wit_as_rust, &rust_normalized) {
                diffs.push(format!(
                    "return type: WIT '{}' ≠ Rust '{}'",
                    wit_ret, rust_ret
                ));
            }
        }
        (None, None) => {}
        _ => {
            diffs.push(format!(
                "return type: WIT={:?} Rust={:?}",
                wit.return_type, rust.return_type
            ));
        }
    }

    diffs
}

fn wit_type_to_rust(wit: &str) -> String {
    match wit {
        "string" => "String".to_string(),
        "bool" => "bool".to_string(),
        "u8" => "u8".to_string(),
        "u16" => "u16".to_string(),
        "u32" => "u32".to_string(),
        "u64" => "u64".to_string(),
        "s8" => "i8".to_string(),
        "s16" => "i16".to_string(),
        "s32" => "i32".to_string(),
        "s64" => "i64".to_string(),
        "float32" => "f32".to_string(),
        "float64" => "f64".to_string(),
        t if t.starts_with("option<") => {
            let inner = &t[7..t.len() - 1];
            format!("Option<{}>", wit_type_to_rust(inner))
        }
        t if t.starts_with("list<") => {
            let inner = &t[5..t.len() - 1];
            format!("Vec<{}>", wit_type_to_rust(inner))
        }
        t if t.starts_with("result<") => {
            let inner = &t[7..t.len() - 1];
            let (ok, err) = inner
                .split_once(',')
                .map(|(ok, err)| (ok.trim(), err.trim()))
                .unwrap_or((inner.trim(), ""));
            let ok_rust = if ok == "_" {
                "_".to_string()
            } else {
                wit_type_to_rust(ok)
            };
            format!("Result<{ok_rust}, {}>", wit_type_to_rust(err))
        }
        _ => kebab_to_camel(wit),
    }
}

fn type_equivalent(rust_a: &str, rust_b: &str) -> bool {
    let normalize = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_lowercase()
    };
    normalize(rust_a) == normalize(rust_b)
}

fn kebab_to_camel(input: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in input.chars() {
        if c == '-' || c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn kebab_to_snake(input: &str) -> String {
    input.replace('-', "_")
}

fn snake_to_kebab(input: &str) -> String {
    input.replace('_', "-")
}

fn find_matching_brace(content: &str) -> Option<(usize, String)> {
    if !content.starts_with('{') {
        return None;
    }
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (i, c) in content.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some((i + 1, content[..i + 1].to_string()));
                }
            }
            _ => {}
        }
    }
    None
}

fn project_root() -> PathBuf {
    let mut current = env::current_exe()
        .expect("Failed to get current exe path")
        .parent()
        .expect("Failed to get parent directory")
        .to_path_buf();

    while current.parent().is_some() {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml).expect("Failed to read Cargo.toml");
            if content.contains("[workspace]") {
                return current;
            }
        }
        current = current.parent().unwrap().to_path_buf();
    }

    env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().expect("Failed to get current directory"))
}
