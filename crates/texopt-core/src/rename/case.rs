//! Word splitting and case transforms (Unicode-aware, so diacritics survive).

use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CaseMode {
    #[default]
    Keep,
    Lower,
    Upper,
    Snake,
    Kebab,
    Camel,
    Pascal,
}

/// Byte ranges of the words in `s`. Any non-alphanumeric character separates
/// words; a new word also starts at `aB`, `1B` and before the last capital of
/// an acronym followed by lowercase (`XMLFile` → `XML`, `File`).
pub fn word_spans(s: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &(pos, c)) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if let Some(st) = start.take() {
                spans.push(st..pos);
            }
            continue;
        }
        if let Some(st) = start {
            let prev = chars[i - 1].1;
            let next = chars.get(i + 1).map(|&(_, n)| n);
            let boundary = c.is_uppercase()
                && (prev.is_lowercase()
                    || prev.is_numeric()
                    || (prev.is_uppercase() && next.is_some_and(char::is_lowercase)));
            if boundary {
                spans.push(st..pos);
                start = Some(pos);
            }
        } else {
            start = Some(pos);
        }
    }
    if let Some(st) = start {
        spans.push(st..s.len());
    }
    spans
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.as_str().to_lowercase().chars())
            .collect(),
        None => String::new(),
    }
}

pub fn apply_case(s: &str, mode: CaseMode) -> String {
    let words = || word_spans(s).into_iter().map(|r| &s[r]);
    match mode {
        CaseMode::Keep => s.to_owned(),
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Snake => words().map(str::to_lowercase).collect::<Vec<_>>().join("_"),
        CaseMode::Kebab => words().map(str::to_lowercase).collect::<Vec<_>>().join("-"),
        CaseMode::Pascal => words().map(capitalize).collect(),
        CaseMode::Camel => words()
            .enumerate()
            .map(|(i, w)| {
                if i == 0 {
                    w.to_lowercase()
                } else {
                    capitalize(w)
                }
            })
            .collect(),
    }
}
