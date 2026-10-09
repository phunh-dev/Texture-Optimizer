//! Pattern-based batch renaming with conflict detection and revertable logs.
//!
//! [`plan`] is pure (the caller supplies an `exists` probe), [`execute`] and
//! [`revert`] touch the file system and are all-or-nothing.
//!
//! Per-file pipeline: smart keyword strip → find/replace (in order) on the
//! original stem → template expansion → prefix/suffix → case → engine
//! convention (smart) → sanitize → extension.

mod case;
mod exec;
mod names;
mod smart;
mod sort;
mod template;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::{NoExpand, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

pub use self::case::{CaseMode, apply_case};
pub use self::exec::{ExecuteMode, RenameLog, RenameLogEntry, execute, revert};
pub use self::names::{is_valid_file_name, sanitize_stem};
pub use self::smart::{EnginePreset, SmartParams, TextureType, detect_texture_type};
pub use self::sort::{SortBy, natural_cmp};
use self::template::{DateFormat, Segment, Token};
use crate::{OpError, OpResult};

/// A find/replace pattern failed to compile. Params: `pattern`, `index`, `detail`.
pub const RENAME_INVALID_REGEX: &str = "RENAME_INVALID_REGEX";
/// Template contains an unknown `{token}`. Params: `token`.
pub const RENAME_UNKNOWN_TOKEN: &str = "RENAME_UNKNOWN_TOKEN";
/// `dateFormat` is not a valid strftime format. Params: `format`.
pub const RENAME_INVALID_DATE_FORMAT: &str = "RENAME_INVALID_DATE_FORMAT";
/// Execution refused because the plan still has conflicts. Params: `count`.
pub const RENAME_HAS_CONFLICTS: &str = "RENAME_HAS_CONFLICTS";
/// A file to rename/copy/revert does not exist. Params: `path`.
pub const RENAME_SOURCE_MISSING: &str = "RENAME_SOURCE_MISSING";
/// A destination already exists; nothing was changed. Params: `path`.
pub const RENAME_TARGET_EXISTS: &str = "RENAME_TARGET_EXISTS";
/// A file system operation failed; completed steps were rolled back.
/// Params: `from`, `to`, `detail`.
pub const RENAME_IO_FAILED: &str = "RENAME_IO_FAILED";
/// Rolling back after a failure also failed; some files may keep temporary
/// names. Params: `paths` (array of strings still to fix), `detail`.
pub const RENAME_ROLLBACK_FAILED: &str = "RENAME_ROLLBACK_FAILED";

const MAX_ZERO_PAD: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionCase {
    #[default]
    Keep,
    Lower,
    Upper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FindReplace {
    pub find: String,
    /// Regex mode supports `$1` / `${name}` capture references; literal mode inserts as-is.
    pub replace: String,
    pub regex: bool,
    pub case_sensitive: bool,
}

impl Default for FindReplace {
    fn default() -> Self {
        Self {
            find: String::new(),
            replace: String::new(),
            regex: false,
            case_sensitive: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenameParams {
    /// Tokens: `{name} {index} {parent} {width} {height} {ext} {date} {type}`.
    pub template: String,
    pub prefix: String,
    pub suffix: String,
    pub start_number: i64,
    pub step: i64,
    /// Minimum digits of `{index}` (0 = no padding, max 32).
    pub zero_pad: u32,
    /// Applied to the name part, never to the extension.
    pub case: CaseMode,
    pub find_replace: Vec<FindReplace>,
    pub smart: SmartParams,
    pub sort_by: SortBy,
    pub sort_desc: bool,
    pub keep_extension: bool,
    pub extension_case: ExtensionCase,
    /// strftime format for `{date}` (file modified time, local zone).
    pub date_format: String,
    /// Fix invalid names instead of flagging them.
    pub sanitize: bool,
    pub invalid_char_replacement: String,
}

impl Default for RenameParams {
    fn default() -> Self {
        Self {
            template: "{name}".into(),
            prefix: String::new(),
            suffix: String::new(),
            start_number: 1,
            step: 1,
            zero_pad: 0,
            case: CaseMode::Keep,
            find_replace: Vec::new(),
            smart: SmartParams::default(),
            sort_by: SortBy::None,
            sort_desc: false,
            keep_extension: true,
            extension_case: ExtensionCase::Keep,
            date_format: "%Y%m%d".into(),
            sanitize: false,
            invalid_char_replacement: "_".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEntry {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Last modification time, ms since the Unix epoch.
    pub modified_ms: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    /// Another item of the batch gets the same target (compared case-insensitively).
    DuplicateInBatch,
    /// Target exists and is not being renamed away by this batch.
    ExistsOnDisk,
    /// Illegal on Windows/macOS/Linux: `<>:"/\|?*`, control chars, trailing
    /// dot/space, reserved device names, empty name, > 255 bytes.
    InvalidName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlanItem {
    pub from: PathBuf,
    pub to: PathBuf,
    pub conflict: Option<Conflict>,
}

enum Rule {
    Regex(Regex, String),
    Literal(Regex, String),
}

impl Rule {
    fn apply(&self, s: &str) -> String {
        match self {
            Rule::Regex(re, rep) => re.replace_all(s, rep.as_str()).into_owned(),
            Rule::Literal(re, rep) => re.replace_all(s, NoExpand(rep)).into_owned(),
        }
    }
}

fn compile_rules(rules: &[FindReplace]) -> OpResult<Vec<Rule>> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.find.is_empty())
        .map(|(i, r)| {
            let pattern = if r.regex {
                r.find.clone()
            } else {
                regex::escape(&r.find)
            };
            let re = RegexBuilder::new(&pattern)
                .case_insensitive(!r.case_sensitive)
                .build()
                .map_err(|e| {
                    OpError::new(RENAME_INVALID_REGEX)
                        .with("pattern", r.find.as_str())
                        .with("index", i)
                        .with("detail", e.to_string())
                })?;
            Ok(if r.regex {
                Rule::Regex(re, r.replace.clone())
            } else {
                Rule::Literal(re, r.replace.clone())
            })
        })
        .collect()
}

fn validate(params: &RenameParams) -> OpResult<()> {
    if params.zero_pad > MAX_ZERO_PAD {
        return Err(OpError::invalid_param("zeroPad", "outOfRange"));
    }
    if params.sanitize
        && params
            .invalid_char_replacement
            .chars()
            .any(names::is_invalid_char)
    {
        return Err(OpError::invalid_param(
            "invalidCharReplacement",
            "invalidChars",
        ));
    }
    Ok(())
}

/// Case-insensitive, separator-agnostic key used to compare paths.
fn path_key(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

fn lossy(s: Option<&std::ffi::OsStr>) -> String {
    s.map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Computes new names for `entries` (returned in numbering order) and flags
/// conflicts. `exists` reports whether a target path is already taken; for a
/// copy into another folder, probe that folder (`dir.join(to.file_name())`).
pub fn plan(
    entries: &[RenameEntry],
    params: &RenameParams,
    exists: &dyn Fn(&Path) -> bool,
) -> OpResult<Vec<RenamePlanItem>> {
    validate(params)?;
    let segments = template::parse(&params.template)?;
    let has_type_token = segments.contains(&Segment::Token(Token::Type));
    let date_fmt = if segments.contains(&Segment::Token(Token::Date)) {
        Some(DateFormat::new(&params.date_format).ok_or_else(|| {
            OpError::new(RENAME_INVALID_DATE_FORMAT).with("format", params.date_format.as_str())
        })?)
    } else {
        None
    };
    let rules = compile_rules(&params.find_replace)?;
    let pad = params.zero_pad as usize;

    let mut items = Vec::with_capacity(entries.len());
    let mut invalid = Vec::with_capacity(entries.len());
    for (pos, &i) in sort::order(entries, params.sort_by, params.sort_desc)
        .iter()
        .enumerate()
    {
        let e = &entries[i];
        let stem = lossy(e.path.file_stem());
        let ext = lossy(e.path.extension());

        let detection =
            smart::detect(&stem).filter(|d| smart::suffix(&params.smart, d.ty).is_some());
        let type_suffix = detection
            .as_ref()
            .and_then(|d| smart::suffix(&params.smart, d.ty));
        let mut name = match (&detection, params.smart.enabled) {
            (Some(d), true) => smart::strip_keyword(&stem, &d.span),
            _ => stem.clone(),
        };
        for rule in &rules {
            name = rule.apply(&name);
        }

        let index = params
            .start_number
            .saturating_add((pos as i64).saturating_mul(params.step));
        let mut expanded = String::new();
        for seg in &segments {
            match seg {
                Segment::Literal(s) => expanded.push_str(s),
                Segment::Token(Token::Name) => expanded.push_str(&name),
                Segment::Token(Token::Index) => {
                    expanded.push_str(&template::format_index(index, pad))
                }
                Segment::Token(Token::Parent) => {
                    expanded.push_str(&lossy(e.path.parent().and_then(Path::file_name)))
                }
                Segment::Token(Token::Width) => expanded.push_str(&e.width.to_string()),
                Segment::Token(Token::Height) => expanded.push_str(&e.height.to_string()),
                Segment::Token(Token::Ext) => expanded.push_str(&ext),
                Segment::Token(Token::Date) => {
                    if let Some(s) = date_fmt.as_ref().and_then(|f| f.format(e.modified_ms)) {
                        expanded.push_str(&s);
                    }
                }
                Segment::Token(Token::Type) => {
                    expanded.push_str(type_suffix.as_deref().unwrap_or(""))
                }
            }
        }

        let mut body = apply_case(
            &format!("{}{}{}", params.prefix, expanded, params.suffix),
            params.case,
        );
        if params.smart.enabled {
            let suffix = if has_type_token {
                None
            } else {
                type_suffix.as_deref()
            };
            body = smart::wrap(params.smart.preset, &body, suffix);
        }

        let new_ext = match params.extension_case {
            ExtensionCase::Keep => ext.clone(),
            ExtensionCase::Lower => ext.to_lowercase(),
            ExtensionCase::Upper => ext.to_uppercase(),
        };
        let with_ext = params.keep_extension && !new_ext.is_empty();
        if params.sanitize {
            body = sanitize_stem(&body, &params.invalid_char_replacement, !with_ext);
        }
        let file_name = if with_ext {
            format!("{body}.{new_ext}")
        } else {
            body.clone()
        };

        invalid.push(body.is_empty() || !is_valid_file_name(&file_name));
        let to = e
            .path
            .parent()
            .map(|p| p.join(&file_name))
            .unwrap_or_else(|| PathBuf::from(&file_name));
        items.push(RenamePlanItem {
            from: e.path.clone(),
            to,
            conflict: None,
        });
    }

    let mut target_count: HashMap<String, usize> = HashMap::new();
    for it in &items {
        *target_count.entry(path_key(&it.to)).or_default() += 1;
    }
    let moving_away: HashSet<String> = items
        .iter()
        .filter(|it| it.from != it.to)
        .map(|it| path_key(&it.from))
        .collect();
    for (it, bad) in items.iter_mut().zip(invalid) {
        let key = path_key(&it.to);
        it.conflict = if bad {
            Some(Conflict::InvalidName)
        } else if target_count[&key] > 1 {
            Some(Conflict::DuplicateInBatch)
        } else if key != path_key(&it.from) && !moving_away.contains(&key) && exists(&it.to) {
            Some(Conflict::ExistsOnDisk)
        } else {
            None
        };
    }
    Ok(items)
}
