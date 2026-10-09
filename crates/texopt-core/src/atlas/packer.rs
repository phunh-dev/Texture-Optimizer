//! Rectangle bin packing (MaxRects + Skyline) and page sizing.
//!
//! The packer works in "bin space": every sprite becomes a *cell* of
//! `w + 2*extrude + padding` x `h + 2*extrude + padding` pixels and a page of
//! `W x H` pixels becomes a bin of `W - 2*border + padding` x
//! `H - 2*border + padding`. A cell at bin position `(cx, cy)` puts the sprite
//! frame at page position `(border + cx + extrude, border + cy + extrude)`, so
//! neighbouring extruded areas are always at least `padding` pixels apart and
//! nothing touches the `border` strip.

use super::params::{AtlasParams, PackAlgorithm, PackHeuristic, SizeMode, floor_pot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> u32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }
    pub fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        o.x < self.right() && o.right() > self.x && o.y < self.bottom() && o.bottom() > self.y
    }
}

/// Position of a cell inside a bin. `rotated` means the cell occupies `h x w`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    pub x: u32,
    pub y: u32,
    pub rotated: bool,
}

/// Unrotated cell size (already inflated by padding/extrude).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackItem {
    pub w: u32,
    pub h: u32,
}

impl PackItem {
    pub fn dims(&self, rotated: bool) -> (u32, u32) {
        if rotated {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        }
    }
    fn fits_in(&self, bw: u32, bh: u32, allow_rotation: bool) -> bool {
        (self.w <= bw && self.h <= bh) || (allow_rotation && self.h <= bw && self.w <= bh)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackCfg {
    pub algorithm: PackAlgorithm,
    pub heuristic: PackHeuristic,
    pub allow_rotation: bool,
}

impl PackCfg {
    pub fn from_params(p: &AtlasParams) -> Self {
        Self {
            algorithm: p.algorithm,
            heuristic: p.heuristic,
            allow_rotation: p.allow_rotation,
        }
    }
}

/// Spacing rules shared by packing and composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Geometry {
    pub padding: u32,
    pub extrude: u32,
    pub border: u32,
}

impl Geometry {
    pub fn from_params(p: &AtlasParams) -> Self {
        Self {
            padding: p.padding,
            extrude: p.extrude,
            border: p.border,
        }
    }

    pub fn cell(&self, w: u32, h: u32) -> PackItem {
        let grow = 2 * self.extrude + self.padding;
        PackItem {
            w: w + grow,
            h: h + grow,
        }
    }

    pub fn bin_size(&self, page_w: u32, page_h: u32) -> Option<(u32, u32)> {
        let f = |v: u32| {
            let b = i64::from(v) + i64::from(self.padding) - 2 * i64::from(self.border);
            (b > 0).then_some(b as u32)
        };
        Some((f(page_w)?, f(page_h)?))
    }

    /// Smallest page holding cells up to bin coordinates `(right, bottom)`.
    pub fn page_extent(&self, right: u32, bottom: u32) -> (u32, u32) {
        (
            right - self.padding + 2 * self.border,
            bottom - self.padding + 2 * self.border,
        )
    }

    pub fn frame_pos(&self, cell_x: u32, cell_y: u32) -> (u32, u32) {
        (
            self.border + cell_x + self.extrude,
            self.border + cell_y + self.extrude,
        )
    }

    pub fn cell_pos(&self, frame_x: u32, frame_y: u32) -> Option<(u32, u32)> {
        let off = self.border + self.extrude;
        Some((frame_x.checked_sub(off)?, frame_y.checked_sub(off)?))
    }
}

// ---------------------------------------------------------------- MaxRects

#[derive(Debug, Clone)]
pub(crate) struct MaxRectsBin {
    w: u32,
    h: u32,
    heuristic: PackHeuristic,
    free: Vec<Rect>,
    used: Vec<Rect>,
}

impl MaxRectsBin {
    pub fn new(w: u32, h: u32, heuristic: PackHeuristic) -> Self {
        let free = if w > 0 && h > 0 {
            vec![Rect::new(0, 0, w, h)]
        } else {
            Vec::new()
        };
        Self {
            w,
            h,
            heuristic,
            free,
            used: Vec::new(),
        }
    }

