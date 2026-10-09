import i18n from '@/i18n'
import type { AppError } from '@/lib/ipc/types'

/** Normalises anything thrown by a command into an AppError. */
export function toAppError(err: unknown): AppError {
  if (err && typeof err === 'object' && 'code' in err && typeof (err as AppError).code === 'string') {
    const e = err as AppError
    return { code: e.code, params: e.params ?? {} }
  }
  return { code: 'UNKNOWN', params: { detail: String(err) } }
}

/** Translates an AppError through the `errors` namespace, falling back to errors:UNKNOWN. */
export function translateError(err: unknown): string {
  const { code, params } = toAppError(err)
  const key = `errors:${code}`
  const t = i18n.t as unknown as (k: string, o?: Record<string, unknown>) => string
  return i18n.exists(key) ? t(key, params) : t('errors:UNKNOWN', params)
}
