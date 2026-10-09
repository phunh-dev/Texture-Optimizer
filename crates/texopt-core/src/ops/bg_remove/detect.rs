//! Background detection from the image border: dominant color or checkerboard.

use super::color::rgb_dist;
use crate::ImageBuf;

/// RGB distance under which two border pixels count as the same color while
/// detecting (independent of the user tolerance so detection is stable).
const CLUSTER_DIST: f32 = 24.0;
/// Share of labelled border pixels that must agree with the fitted grid.
const MIN_GRID_AGREEMENT: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checker {
    /// `colors[parity]`, see [`Checker::parity`].
    pub colors: [[u8; 3]; 2],
    pub cell: u32,
    /// Grid phase: cell boundaries are at `x ≡ ox (mod cell)`, `y ≡ oy (mod cell)`.
    pub ox: u32,
    pub oy: u32,
}

impl Checker {
    pub fn parity(&self, x: u32, y: u32) -> usize {
        let cx = (x + self.cell - self.ox) / self.cell;
        let cy = (y + self.cell - self.oy) / self.cell;
        ((cx + cy) % 2) as usize
    }

    pub fn expected(&self, x: u32, y: u32) -> [u8; 3] {
        self.colors[self.parity(x, y)]
    }
}

/// Pixels along one border line: `(x, y, rgb or None when fully transparent)`.
type Line = Vec<(u32, u32, Option<[u8; 3]>)>;

/// Border pixels as four lines (top, bottom: horizontal; left, right: vertical).
struct Border {
    horizontal: Vec<Line>,
    vertical: Vec<Line>,
}

impl Border {
    fn new(img: &ImageBuf) -> Self {
        let (w, h) = img.dimensions();
        let px = |x: u32, y: u32| {
            let p = img.get_pixel(x, y);
            (x, y, (p[3] > 0).then_some([p[0], p[1], p[2]]))
        };
        let horizontal = [0, h - 1]
            .iter()
            .map(|&y| (0..w).map(|x| px(x, y)).collect())
            .collect();
        let vertical = [0, w - 1]
            .iter()
            .map(|&x| (0..h).map(|y| px(x, y)).collect())
            .collect();
        Self {
            horizontal,
            vertical,
        }
    }

    fn lines(&self) -> impl Iterator<Item = &Line> {
        self.horizontal.iter().chain(self.vertical.iter())
    }

    fn colors(&self) -> Vec<[u8; 3]> {
        self.lines()
            .flat_map(|l| l.iter().filter_map(|p| p.2))
            .collect()
    }

    fn len(&self) -> usize {
        self.lines().map(Vec::len).sum()
    }
}

/// A group of similar colors: rounded mean and member count.
#[derive(Debug, Clone, Copy)]
struct Cluster {
    mean: [u8; 3],
    count: usize,
}

/// Finds the most common color group among `colors`, ignoring `exclude`d ones.
fn dominant_cluster(colors: &[[u8; 3]], exclude: Option<[u8; 3]>) -> Option<Cluster> {
    let pool: Vec<[u8; 3]> = colors
        .iter()
        .copied()
        .filter(|c| exclude.is_none_or(|e| rgb_dist(*c, e) > CLUSTER_DIST))
        .collect();
    if pool.is_empty() {
        return None;
    }
    let key = |c: &[u8; 3]| {
        ((c[0] as usize >> 4) << 8) | ((c[1] as usize >> 4) << 4) | (c[2] as usize >> 4)
    };
    let mut hist = vec![0usize; 4096];
    for c in &pool {
        hist[key(c)] += 1;
    }
    let best = (0..hist.len()).max_by_key(|&i| (hist[i], std::cmp::Reverse(i)))?;
    let seed = mean(pool.iter().filter(|c| key(c) == best))?;
    let members: Vec<&[u8; 3]> = pool
        .iter()
        .filter(|c| rgb_dist(**c, seed) <= CLUSTER_DIST)
        .collect();
    let count = members.len();
    Some(Cluster {
        mean: mean(members.into_iter()).unwrap_or(seed),
        count,
    })
}

fn mean<'a>(it: impl Iterator<Item = &'a [u8; 3]>) -> Option<[u8; 3]> {
    let (mut sum, mut n) = ([0f64; 3], 0usize);
    for c in it {
        for i in 0..3 {
            sum[i] += c[i] as f64;
        }
        n += 1;
    }
    (n > 0).then(|| sum.map(|s| (s / n as f64).round() as u8))
}

