//! Name template parsing: `{token}` placeholders and literal text.

use std::fmt::Write as _;

use chrono::format::{Item, StrftimeItems};
use chrono::{Local, TimeZone};

use super::RENAME_UNKNOWN_TOKEN;
use crate::{OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Name,
    Index,
    Parent,
    Width,
    Height,
    Ext,
    Date,
    Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Literal(String),
    Token(Token),
}

/// Splits a template into segments. `{ident}` must be a known token; braces
/// that do not enclose an identifier are kept literally.
pub fn parse(template: &str) -> OpResult<Vec<Segment>> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        lit.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let ident_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if ident_len > 0 && after[ident_len..].starts_with('}') {
            let ident = &after[..ident_len];
            let token = match ident {
                "name" => Token::Name,
                "index" => Token::Index,
                "parent" => Token::Parent,
                "width" => Token::Width,
                "height" => Token::Height,
                "ext" => Token::Ext,
                "date" => Token::Date,
                "type" => Token::Type,
                _ => return Err(OpError::new(RENAME_UNKNOWN_TOKEN).with("token", ident)),
            };
            if !lit.is_empty() {
                out.push(Segment::Literal(std::mem::take(&mut lit)));
            }
            out.push(Segment::Token(token));
            rest = &after[ident_len + 1..];
        } else {
            lit.push('{');
            rest = after;
        }
    }
    lit.push_str(rest);
    if !lit.is_empty() {
        out.push(Segment::Literal(lit));
    }
    Ok(out)
}

/// Validated strftime format.
pub struct DateFormat<'a>(Vec<Item<'a>>);

impl<'a> DateFormat<'a> {
    pub fn new(fmt: &'a str) -> Option<Self> {
        let items: Vec<Item<'a>> = StrftimeItems::new(fmt).collect();
        (!items.iter().any(|i| matches!(i, Item::Error))).then_some(Self(items))
    }

    /// Formats a Unix timestamp (ms) in local time; `None` if out of range.
    pub fn format(&self, ms: i64) -> Option<String> {
        let dt = Local.timestamp_millis_opt(ms).earliest()?;
        let mut s = String::new();
        write!(s, "{}", dt.format_with_items(self.0.iter())).ok()?;
        Some(s)
    }
}

pub fn format_index(n: i64, pad: usize) -> String {
    if n < 0 {
        format!("-{:0pad$}", n.unsigned_abs())
    } else {
        format!("{n:0pad$}")
    }
}