    pub fn insert(&mut self, item: PackItem, allow_rotation: bool) -> Option<Placed> {
        let mut best: Option<((i64, i64), Rect, bool)> = None;
        for fr in &self.free {
            for rotated in [false, true] {
                if rotated && (!allow_rotation || item.w == item.h) {
                    continue;
                }
                let (w, h) = item.dims(rotated);
                if w > fr.w || h > fr.h {
                    continue;
                }
                let score = self.score(fr, w, h);
                if best.as_ref().is_none_or(|b| score < b.0) {
                    best = Some((score, Rect::new(fr.x, fr.y, w, h), rotated));
                }
            }
        }
        let (_, rect, rotated) = best?;
        self.place(rect);
        Some(Placed {
            x: rect.x,
            y: rect.y,
            rotated,
        })
    }

    fn score(&self, fr: &Rect, w: u32, h: u32) -> (i64, i64) {
        let (fw, fh, w, h) = (i64::from(fr.w), i64::from(fr.h), i64::from(w), i64::from(h));
        let dw = fw - w;
        let dh = fh - h;
        match self.heuristic {
            PackHeuristic::BestLongSideFit => (dw.max(dh), dw.min(dh)),
            PackHeuristic::BestAreaFit => (fw * fh - w * h, dw.min(dh)),
            PackHeuristic::BottomLeft | PackHeuristic::MinWaste => {
                (i64::from(fr.y) + h, i64::from(fr.x))
            }
            PackHeuristic::ContactPoint => {
                let r = Rect::new(fr.x, fr.y, w as u32, h as u32);
                (-self.contact_score(&r), i64::from(fr.y) + h)
            }
            PackHeuristic::BestShortSideFit => (dw.min(dh), dw.max(dh)),
        }
    }

    fn contact_score(&self, r: &Rect) -> i64 {
        fn overlap(a0: u32, a1: u32, b0: u32, b1: u32) -> i64 {
            if a1 < b0 || b1 < a0 {
                0
            } else {
                i64::from(a1.min(b1)) - i64::from(a0.max(b0))
            }
        }
        let mut score = 0;
        if r.x == 0 || r.right() == self.w {
            score += i64::from(r.h);
        }
        if r.y == 0 || r.bottom() == self.h {
            score += i64::from(r.w);
        }
        for u in &self.used {
            if u.x == r.right() || u.right() == r.x {
                score += overlap(u.y, u.bottom(), r.y, r.bottom());
            }
            if u.y == r.bottom() || u.bottom() == r.y {
                score += overlap(u.x, u.right(), r.x, r.right());
            }
        }
        score
    }