/// Most common opaque border color, if the border has any visible pixel.
pub fn dominant_border_color(img: &ImageBuf) -> Option<[u8; 3]> {
    if img.width() == 0 || img.height() == 0 {
        return None;
    }
    dominant_cluster(&Border::new(img).colors(), None).map(|c| c.mean)
}

/// Detects a two-color checkerboard on the border. `cell` forces the cell size.
pub fn detect_checker(img: &ImageBuf, cell: Option<u32>) -> Option<Checker> {
    if img.width() < 2 || img.height() < 2 {
        return None;
    }
    let border = Border::new(img);
    let colors = border.colors();
    let a = dominant_cluster(&colors, None)?;
    let b = dominant_cluster(&colors, Some(a.mean))?;
    let total = border.len();
    if (a.count + b.count) * 2 < total || b.count * 10 < a.count + b.count {
        return None;
    }

    let label = |c: Option<[u8; 3]>| -> u8 {
        let Some(c) = c else { return 2 };
        let (da, db) = (rgb_dist(c, a.mean), rgb_dist(c, b.mean));
        if da.min(db) > CLUSTER_DIST {
            2
        } else if da <= db {
            0
        } else {
            1
        }
    };
    let labelled: Vec<Vec<(u32, u32, u8)>> = border
        .lines()
        .map(|l| l.iter().map(|&(x, y, c)| (x, y, label(c))).collect())
        .collect();
    let (h_lines, v_lines) = labelled.split_at(border.horizontal.len());

    let cell = match cell {
        Some(c) => c,
        None => estimate_cell(&labelled)?,
    };
    let ox = estimate_phase(h_lines, cell, |p| p.0);
    let oy = estimate_phase(v_lines, cell, |p| p.1);
    let grid = Checker {
        colors: [a.mean, b.mean],
        cell,
        ox,
        oy,
    };

    let (mut agree, mut disagree) = (0usize, 0usize);
    for &(x, y, l) in labelled.iter().flatten() {
        if l == 2 {
            continue;
        }
        if grid.parity(x, y) == l as usize {
            agree += 1;
        } else {
            disagree += 1;
        }
    }
    let colors = if agree >= disagree {
        [a.mean, b.mean]
    } else {
        [b.mean, a.mean]
    };
    let ratio = agree.max(disagree) as f64 / (agree + disagree).max(1) as f64;
    (ratio >= MIN_GRID_AGREEMENT).then_some(Checker { colors, ..grid })
}

/// Runs of equal labels along a line: `(label, length)`.
fn runs(line: &[(u32, u32, u8)]) -> Vec<(u8, u32)> {
    let mut out: Vec<(u8, u32)> = Vec::new();
    for p in line {
        match out.last_mut() {
            Some((l, n)) if *l == p.2 => *n += 1,
            _ => out.push((p.2, 1)),
        }
    }
    out
}

/// Most frequent run length; interior runs (flanked by checker runs) are
/// preferred because runs touching the image edge may be partial cells.
fn estimate_cell(lines: &[Vec<(u32, u32, u8)>]) -> Option<u32> {
    let mut interior = Vec::new();
    let mut all = Vec::new();
    for line in lines {
        let r = runs(line);
        for (i, &(l, n)) in r.iter().enumerate() {
            if l == 2 {
                continue;
            }
            all.push(n);
            if i > 0 && i + 1 < r.len() && r[i - 1].0 != 2 && r[i + 1].0 != 2 {
                interior.push(n);
            }
        }
    }
    let lengths = if interior.is_empty() { all } else { interior };
    let mut counts = std::collections::BTreeMap::<u32, usize>::new();
    for n in lengths {
        *counts.entry(n).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(n, c)| (c, n))
        .map(|(n, _)| n)
}

/// Most voted `position mod cell` among label changes between the two colors.
fn estimate_phase(
    lines: &[Vec<(u32, u32, u8)>],
    cell: u32,
    coord: impl Fn(&(u32, u32, u8)) -> u32,
) -> u32 {
    let mut votes = vec![0usize; cell as usize];
    for line in lines {
        for pair in line.windows(2) {
            let (l0, l1) = (pair[0].2, pair[1].2);
            if l0 != 2 && l1 != 2 && l0 != l1 {
                votes[(coord(&pair[1]) % cell) as usize] += 1;
            }
        }
    }
    (0..cell)
        .max_by_key(|&i| (votes[i as usize], std::cmp::Reverse(i)))
        .unwrap_or(0)
}
