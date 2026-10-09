import type { CompareRect } from '@/components/ToolLayout'
import type { OpRequest } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { hexToRgba } from '../_imageOps/color'
import { anchorOffset, checkMaxDimension, invalidParam, nextPot, tooLarge, type SizeResult } from '../_imageOps/size'
import { potPadSchema, type PotPadParams } from './schema'

/** Session params → `{ kind: 'potPad', params }` (null while params are invalid). */
export function buildPotPadRequest(params: Params): OpRequest | null {
  const parsed = potPadSchema.safeParse(params)
  if (!parsed.success) return null
  const p = parsed.data
  return {
    kind: 'potPad',
    params: {
      target: p.target,
      width: p.width,
      height: p.height,
      anchor: p.anchor,
      fill: p.fill,
      color: hexToRgba(p.color),
      minSize: p.minSize,
      maxSize: p.maxSize,
    },
  }
}

/** Mirror of `pot_pad::target_size` (crates/texopt-core/src/ops/pot_pad.rs). */
export function predictPotPad(w: number, h: number, p: PotPadParams): SizeResult {
  const min = p.minSize
  let tw: number
  let th: number
  if (p.target === 'nextPot') {
    tw = nextPot(Math.max(w, min))
    th = nextPot(Math.max(h, min))
  } else if (p.target === 'squarePot') {
    tw = th = nextPot(Math.max(w, h, min))
  } else {
    if (p.width === 0) return invalidParam('width', 'must be > 0')
    if (p.height === 0) return invalidParam('height', 'must be > 0')
    if (w > p.width || h > p.height) return tooLarge(w > p.width ? p.width : p.height, w, h)
    tw = p.width
    th = p.height
  }
  if (p.maxSize > 0 && (tw > p.maxSize || th > p.maxSize)) return tooLarge(p.maxSize, tw, th)
  return checkMaxDimension(tw, th)
}

export function predictPotPadFromParams(w: number, h: number, params: Params): SizeResult | null {
  const parsed = potPadSchema.safeParse(params)
  return parsed.success ? predictPotPad(w, h, parsed.data) : null
}

/** The original is copied 1:1 into the canvas at its anchor offset. */
export function potPadPlacement(w: number, h: number, tw: number, th: number, request: OpRequest): CompareRect {
  const anchor = ((request.params as { anchor?: PotPadParams['anchor'] }).anchor ?? 'center') as PotPadParams['anchor']
  const [ox, oy] = anchorOffset(anchor, tw, th, w, h)
  return { x: 0 - ox, y: 0 - oy, width: tw, height: th } // 0 - v avoids -0
}