    /// Mark `rect` as occupied (also used to restore fixed rects in incremental mode).
    pub fn place(&mut self, rect: Rect) {
        let mut next = Vec::with_capacity(self.free.len() + 4);
        for fr in self.free.drain(..) {
            if !fr.intersects(&rect) {
                next.push(fr);
                continue;
            }
            if rect.x > fr.x {
                next.push(Rect::new(fr.x, fr.y, rect.x - fr.x, fr.h));
            }
            if rect.right() < fr.right() {
                next.push(Rect::new(
                    rect.right(),
                    fr.y,
                    fr.right() - rect.right(),
                    fr.h,
                ));
            }
            if rect.y > fr.y {
                next.push(Rect::new(fr.x, fr.y, fr.w, rect.y - fr.y));
            }
            if rect.bottom() < fr.bottom() {
                next.push(Rect::new(
                    fr.x,
                    rect.bottom(),
                    fr.w,
                    fr.bottom() - rect.bottom(),
                ));
            }
        }
        let n = next.len();
        let mut keep = vec![true; n];
        for i in 0..n {
            for j in 0..n {
                if i != j && keep[j] && next[j].contains(&next[i]) {
                    keep[i] = false;
                    break;
                }
            }
        }
        self.free = next
            .into_iter()
            .zip(keep)
            .filter_map(|(r, k)| k.then_some(r))
            .collect();
        self.used.push(rect);
    }
}

// ----------------------------------------------------------------- Skyline

#[derive(Debug, Clone, Copy)]
struct SkyNode {
    x: u32,
    y: u32,
    w: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct SkylineBin {
    w: u32,
    h: u32,
    min_waste: bool,
    nodes: Vec<SkyNode>,
}

impl SkylineBin {
    pub fn new(w: u32, h: u32, heuristic: PackHeuristic) -> Self {
        Self {
            w,
            h,
            min_waste: heuristic == PackHeuristic::MinWaste,
            nodes: vec![SkyNode { x: 0, y: 0, w }],
        }
    }

    fn fits(&self, i: usize, w: u32, h: u32) -> Option<u32> {
        let x = self.nodes[i].x;
        if x + w > self.w {
            return None;
        }
        let mut left = i64::from(w);
        let mut j = i;
        let mut y = self.nodes[i].y;
        while left > 0 {
            let n = self.nodes.get(j)?;
            y = y.max(n.y);
            if y + h > self.h {
                return None;
            }
            left -= i64::from(n.w);
            j += 1;
        }
        Some(y)
    }

    fn waste(&self, i: usize, w: u32, y: u32) -> i64 {
        let x1 = self.nodes[i].x + w;
        let mut waste = 0i64;
        for n in &self.nodes[i..] {
            if n.x >= x1 {
                break;
            }
            let right = (n.x + n.w).min(x1);
            waste += i64::from(right - n.x) * i64::from(y - n.y);
        }
        waste
    }

    pub fn insert(&mut self, item: PackItem, allow_rotation: bool) -> Option<Placed> {
        let mut best: Option<((i64, i64), usize, u32, bool)> = None;
        for i in 0..self.nodes.len() {
            for rotated in [false, true] {
                if rotated && (!allow_rotation || item.w == item.h) {
                    continue;
                }
                let (w, h) = item.dims(rotated);
                let Some(y) = self.fits(i, w, h) else {
                    continue;
                };
                let top = i64::from(y) + i64::from(h);
                let score = if self.min_waste {
                    (self.waste(i, w, y), top)
                } else {
                    (top, i64::from(self.nodes[i].w))
                };
                if best.as_ref().is_none_or(|b| score < b.0) {
                    best = Some((score, i, y, rotated));
                }
            }
        }
        let (_, i, y, rotated) = best?;
        let (w, h) = item.dims(rotated);
        let x = self.nodes[i].x;
        self.add_level(i, x, y, w, h);
        Some(Placed { x, y, rotated })
    }

