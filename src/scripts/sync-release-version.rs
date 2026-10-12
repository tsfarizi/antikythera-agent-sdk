//! Release version synchronizer.
//!
//! Reads the version from `[workspace.package]` in the workspace root
//! `Cargo.toml` (single source of truth) and rewrites every package version
//! that must follow it:
//!
//!   - clients/npm/antikythera-sdk/package.json       ->  "version": "VERSION"
//!   - clients/python/pyproject.toml                  ->  version = "VERSION"
//!   - clients/python/antikythera_agent/__init__.py   ->  __version__ = "VERSION"
//!   - clients/python/antikythera_agent/utils.py      ->  return "VERSION"
//!   - clients/npm/antikythera-sdk/index.js            ->  return "VERSION";  (inside
//!     function getVersion)
//!
//! Modes:
//!   (default)  rewrite the target files to the Cargo workspace version
//!   --check    verify all targets carry the Cargo version; exit 1 on drift,
//!              write nothing
//!   --print    print the Cargo workspace version and exit
//!   --help     usage
//!
//! Every target must contain its version marker. Missing files or markers
//! fail validation before any file is written, so a successful check verifies
//! every client version rather than accepting partially initialized targets.
//!
//! Stdlib only (no external dependencies), matching the rest of the
//! build-scripts crate. Line-based: it locates the version-bearing line per
//! target and replaces the version token, preserving the rest of the file
//! byte-for-byte (including line endings).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

struct Target {
    /// Human-readable label for messages.
    label: &'static str,
    /// Path relative to the workspace root.
    rel_path: &'static str,
    /// Line marker that identifies the version-bearing line.
    marker: &'static str,
}

const TARGETS: &[Target] = &[
    Target {
        label: "npm package.json",
        rel_path: "clients/npm/antikythera-sdk/package.json",
        marker: "\"version\"",
    },
    Target {
        label: "python pyproject.toml",
        rel_path: "clients/python/pyproject.toml",
        marker: "version = \"",
    },
    Target {
        label: "python __init__.py",
        rel_path: "clients/python/antikythera_agent/__init__.py",
        marker: "__version__",
    },
    Target {
        label: "python utils.py",
        rel_path: "clients/python/antikythera_agent/utils.py",
        marker: "return \"",
    },
    Target {
        label: "npm index.js getVersion",
        rel_path: "clients/npm/antikythera-sdk/index.js",
        marker: "return \"",
    },
];

/// Read + rewrite every target in memory first; write only when all rewrites succeeded (no
/// partial updates).
///
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = match args.as_slice() {
        [] => Mode::Write,
        [flag] if flag == "--check" => Mode::Check,
        [flag] if flag == "--print" => Mode::Print,
        [flag] if flag == "--help" || flag == "-h" => {
            print_usage();
            return;
        }
        _ => {
            eprintln!("unknown arguments: {args:?}");
            print_usage();
            exit(2);
        }
    };

    let root = project_root();
    let version = match read_workspace_version(&root.join("Cargo.toml")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            exit(1);
        }
    };

    if mode == Mode::Print {
        println!("{version}");
        return;
    }

    let mut rewrites: Vec<(String, String, String)> = Vec::new();
    let mut drift = false;
    for target in TARGETS {
        let path = root.join(target.rel_path);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "error: cannot read {} ({}): {e}",
                    target.label,
                    path.display()
                );
                exit(1);
            }
        };
        let rewritten = match rewrite_version_line(&text, target.marker, &version) {
            Some(r) => r,
            None => {
                eprintln!(
                    "error: no version-bearing line found in {} (marker '{}')",
                    target.label, target.marker
                );
                exit(1);
            }
        };
        if rewritten != text {
            drift = true;
            println!("drift: {} {} -> {version}", target.label, target.rel_path);
        }
        rewrites.push((
            target.label.to_string(),
            path.display().to_string(),
            rewritten,
        ));
    }

    if mode == Mode::Check {
        if drift {
            eprintln!(
                "FAIL: version drift detected — run `task sync-version` or `cargo run -p build-scripts --bin sync-release-version`"
            );
            exit(1);
        }
        println!("OK: npm and python versions match Cargo workspace version {version}");
        return;
    }

    if !drift {
        println!("OK: all targets already at Cargo workspace version {version}");
        return;
    }

    for (label, path, new_text) in &rewrites {
        if let Err(e) = fs::write(path, new_text) {
            eprintln!("error: cannot write {} ({}): {e}", label, path);
            exit(1);
        }
        println!("updated: {label}");
    }
    println!("synced npm and python versions to Cargo workspace version {version}");
}

