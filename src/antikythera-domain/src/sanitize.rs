//! Text sanitization utilities for TOML-safe strings.

/// Characters that are problematic in TOML basic strings.
const TOML_UNSAFE_CHARS: &[char] = &['\n', '\r', '\t', '\\', '"'];

/// Sanitize a string to be TOML-safe for use in basic strings.
///
/// This function:
/// - Removes newlines and replaces with spaces
/// - Removes emojis and special Unicode characters
/// - Escapes backslashes and quotes
/// - Trims excess whitespace
///
/// Keeps common non-ASCII letters (Indonesian uses standard Latin).
///
/// Removes emojis and other special characters, replace with space.
///
pub fn sanitize_for_toml(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut prev_was_space = false;

    for c in input.chars() {
        match c {
            '\n' | '\r' | '\t' => {
                if !prev_was_space {
                    result.push(' ');
                    prev_was_space = true;
                }
            }
            '\\' => {
                result.push_str("\\\\");
                prev_was_space = false;
            }
            '"' => {
                result.push_str("\\\"");
                prev_was_space = false;
            }
            c if c.is_ascii() && !c.is_ascii_control() => {
                if c == ' ' {
                    if !prev_was_space {
                        result.push(c);
                        prev_was_space = true;
                    }
                } else {
                    result.push(c);
                    prev_was_space = false;
                }
            }
            c if c.is_alphabetic() => {
                result.push(c);
                prev_was_space = false;
            }
            _ => {
                if !prev_was_space {
                    result.push(' ');
                    prev_was_space = true;
                }
            }
        }
    }

    result.trim().to_string()
}

/// Check if a string contains characters that would break TOML basic string parsing.
pub fn needs_sanitization(input: &str) -> bool {
    input
        .chars()
        .any(|c| TOML_UNSAFE_CHARS.contains(&c) || c.is_control() || is_emoji(c))
}

/// Identify symbols, pictographs, dingbats, variation selectors, and the
/// zero-width joiner that require sanitization in TOML-bound text.
fn is_emoji(c: char) -> bool {
    let code = c as u32;
    matches!(code,
        0x1F300..=0x1F9FF |
        0x2600..=0x26FF |
        0x2700..=0x27BF |
        0xFE00..=0xFE0F |
        0x200D
    )
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/sanitize.tests.rs"]
mod tests;