    fn add_level(&mut self, i: usize, x: u32, y: u32, w: u32, h: u32) {
        self.nodes.insert(i, SkyNode { x, y: y + h, w });
        let k = i + 1;
        while k < self.nodes.len() {
            let prev_end = self.nodes[k - 1].x + self.nodes[k - 1].w;
            if self.nodes[k].x >= prev_end {
                break;
            }
            let shrink = prev_end - self.nodes[k].x;
            if self.nodes[k].w <= shrink {
                self.nodes.remove(k);
            } else {
                self.nodes[k].x += shrink;
                self.nodes[k].w -= shrink;
                break;
            }
        }
        let mut k = 0;
        while k + 1 < self.nodes.len() {
            if self.nodes[k].y == self.nodes[k + 1].y {
                self.nodes[k].w += self.nodes[k + 1].w;
                self.nodes.remove(k + 1);
            } else {
                k += 1;
            }
        }
    }
}

enum AnyBin {
    MaxRects(MaxRectsBin),
    Skyline(SkylineBin),
}

impl AnyBin {
    fn new(cfg: &PackCfg, w: u32, h: u32) -> Self {
        match cfg.algorithm {
            PackAlgorithm::MaxRects => AnyBin::MaxRects(MaxRectsBin::new(w, h, cfg.heuristic)),
            PackAlgorithm::Skyline => AnyBin::Skyline(SkylineBin::new(w, h, cfg.heuristic)),
        }
    }
    fn insert(&mut self, item: PackItem, allow_rotation: bool) -> Option<Placed> {
        match self {
            AnyBin::MaxRects(b) => b.insert(item, allow_rotation),
            AnyBin::Skyline(b) => b.insert(item, allow_rotation),
        }
    }
}

// ----------------------------------------------------------- whole pages

/// Pack every item into one `bw x bh` bin, or `None` if any does not fit.
pub(crate) fn try_pack_all(
    items: &[PackItem],
    bw: u32,
    bh: u32,
    cfg: &PackCfg,
) -> Option<Vec<Placed>> {
    let area: u64 = items.iter().map(|i| u64::from(i.w) * u64::from(i.h)).sum();
    if area > u64::from(bw) * u64::from(bh)
        || !items.iter().all(|i| i.fits_in(bw, bh, cfg.allow_rotation))
    {
        return None;
    }
    let mut bin = AnyBin::new(cfg, bw, bh);
    items
        .iter()
        .map(|it| bin.insert(*it, cfg.allow_rotation))
        .collect()
}

/// Pack as many items as possible (in order); `None` for those left out.
pub(crate) fn pack_greedy(
    items: &[PackItem],
    bw: u32,
    bh: u32,
    cfg: &PackCfg,
) -> Vec<Option<Placed>> {
    let mut bin = AnyBin::new(cfg, bw, bh);
    items
        .iter()
        .map(|it| bin.insert(*it, cfg.allow_rotation))
        .collect()
}

/// Can `item` be placed alone on the largest allowed page?
pub(crate) fn fits_max_page(item: PackItem, params: &AtlasParams) -> bool {
    let (mw, mh) = params.effective_max();
    Geometry::from_params(params)
        .bin_size(mw, mh)
        .is_some_and(|(bw, bh)| item.fits_in(bw, bh, params.allow_rotation))
}

fn extent(items: &[PackItem], placed: &[Placed]) -> (u32, u32) {
    items.iter().zip(placed).fold((0, 0), |(r, b), (it, p)| {
        let (w, h) = it.dims(p.rotated);
        (r.max(p.x + w), b.max(p.y + h))
    })
}

fn pots_up_to(max: u32) -> Vec<u32> {
    let mut v = Vec::new();
    let mut p = 1u32;
    while p <= max {
        v.push(p);
        p <<= 1;
    }
    v
}

/// Evenly spaced candidates in `lo..=hi` (at most ~65), always including `hi`.
fn spread(lo: u32, hi: u32) -> Vec<u32> {
    if lo > hi {
        return Vec::new();
    }
    let span = hi - lo;
    if span <= 64 {
        return (lo..=hi).collect();
    }
    let mut v: Vec<u32> = (0..64u64)
        .map(|k| lo + (u64::from(span) * k / 64) as u32)
        .collect();
    v.push(hi);
    v.dedup();
    v
}

/// Lay out all `items` on a single page following `sizeMode`, `forcePot` and
/// `forceSquare`. Returns `(page_w, page_h, placements)`.
pub(crate) fn layout_page(
    items: &[PackItem],
    params: &AtlasParams,
) -> Option<(u32, u32, Vec<Placed>)> {
    let cfg = PackCfg::from_params(params);
    let geom = Geometry::from_params(params);
    let (mw, mh) = params.effective_max();
    if params.size_mode == SizeMode::Fixed {
        let (bw, bh) = geom.bin_size(mw, mh)?;
        return try_pack_all(items, bw, bh, &cfg).map(|p| (mw, mh, p));
    }
    if params.force_pot {
        let mut cands: Vec<(u32, u32)> = Vec::new();
        for &w in &pots_up_to(floor_pot(mw)) {
            for &h in &pots_up_to(floor_pot(mh)) {
                if !params.force_square || w == h {
                    cands.push((w, h));
                }
            }
        }
        cands.sort_by_key(|&(w, h)| (u64::from(w) * u64::from(h), w.max(h), h));
        for (w, h) in cands {
            let Some((bw, bh)) = geom.bin_size(w, h) else {
                continue;
            };
            if let Some(p) = try_pack_all(items, bw, bh, &cfg) {
                return Some((w, h, p));
            }
        }
        return None;
    }

    let area: u64 = items.iter().map(|i| u64::from(i.w) * u64::from(i.h)).sum();
    let min_w = items
        .iter()
        .map(|i| {
            if cfg.allow_rotation {
                i.w.min(i.h)
            } else {
                i.w
            }
        })
        .max()
        .unwrap_or(1);
    if params.force_square {
        let (side_bin_max, _) = geom.bin_size(mw.min(mh), mw.min(mh))?;
        // Rotation cannot help in a square bin: every cell needs max(w, h).
        let need = items.iter().map(|i| i.w.max(i.h)).max().unwrap_or(1);
        let lo = need.max((area as f64).sqrt().ceil() as u32);
        let attempt = |s: u32| try_pack_all(items, s, s, &cfg);
        let mut prev_fail = lo.saturating_sub(1);
        for s in spread(lo, side_bin_max) {
            if let Some(mut best) = attempt(s) {
                let (mut lo2, mut hi2) = (prev_fail + 1, s);
                while lo2 < hi2 {
                    let mid = lo2 + (hi2 - lo2) / 2;
                    match attempt(mid) {
                        Some(p) => {
                            best = p;
                            hi2 = mid;
                        }
                        None => lo2 = mid + 1,
                    }
                }
                let (r, b) = extent(items, &best);
                let (pw, ph) = geom.page_extent(r, b);
                let side = pw.max(ph);
                return Some((side, side, best));
            }
            prev_fail = s;
        }
        return None;
    }

    let (bw_max, bh_max) = geom.bin_size(mw, mh)?;
    let lo = min_w.max(area.div_ceil(u64::from(bh_max)).min(u64::from(u32::MAX)) as u32);
    type Candidate = ((u64, u32, u32), u32, u32, Vec<Placed>);
    let mut best: Option<Candidate> = None;
    for bw in spread(lo, bw_max) {
        let Some(p) = try_pack_all(items, bw, bh_max, &cfg) else {
            continue;
        };
        let (r, b) = extent(items, &p);
        let (pw, ph) = geom.page_extent(r, b);
        let score = (u64::from(pw) * u64::from(ph), pw.max(ph), ph);
        if best.as_ref().is_none_or(|b| score < b.0) {
            best = Some((score, pw, ph, p));
        }
    }
    best.map(|(_, w, h, p)| (w, h, p))
}

/// Size steps used when a page has to grow during incremental packing.
pub(crate) fn grow_step(w: u32, h: u32, params: &AtlasParams) -> Option<(u32, u32)> {
    let (mw, mh) = params.effective_max();
    let up = |v: u32, m: u32| {
        if v >= m {
            None
        } else {
            Some(v.saturating_mul(2).min(m))
        }
    };
    if params.force_square {
        let s = up(w.max(h), mw.min(mh))?;
        return Some((s, s));
    }
    let width_first = w <= h;
    let try_w = up(w, mw).map(|nw| (nw, h));
    let try_h = up(h, mh).map(|nh| (w, nh));
    if width_first {
        try_w.or(try_h)
    } else {
        try_h.or(try_w)
    }
}