#[derive(PartialEq)]
enum Mode {
    Write,
    Check,
    Print,
}

fn print_usage() {
    eprintln!(
        "sync-release-version\n\
         \n\
         Syncs npm and python package versions to the Cargo workspace version.\n\
         \n\
         Usage: sync-release-version [--check | --print | --help]\n\
         \n\
         (no flag)  rewrite clients/npm/package.json + python pyproject/__init__/utils\n\
          --check    verify every target matches the Cargo version (no writes)\n\
         --print    print the Cargo workspace version\n\
         --help     this help"
    );
}

/// Reads the `version` declared under `[workspace.package]`.
/// Next section: no version found.
///
fn read_workspace_version(cargo_toml: &Path) -> Result<String, String> {
    let content = fs::read_to_string(cargo_toml)
        .map_err(|e| format!("cannot read {}: {e}", cargo_toml.display()))?;
    let mut in_workspace_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace.package]" {
            in_workspace_package = true;
            continue;
        }
        if in_workspace_package {
            if trimmed.starts_with('[') {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim_start();
                if let Some(v) = rest.strip_prefix('=') {
                    let v = v.trim();
                    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
                        let version = &v[1..v.len() - 1];
                        if is_valid_version(version) {
                            return Ok(version.to_string());
                        }
                        return Err(format!(
                            "invalid version '{}' in {}",
                            version,
                            cargo_toml.display()
                        ));
                    }
                }
            }
        }
    }
    Err(format!(
        "could not read version from [workspace.package] in {}",
        cargo_toml.display()
    ))
}

/// Require at least `d+.d+.d+` before any optional suffix.
///
/// Optional pre-release/build suffix: `-` or `+` followed by `0-9A-Za-z.-`.
///
fn is_valid_version(version: &str) -> bool {
    let bytes = version.as_bytes();
    let mut digits = 0;
    let mut dots = 0;
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if digits == 0 {
        return false;
    }
    while i < bytes.len()
        && bytes[i] == b'.'
        && i + 1 < bytes.len()
        && bytes[i + 1].is_ascii_digit()
    {
        dots += 1;
        i += 1;
        let mut d = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            d += 1;
        }
        if d == 0 {
            return false;
        }
    }
    if dots < 2 {
        return false;
    }
    if i < bytes.len() {
        if bytes[i] != b'-' && bytes[i] != b'+' {
            return false;
        }
        i += 1;
        if i >= bytes.len() {
            return false;
        }
        while i < bytes.len() {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
                return false;
            }
            i += 1;
        }
    }
    i == bytes.len()
}

/// Replaces the version token on the first line containing `marker`.
///
/// The version token is the first run of `[0-9A-Za-z.+-]` starting at a
/// digit on that line; it must contain a dot to be treated as a version.
/// Returns `None` if no version-like token is found.
fn rewrite_version_line(text: &str, marker: &str, version: &str) -> Option<String> {
    let mut found = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if !found && line.contains(marker) {
            out.push_str(&replace_version_token(line, version)?);
            found = true;
        } else {
            out.push_str(line);
        }
    }
    if found { Some(out) } else { None }
}

fn replace_version_token(line: &str, version: &str) -> Option<String> {
    let start = line.char_indices().find(|(_, c)| c.is_ascii_digit())?.0;
    let bytes = line.as_bytes();
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'.' | b'-' | b'+'))
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    let token = &line[start..end];
    if !token.contains('.') {
        return None;
    }
    Some(format!("{}{}{}", &line[..start], version, &line[end..]))
}

/// Walks up from the running binary to the workspace root (a directory
/// containing a `Cargo.toml` with a `[workspace]` section).
fn project_root() -> PathBuf {
    let mut current = env::current_exe()
        .expect("failed to get current exe path")
        .parent()
        .expect("failed to get current exe parent")
        .to_path_buf();

    while current.parent().is_some() {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = fs::read_to_string(&cargo_toml)
            && content.contains("[workspace]")
        {
            return current;
        }
        current = current.parent().unwrap().to_path_buf();
    }

    env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().expect("failed to get current directory"))
}
