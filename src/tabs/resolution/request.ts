import type { CompareRect } from '@/components/ToolLayout'
import type { OpRequest } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { hexToRgba } from '../_imageOps/color'
import {
  anchorOffset,
  checkMaxDimension,
  invalidParam,
  prevPot,
  roundPot,
  roundToMultiple,
  scaleDimension,
  type RoundMode,
  type SizeResult,
} from '../_imageOps/size'
import { resolutionSchema, type ResolutionParams } from './schema'

/** Session params → `{ kind: 'resolution', params }` (null while params are invalid). */
export function buildResolutionRequest(params: Params): OpRequest | null {
  const parsed = resolutionSchema.safeParse(params)
  if (!parsed.success) return null
  const p = parsed.data
  return {
    kind: 'resolution',
    params: {
      target: p.target,
      n: p.n,
      round: p.round,
      method: p.method,
      anchor: p.anchor,
      filter: p.filter,
      keepAspect: p.keepAspect,
      allowNonSquare: p.allowNonSquare,
      maxSize: p.maxSize,
      padColor: hexToRgba(p.padColor),
    },
  }
}

/** The rounding actually used: `round` only applies to resample; pad rounds up, crop down. */
export function effectiveRound(p: Pick<ResolutionParams, 'method' | 'round'>): RoundMode {
  if (p.method === 'pad') return 'up'
  if (p.method === 'crop') return 'down'
  return p.round
}

/** Mirror of `resolution::target_size` (crates/texopt-core/src/ops/resolution.rs). */
export function predictResolution(w: number, h: number, p: ResolutionParams): SizeResult {
  const round = effectiveRound(p)
  let multiple: number | null
  if (p.target === 'multipleOf4') multiple = 4
  else if (p.target === 'multipleOfN') {
    if (p.n === 0) return invalidParam('n', 'must be >= 1')
    multiple = p.n
  } else multiple = null
  const fix = (v: number) => (multiple != null ? roundToMultiple(v, multiple, round) : roundPot(v, round))
  let tw = fix(w)
  let th = fix(h)
  if (multiple == null && !p.allowNonSquare) {
    const s = Math.max(tw, th)
    tw = s
    th = s
  }
  if (p.maxSize > 0) {
    const cap = multiple != null ? Math.floor(p.maxSize / multiple) * multiple : prevPot(p.maxSize)
    if (cap === 0) return invalidParam('maxSize', 'smaller than the smallest valid size')
    tw = Math.min(tw, cap)
    th = Math.min(th, cap)
  }
  return checkMaxDimension(tw, th)
}

export function predictResolutionFromParams(w: number, h: number, params: Params): SizeResult | null {
  const parsed = resolutionSchema.safeParse(params)
  return parsed.success ? predictResolution(w, h, parsed.data) : null
}

/**
 * Where a `tw`×`th` result sits over the `w`×`h` original (original pixel coordinates), so
 * the comparison lines content up: pad/crop place the original 1:1 by anchor; resample
 * stretches it, or (keepAspect) scales it uniformly and anchors it inside the canvas.
 */
export function resolutionPlacement(w: number, h: number, tw: number, th: number, request: OpRequest): CompareRect {
  const rp = request.params as { method?: string; anchor?: ResolutionParams['anchor']; keepAspect?: boolean }
  const anchor = rp.anchor ?? 'center'
  if (rp.method === 'pad' || rp.method === 'crop') {
    const [ox, oy] = anchorOffset(anchor, tw, th, w, h)
    return { x: 0 - ox, y: 0 - oy, width: tw, height: th } // 0 - v avoids -0
  }
  if ((tw === w && th === h) || !rp.keepAspect) return { x: 0, y: 0, width: w, height: h }
  const s = Math.min(tw / w, th / h)
  const iw = Math.min(scaleDimension(w, s), tw)
  const ih = Math.min(scaleDimension(h, s), th)
  const [ox, oy] = anchorOffset(anchor, tw, th, iw, ih)
  const sx = w / iw
  const sy = h / ih
  return { x: 0 - ox * sx, y: 0 - oy * sy, width: tw * sx, height: th * sy }
}
