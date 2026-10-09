//! Ordering of entries before numbering.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::RenameEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    /// Keep the input order.
    #[default]
    None,
    /// Case-insensitive file name.
    Name,
    /// Case-insensitive file name with digit runs compared as numbers (`2 < 10`).
    NaturalName,
    Modified,
    Size,
    /// Pixel area, then width.
    Dimensions,
}

fn file_name(e: &RenameEntry) -> String {
    e.path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Compares strings treating runs of ASCII digits as numbers.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        s.push(c);
                    }
                    s
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let ord = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| da.len().cmp(&db.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Indices of `entries` in the requested order (stable).
pub fn order(entries: &[RenameEntry], by: SortBy, desc: bool) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..entries.len()).collect();
    let by_name = |a: &RenameEntry, b: &RenameEntry, natural: bool| {
        let (na, nb) = (file_name(a), file_name(b));
        let (la, lb) = (na.to_lowercase(), nb.to_lowercase());
        let primary = if natural {
            natural_cmp(&la, &lb)
        } else {
            la.cmp(&lb)
        };
        primary.then_with(|| na.cmp(&nb))
    };
    let cmp = |a: &RenameEntry, b: &RenameEntry| -> Ordering {
        match by {
            SortBy::None => Ordering::Equal,
            SortBy::Name => by_name(a, b, false),
            SortBy::NaturalName => by_name(a, b, true),
            SortBy::Modified => a
                .modified_ms
                .cmp(&b.modified_ms)
                .then_with(|| by_name(a, b, true)),
            SortBy::Size => a
                .size_bytes
                .cmp(&b.size_bytes)
                .then_with(|| by_name(a, b, true)),
            SortBy::Dimensions => (a.width as u64 * a.height as u64)
                .cmp(&(b.width as u64 * b.height as u64))
                .then_with(|| a.width.cmp(&b.width))
                .then_with(|| by_name(a, b, true)),
        }
    };
    if by != SortBy::None {
        idx.sort_by(|&i, &j| {
            let o = cmp(&entries[i], &entries[j]);
            if desc { o.reverse() } else { o }
        });
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["a10", "a2", "a02", "a1", "b", "a"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["a", "a1", "a2", "a02", "a10", "b"]);
    }
}
