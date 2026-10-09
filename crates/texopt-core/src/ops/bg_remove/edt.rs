//! Exact Euclidean distance transform (Felzenszwalb & Huttenlocher).

const INF: f64 = 1e20;

/// Squared Euclidean distance from every pixel to the nearest `source` pixel
/// (0 for sources). Pixels are unreachable (≈1e20) when there is no source.
pub fn squared_distance(w: usize, h: usize, source: &[bool]) -> Vec<f64> {
    debug_assert_eq!(source.len(), w * h);
    let mut grid: Vec<f64> = source.iter().map(|&s| if s { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let mut f = vec![0f64; n];
    let mut d = vec![0f64; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];

    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        transform_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        let row = &mut grid[y * w..(y + 1) * w];
        f[..w].copy_from_slice(row);
        transform_1d(&f[..w], row, &mut v, &mut z);
    }
    grid
}

fn transform_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for q in 1..n {
        let qf = q as f64;
        let mut s;
        loop {
            let p = v[k] as f64;
            s = ((f[q] + qf * qf) - (f[v[k]] + p * p)) / (2.0 * qf - 2.0 * p);
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = INF;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        let qf = q as f64;
        while z[k + 1] < qf {
            k += 1;
        }
        let p = v[k] as f64;
        *out = (qf - p) * (qf - p) + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_brute_force() {
        let (w, h) = (13usize, 9usize);
        let mut src = vec![false; w * h];
        for &(x, y) in &[(0usize, 0usize), (7, 3), (12, 8), (4, 6)] {
            src[y * w + x] = true;
        }
        let got = squared_distance(w, h, &src);
        for y in 0..h {
            for x in 0..w {
                let best = src
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| **s)
                    .map(|(i, _)| {
                        let (sx, sy) = ((i % w) as f64, (i / w) as f64);
                        (sx - x as f64).powi(2) + (sy - y as f64).powi(2)
                    })
                    .fold(f64::INFINITY, f64::min);
                assert_eq!(got[y * w + x], best, "({x},{y})");
            }
        }
    }
}
