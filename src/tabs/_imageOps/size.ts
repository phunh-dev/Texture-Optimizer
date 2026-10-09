// TS mirror of the size math in `crates/texopt-core/src/ops/common/mod.rs`.
// Used to predict output sizes instantly (no IPC) for every file in a tab.
// Keep in sync with Rust; `size.test.ts` replays the Rust test tables.
import type { Anchor } from '@/components/ParamForm'
import type { AppError } from '@/lib/ipc/types'

/** Hard cap for any output side (`MAX_DIMENSION`). */
export const MAX_DIMENSION = 32768

const U32_MAX = 0xffffffff
const POT_MAX = 2 ** 31

export type RoundMode = 'nearest' | 'up' | 'down'
export type SnapMode = 'none' | 'multipleOf4' | 'pot'

export interface Size {
  width: number
  height: number
}

/** Predicted output size, or the error the backend would report. */
export type SizeResult = ({ ok: true } & Size) | { ok: false; error: AppError }

export const ok = (width: number, height: number): SizeResult => ({ ok: true, width, height })
export const fail = (code: string, params: Record<string, unknown>): SizeResult => ({ ok: false, error: { code, params } })
export const invalidParam = (param: string, reason: string): SizeResult => fail('INVALID_PARAMS', { param, reason })
export const tooLarge = (max: number, width: number, height: number): SizeResult => fail('IMG_TOO_LARGE', { max, width, height })

export function isPot(n: number): boolean {
  return Number.isInteger(n) && n > 0 && (n & (n - 1)) === 0 // n <= 2^31 in practice
}

/** Smallest power of two >= n (nextPot(0) = 1); saturates at 2^31. */
export function nextPot(n: number): number {
  const v = Math.max(1, Math.floor(n))
  if (v > POT_MAX) return POT_MAX
  let p = 1
  while (p < v) p *= 2
  return p
}

/** Largest power of two <= n (prevPot(0) = 1, never 0). */
export function prevPot(n: number): number {
  if (n <= 1) return 1
  let p = 1
  while (p * 2 <= n) p *= 2
  return p
}

/** Round to a power of two; ties round up; never 0. */
export function roundPot(n: number, mode: RoundMode): number {
  if (mode === 'up') return nextPot(n)
  if (mode === 'down') return prevPot(n)
  const lo = prevPot(n)
  const hi = nextPot(n)
  if (lo >= n || hi <= lo) return lo
  return n - lo < hi - n ? lo : hi
}

/** Round n to a multiple of m (m = 0 → 1). Ties round up; never below m. */
export function roundToMultiple(n: number, m: number, mode: RoundMode): number {
  const mm = Math.max(1, m)
  const down = Math.floor(n / mm) * mm
  const up = down === n ? n : down + mm
  let r: number
  if (mode === 'down') r = down
  else if (mode === 'up') r = up
  else r = n - down < up - n ? down : up
  const maxMultiple = Math.floor(U32_MAX / mm) * mm
  return Math.min(Math.max(r, mm), maxMultiple)
}

export function snapDimension(n: number, snap: SnapMode, round: RoundMode): number {
  if (snap === 'multipleOf4') return roundToMultiple(n, 4, round)
  if (snap === 'pot') return roundPot(n, round)
  return Math.max(1, n)
}

/** round(n * factor), at least 1, saturating at u32::MAX. */
export function scaleDimension(n: number, factor: number): number {
  // Math.round equals Rust's f64::round (half away from zero) for non-negative values.
  const v = Math.round(n * factor)
  if (Number.isNaN(v) || v < 1) return 1
  if (v >= U32_MAX) return U32_MAX
  return v
}

/** IMG_TOO_LARGE when a side exceeds MAX_DIMENSION. */
export function checkMaxDimension(width: number, height: number): SizeResult {
  return width > MAX_DIMENSION || height > MAX_DIMENSION ? tooLarge(MAX_DIMENSION, width, height) : ok(width, height)
}

const FACTORS: Record<Anchor, [number, number]> = {
  topLeft: [0, 0],
  top: [1, 0],
  topRight: [2, 0],
  left: [0, 1],
  center: [1, 1],
  right: [2, 1],
  bottomLeft: [0, 2],
  bottom: [1, 2],
  bottomRight: [2, 2],
}

/**
 * Position of an inner box inside an outer box (Rust `anchor_offset`): positive = padding,
 * negative = cropping. Centering truncates toward zero.
 */
export function anchorOffset(anchor: Anchor, outerW: number, outerH: number, innerW: number, innerH: number): [number, number] {
  const [fx, fy] = FACTORS[anchor] ?? FACTORS.center
  const axis = (f: number, outer: number, inner: number) => {
    const diff = outer - inner
    if (f === 0) return 0
    if (f === 1) return Math.trunc(diff / 2)
    return diff
  }
  return [axis(fx, outerW, innerW), axis(fy, outerH, innerH)]
}
