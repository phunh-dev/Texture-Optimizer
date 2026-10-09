import type { OpRequest } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { checkMaxDimension, invalidParam, scaleDimension, snapDimension, type SizeResult } from '../_imageOps/size'
import { resizeSchema, type ResizeParams } from './schema'

/** Session params → `{ kind: 'resize', params }` (null while params are invalid). */
export function buildResizeRequest(params: Params): OpRequest | null {
  const parsed = resizeSchema.safeParse(params)
  if (!parsed.success) return null
  const p = parsed.data
  return {
    kind: 'resize',
    params: {
      mode: p.mode,
      percent: p.percent,
      width: p.width,
      height: p.height,
      longestSide: p.longestSide,
      keepAspect: p.keepAspect,
      filter: p.filter,
      linearSpace: p.linearSpace,
      premultiplyAlpha: p.premultiplyAlpha,
      snap: p.snap,
    },
  }
}

/** Mirror of `resize::target_size` (crates/texopt-core/src/ops/resize.rs). */
export function predictResize(w: number, h: number, p: ResizeParams): SizeResult {
  const keep = p.keepAspect
  let tw: number
  let th: number
  switch (p.mode) {
    case 'percent': {
      if (!Number.isFinite(p.percent) || p.percent <= 0) return invalidParam('percent', 'must be > 0')
      tw = scaleDimension(w, p.percent / 100)
      th = scaleDimension(h, p.percent / 100)
      break
    }
    case 'exact': {
      if (p.width <= 0) return invalidParam('width', 'must be > 0')
      if (p.height <= 0) return invalidParam('height', 'must be > 0')
      if (keep) {
        const s = Math.min(p.width / w, p.height / h)
        tw = scaleDimension(w, s)
        th = scaleDimension(h, s)
      } else {
        tw = p.width
        th = p.height
      }
      break
    }
    case 'fitWidth': {
      if (p.width <= 0) return invalidParam('width', 'must be > 0')
      tw = p.width
      th = keep ? scaleDimension(h, p.width / w) : h
      break
    }
    case 'fitHeight': {
      if (p.height <= 0) return invalidParam('height', 'must be > 0')
      tw = keep ? scaleDimension(w, p.height / h) : w
      th = p.height
      break
    }
    case 'longestSide': {
      const l = p.longestSide
      if (l <= 0) return invalidParam('longestSide', 'must be > 0')
      const s = l / Math.max(w, h)
      if (keep) {
        tw = scaleDimension(w, s)
        th = scaleDimension(h, s)
      } else if (w >= h) {
        tw = l
        th = h
      } else {
        tw = w
        th = l
      }
      break
    }
  }
  return checkMaxDimension(snapDimension(tw, p.snap, 'nearest'), snapDimension(th, p.snap, 'nearest'))
}

/** Predicted size from raw session params (null when the params are invalid). */
export function predictResizeFromParams(w: number, h: number, params: Params): SizeResult | null {
  const parsed = resizeSchema.safeParse(params)
  return parsed.success ? predictResize(w, h, parsed.data) : null
}
