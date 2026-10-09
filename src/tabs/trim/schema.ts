// Params of the Sprite Trimmer tab; names, enums and defaults mirror `TrimParams`
// in crates/texopt-core/src/ops/trim.rs. `writeOffsets` is UI-only: it maps to the
// output setting `writeMeta` (trim offsets JSON written next to each output).
import { z } from 'zod'

import { MAX_SIDE, SNAPS } from '../_imageOps/options'

export const TRIM_EMPTY_BEHAVIORS = ['error', 'onePixel'] as const

export const trimSchema = z.object({
  alphaThreshold: z.number().int().min(0).max(255).default(0),
  margin: z.number().int().min(0).max(MAX_SIDE).default(0),
  trimLeft: z.boolean().default(true),
  trimRight: z.boolean().default(true),
  trimTop: z.boolean().default(true),
  trimBottom: z.boolean().default(true),
  snap: z.enum(SNAPS).default('none'),
  emptyBehavior: z.enum(TRIM_EMPTY_BEHAVIORS).default('error'),
  writeOffsets: z.boolean().default(false),
})

export type TrimParams = z.infer<typeof trimSchema>

/** Op metadata returned by the trim op (`TrimMeta`). x / y may be negative (snap grows the rect). */
export interface TrimMeta {
  sourceSize: { w: number; h: number }
  trimRect: { x: number; y: number; w: number; h: number }
}

export function isTrimMeta(v: unknown): v is TrimMeta {
  const m = v as TrimMeta | null
  return (
    !!m &&
    typeof m === 'object' &&
    typeof m.trimRect?.x === 'number' &&
    typeof m.trimRect?.y === 'number' &&
    typeof m.trimRect?.w === 'number' &&
    typeof m.trimRect?.h === 'number'
  )
}
