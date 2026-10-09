// Background Remover params. The shape mirrors `BgRemoveParams` in
// crates/texopt-core/src/ops/bg_remove/mod.rs (serde camelCase) exactly, so
// the session params are sent as-is in the `bgRemove` op request.
import { z } from 'zod'

import type { OpRequest } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { BG_MODES, BG_REMOVE_DEFAULTS, defaultBgRemoveParams, FILL_MODES, METRICS, type BgRemoveParams, type Rgba } from './defaults'

export * from './defaults'

export const MAX_FEATHER = 64
export const MAX_CELL_SIZE = 512

const byte = z.number().int().min(0).max(255)

export const bgRemoveSchema = z.object({
  mode: z.enum(BG_MODES).default('auto'),
  color: z.tuple([byte, byte, byte, byte]).default([255, 255, 255, 255]),
  checkerCellSize: z.number().int().min(1).max(MAX_CELL_SIZE).nullable().default(null),
  fill: z.enum(FILL_MODES).default('floodFromEdges'),
  tolerance: z.number().min(0).max(100).default(10),
  metric: z.enum(METRICS).default('rgb'),
  feather: z.number().int().min(0).max(MAX_FEATHER).default(0),
  defringe: z.boolean().default(false),
  defringeStrength: z.number().min(0).max(100).default(100),
})

const KEYS = Object.keys(BG_REMOVE_DEFAULTS) as (keyof BgRemoveParams)[]

/** Picks exactly the Rust fields (in Rust order) from the session params. */
export function toBgRemoveParams(params: Params): BgRemoveParams {
  const out: Record<string, unknown> = {}
  for (const k of KEYS) out[k] = k in params ? params[k] : BG_REMOVE_DEFAULTS[k]
  return out as unknown as BgRemoveParams
}

export function buildBgRemoveRequest(params: Params): OpRequest {
  return { kind: 'bgRemove', params: toBgRemoveParams(params) as unknown as Record<string, unknown> }
}

export type QuickPresetId = 'white' | 'checker' | 'auto'

/** One-click starting points; applied on top of the defaults. */
export const QUICK_PRESETS: Record<QuickPresetId, Partial<BgRemoveParams>> = {
  white: { mode: 'white', fill: 'floodFromEdges', tolerance: 8, defringe: true, defringeStrength: 100 },
  checker: { mode: 'checker', checkerCellSize: null, fill: 'floodFromEdges', tolerance: 6 },
  auto: { mode: 'auto' },
}

export function quickPresetParams(id: QuickPresetId): Params {
  return { ...defaultBgRemoveParams(), ...structuredClone(QUICK_PRESETS[id]) } as Params
}

export const toHex = (c: readonly number[]): string =>
  `#${c
    .slice(0, 3)
    .map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0'))
    .join('')}`

/** '#rrggbb' -> [r, g, b, 255]; null when malformed. */
export function fromHex(hex: string): Rgba | null {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim())
  if (!m) return null
  return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16), 255]
}
