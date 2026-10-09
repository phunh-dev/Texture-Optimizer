import type { z } from 'zod'

import i18n from '@/i18n'

import type { Params } from '@/stores/session'

/** Inline error for one field: an i18n key plus interpolation values. */
export interface FieldError {
  key: string
  values: Record<string, unknown>
}

type Issue = z.core.$ZodIssue

function issueToError(issue: Issue): FieldError {
  // Custom messages written as i18n keys win: z.number().min(1, { error: 'resize:errors.min' }).
  if (issue.message && issue.message.includes(':') && i18n.exists(issue.message)) {
    return { key: issue.message, values: { ...issue } as Record<string, unknown> }
  }
  switch (issue.code) {
    case 'too_small':
      return { key: 'common:validation.tooSmall', values: { minimum: Number(issue.minimum) } }
    case 'too_big':
      return { key: 'common:validation.tooBig', values: { maximum: Number(issue.maximum) } }
    case 'invalid_type':
      return { key: issue.expected === 'int' ? 'common:validation.notInteger' : 'common:validation.invalidType', values: {} }
    case 'invalid_format':
      return { key: 'common:validation.invalidFormat', values: {} }
    default:
      return { key: 'common:validation.invalid', values: {} }
  }
}

/** Validates params; returns errors keyed by top-level param key ('' for form-level). */
export function validateParams(schema: z.ZodType | undefined, params: Params): Record<string, FieldError> {
  if (!schema) return {}
  const result = schema.safeParse(params)
  if (result.success) return {}
  const errors: Record<string, FieldError> = {}
  for (const issue of result.error.issues) {
    const key = issue.path.length ? String(issue.path[0]) : ''
    if (!errors[key]) errors[key] = issueToError(issue)
  }
  return errors
}

/** Default params from a zod object schema whose fields use .default(). */
export function schemaDefaults(schema: z.ZodType): Params {
  const result = schema.safeParse({})
  return result.success && result.data && typeof result.data === 'object' ? (result.data as Params) : {}
}
