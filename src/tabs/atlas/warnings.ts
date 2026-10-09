// Translation of atlas warnings: ids carried in params (exporter, feature,
// layout reset reason) are replaced by their localized labels first.
import i18n from '@/i18n'
import { tKey } from '@/i18n/loose'
import { toAppError, translateError } from '@/lib/errors'

const LOOKUPS: Record<string, string> = {
  exporter: 'atlas:exporter.kinds',
  feature: 'atlas:features',
}

export function translateAtlasWarning(err: unknown): string {
  const e = toAppError(err)
  const params: Record<string, unknown> = { ...e.params }
  for (const [param, prefix] of Object.entries(LOOKUPS)) {
    const v = params[param]
    if (typeof v === 'string' && i18n.exists(`${prefix}.${v}`)) params[param] = tKey(`${prefix}.${v}`)
  }
  if (e.code === 'ATLAS_LAYOUT_RESET' && typeof params.reason === 'string' && i18n.exists(`atlas:layoutReset.${params.reason}`)) {
    params.reason = tKey(`atlas:layoutReset.${params.reason}`)
  }
  return translateError({ code: e.code, params })
}
