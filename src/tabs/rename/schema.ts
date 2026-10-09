// zod schema of the Pattern Renamer params (inline validation; Rename is
// disabled while invalid). Shapes, defaults and request mapping live in ./defaults.
import { z } from 'zod'

import { CASE_MODES, ENGINE_PRESETS, EXECUTE_MODES, EXTENSION_CASES, INVALID_NAME_CHARS, MAX_ZERO_PAD, SORT_BY, TEXTURE_TYPES, unknownTokens } from './defaults'

export * from './defaults'

const int = z.number().int()

export const renameSchema = z
  .object({
    template: z.string().default('{name}'),
    prefix: z.string().default(''),
    suffix: z.string().default(''),
    startNumber: int.default(1),
    step: int.default(1),
    zeroPad: int.min(0).max(MAX_ZERO_PAD).default(0),
    case: z.enum(CASE_MODES).default('keep'),
    findReplace: z
      .array(z.object({ find: z.string(), replace: z.string(), regex: z.boolean(), caseSensitive: z.boolean() }))
      .default([]),
    smart: z
      .object({
        enabled: z.boolean(),
        preset: z.enum(ENGINE_PRESETS),
        customMap: z.partialRecord(z.enum(TEXTURE_TYPES), z.string()),
      })
      .default({ enabled: false, preset: 'unreal', customMap: {} }),
    sortBy: z.enum(SORT_BY).default('none'),
    sortDesc: z.boolean().default(false),
    keepExtension: z.boolean().default(true),
    extensionCase: z.enum(EXTENSION_CASES).default('keep'),
    dateFormat: z.string().min(1).default('%Y%m%d'),
    sanitize: z.boolean().default(false),
    invalidCharReplacement: z.string().default('_'),
    mode: z.enum(EXECUTE_MODES).default('inPlace'),
    copyDir: z.string().default(''),
  })
  .superRefine((p, ctx) => {
    // Live template validation (the backend reports RENAME_UNKNOWN_TOKEN too).
    const unknown = unknownTokens(p.template)
    if (unknown.length > 0) {
      ctx.addIssue({ code: 'custom', path: ['template'], message: 'rename:validation.unknownToken', params: { tokens: unknown.join(', ') } })
    }
    // Rust only validates the replacement when sanitizing.
    if (p.sanitize && INVALID_NAME_CHARS.test(p.invalidCharReplacement)) {
      ctx.addIssue({ code: 'custom', path: ['invalidCharReplacement'], message: 'rename:validation.invalidReplacement' })
    }
  })
