import type { CompareRect } from '@/components/ToolLayout'
import type { OpRequest, OutputSettings } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { trimSchema, type TrimMeta } from './schema'

/** Session params → `{ kind: 'trim', params }` (null while params are invalid). */
export function buildTrimRequest(params: Params): OpRequest | null {
  const parsed = trimSchema.safeParse(params)
  if (!parsed.success) return null
  const p = parsed.data
  return {
    kind: 'trim',
    params: {
      alphaThreshold: p.alphaThreshold,
      margin: p.margin,
      trimLeft: p.trimLeft,
      trimRight: p.trimRight,
      trimTop: p.trimTop,
      trimBottom: p.trimBottom,
      snap: p.snap,
      emptyBehavior: p.emptyBehavior,
    },
  }
}

/** Output settings for a trim run: "Write offsets JSON" turns on the `<output>.json` sidecar. */
export function trimOutputSettings(output: OutputSettings, params: Params): OutputSettings {
  return { ...output, writeMeta: params.writeOffsets === true }
}

/** The trimmed result sits at `trimRect` over the original (x / y may be negative). */
export function trimPlacement(meta: TrimMeta): CompareRect {
  const r = meta.trimRect
  return { x: r.x, y: r.y, width: r.w, height: r.h }
}
