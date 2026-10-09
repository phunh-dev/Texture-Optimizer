//! Cross-platform file name validation (strictest of Windows, macOS, Linux).

/// Characters forbidden in file names on at least one supported OS.
pub const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
/// Longest file name (in UTF-8 bytes) accepted by common file systems.
pub const MAX_NAME_BYTES: usize = 255;

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn is_invalid_char(c: char) -> bool {
    INVALID_CHARS.contains(&c) || c.is_control()
}

/// Windows reserves device names regardless of extension (`con.png`).
pub fn is_reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(base))
}

pub fn is_valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && !name.chars().any(is_invalid_char)
        && !name.ends_with('.')
        && !name.ends_with(' ')
        && !is_reserved(name)
}

/// Fixes a name part (without extension): replaces invalid characters,
/// optionally trims trailing dots/spaces (only matters when it ends the file
/// name), suffixes reserved device names and fills an empty result.
pub fn sanitize_stem(stem: &str, replacement: &str, is_last: bool) -> String {
    let mut out: String = stem
        .chars()
        .map(|c| {
            if is_invalid_char(c) {
                replacement.to_owned()
            } else {
                c.to_string()
            }
        })
        .collect();
    if is_last {
        out.truncate(out.trim_end_matches(['.', ' ']).len());
    }
    if is_reserved(&out) {
        let base_len = out.split('.').next().unwrap_or("").len();
        out.insert_str(base_len, replacement);
    }
    if out.is_empty() {
        out = replacement.to_owned();
    }
    out
}
