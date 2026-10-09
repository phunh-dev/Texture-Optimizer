// Params of the Resolution Fixer tab; names, enums and defaults mirror
// `ResolutionParams` in crates/texopt-core/src/ops/resolution.rs.
// `padColor` is a '#rrggbbaa' string here (ParamForm color field) and is sent as [r, g, b, a].
import { z } from 'zod'

import { HEX_COLOR_PATTERN } from '../_imageOps/color'
import { ANCHOR_VALUES, FILTERS, MAX_SIDE, ROUNDS } from '../_imageOps/options'

export const RESOLUTION_TARGETS = ['multipleOf4', 'multipleOfN', 'pot'] as const
export const RESOLUTION_METHODS = ['resample', 'pad', 'crop'] as const

export const resolutionSchema = z.object({
  target: z.enum(RESOLUTION_TARGETS).default('multipleOf4'),
  n: z.number().int().min(1).max(MAX_SIDE).default(8),
  round: z.enum(ROUNDS).default('nearest'),
  method: z.enum(RESOLUTION_METHODS).default('resample'),
  anchor: z.enum(ANCHOR_VALUES).default('center'),
  filter: z.enum(FILTERS).default('lanczos3'),
  keepAspect: z.boolean().default(false),
  allowNonSquare: z.boolean().default(true),
  maxSize: z.number().int().min(0).max(MAX_SIDE).default(8192),
  padColor: z.string().regex(HEX_COLOR_PATTERN).default('#00000000'),
})

export type ResolutionParams = z.infer<typeof resolutionSchema>
