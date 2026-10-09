// Params of the Resize tab; names, enums and defaults mirror `ResizeParams`
// in crates/texopt-core/src/ops/resize.rs.
import { z } from 'zod'

import { FILTERS, MAX_SIDE, SNAPS } from '../_imageOps/options'

export const RESIZE_MODES = ['percent', 'exact', 'fitWidth', 'fitHeight', 'longestSide'] as const
export type ResizeMode = (typeof RESIZE_MODES)[number]

const side = (fallback: number) => z.number().int().min(1).max(MAX_SIDE).default(fallback)

export const resizeSchema = z.object({
  mode: z.enum(RESIZE_MODES).default('percent'),
  percent: z.number().gt(0).max(10000).default(50),
  width: side(1024),
  height: side(1024),
  longestSide: side(1024),
  keepAspect: z.boolean().default(true),
  filter: z.enum(FILTERS).default('lanczos3'),
  linearSpace: z.boolean().default(false),
  premultiplyAlpha: z.boolean().default(true),
  snap: z.enum(SNAPS).default('none'),
})

export type ResizeParams = z.infer<typeof resizeSchema>
