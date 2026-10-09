// Params of the POT Padding tab; names, enums and defaults mirror `PotPadParams`
// in crates/texopt-core/src/ops/pot_pad.rs.
// `color` is a '#rrggbbaa' string here (ParamForm color field) and is sent as [r, g, b, a].
import { z } from 'zod'

import { HEX_COLOR_PATTERN } from '../_imageOps/color'
import { ANCHOR_VALUES, MAX_SIDE } from '../_imageOps/options'

export const POT_TARGETS = ['nextPot', 'squarePot', 'fixed'] as const
export const PAD_FILLS = ['transparent', 'color', 'edgeExtend'] as const

export const potPadSchema = z.object({
  target: z.enum(POT_TARGETS).default('nextPot'),
  width: z.number().int().min(1).max(MAX_SIDE).default(1024),
  height: z.number().int().min(1).max(MAX_SIDE).default(1024),
  anchor: z.enum(ANCHOR_VALUES).default('center'),
  fill: z.enum(PAD_FILLS).default('transparent'),
  color: z.string().regex(HEX_COLOR_PATTERN).default('#000000ff'),
  minSize: z.number().int().min(0).max(MAX_SIDE).default(0),
  maxSize: z.number().int().min(0).max(MAX_SIDE).default(8192),
})

export type PotPadParams = z.infer<typeof potPadSchema>
