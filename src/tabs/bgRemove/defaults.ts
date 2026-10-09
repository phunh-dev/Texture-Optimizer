// Background Remover params shape + defaults. Kept free of zod so the tool
// registry (main bundle) can import the defaults without pulling the schema.
// The shape mirrors `BgRemoveParams` in crates/texopt-core/src/ops/bg_remove/mod.rs.
import type { Params } from '@/stores/session'

export const BG_MODES = ['auto', 'white', 'checker', 'color'] as const
export const FILL_MODES = ['floodFromEdges', 'global'] as const
export const METRICS = ['rgb', 'lab'] as const

export type BgMode = (typeof BG_MODES)[number]
/** Straight-alpha RGBA, `[r, g, b, a]` (alpha is ignored by the remover). */
export type Rgba = [number, number, number, number]

export interface BgRemoveParams {
  mode: BgMode
  color: Rgba
  /** null = auto-detect the checker cell size. */
  checkerCellSize: number | null
  fill: (typeof FILL_MODES)[number]
  tolerance: number
  metric: (typeof METRICS)[number]
  feather: number
  defringe: boolean
  defringeStrength: number
}

/** Same values as `impl Default for BgRemoveParams` in Rust. */
export const BG_REMOVE_DEFAULTS: BgRemoveParams = {
  mode: 'auto',
  color: [255, 255, 255, 255],
  checkerCellSize: null,
  fill: 'floodFromEdges',
  tolerance: 10,
  metric: 'rgb',
  feather: 0,
  defringe: false,
  defringeStrength: 100,
}

export const defaultBgRemoveParams = (): Params => structuredClone(BG_REMOVE_DEFAULTS) as unknown as Params
