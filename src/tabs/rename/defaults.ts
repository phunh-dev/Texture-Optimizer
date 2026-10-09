// Pattern Renamer params shape, defaults and request mapping. Kept free of zod
// so the tool registry (main bundle) can import the defaults cheaply.
// The Rust fields mirror `RenameParams` in crates/texopt-core/src/rename/mod.rs
// (serde camelCase); `mode` and `copyDir` are UI-only and become the separate
// `ExecuteMode` argument.
import type { Params } from '@/stores/session'

export const TOKENS = ['name', 'index', 'parent', 'width', 'height', 'ext', 'date', 'type'] as const
export type Token = (typeof TOKENS)[number]

export const CASE_MODES = ['keep', 'lower', 'upper', 'snake', 'kebab', 'camel', 'pascal'] as const
export const SORT_BY = ['none', 'name', 'naturalName', 'modified', 'size', 'dimensions'] as const
export const EXTENSION_CASES = ['keep', 'lower', 'upper'] as const
export const ENGINE_PRESETS = ['unreal', 'unity', 'godot', 'custom'] as const
export const TEXTURE_TYPES = ['baseColor', 'normal', 'roughness', 'metallic', 'ao', 'emissive', 'height', 'mask', 'orm', 'opacity'] as const
export const EXECUTE_MODES = ['inPlace', 'copyTo'] as const

export type TextureType = (typeof TEXTURE_TYPES)[number]
export const MAX_ZERO_PAD = 32
/** Characters forbidden in file names on Windows/macOS/Linux (same list as Rust). */
export const INVALID_NAME_CHARS = /[<>:"/\\|?*\p{Cc}]/u

export interface FindReplaceRule {
  find: string
  replace: string
  regex: boolean
  caseSensitive: boolean
}

export interface SmartParams {
  enabled: boolean
  preset: (typeof ENGINE_PRESETS)[number]
  customMap: Partial<Record<TextureType, string>>
}

/** Exactly the Rust `RenameParams` JSON. */
export interface RenameParams {
  template: string
  prefix: string
  suffix: string
  startNumber: number
  step: number
  zeroPad: number
  case: (typeof CASE_MODES)[number]
  findReplace: FindReplaceRule[]
  smart: SmartParams
  sortBy: (typeof SORT_BY)[number]
  sortDesc: boolean
  keepExtension: boolean
  extensionCase: (typeof EXTENSION_CASES)[number]
  dateFormat: string
  sanitize: boolean
  invalidCharReplacement: string
}

/** `{"kind":"inPlace"}` or `{"kind":"copyTo","dir":"..."}` (Rust `ExecuteMode`). */
export type ExecuteMode = { kind: 'inPlace' } | { kind: 'copyTo'; dir: string }

/** Same values as `impl Default for RenameParams` in Rust. */
export const RENAME_DEFAULTS: RenameParams = {
  template: '{name}',
  prefix: '',
  suffix: '',
  startNumber: 1,
  step: 1,
  zeroPad: 0,
  case: 'keep',
  findReplace: [],
  smart: { enabled: false, preset: 'unreal', customMap: {} },
  sortBy: 'none',
  sortDesc: false,
  keepExtension: true,
  extensionCase: 'keep',
  dateFormat: '%Y%m%d',
  sanitize: false,
  invalidCharReplacement: '_',
}

export const NEW_RULE: FindReplaceRule = { find: '', replace: '', regex: false, caseSensitive: true }

/** Unknown `{tokens}` in a template (same grammar as the Rust parser). */
export function unknownTokens(template: string): string[] {
  const out: string[] = []
  for (const m of template.matchAll(/\{([A-Za-z0-9_]+)\}/g)) {
    if (!(TOKENS as readonly string[]).includes(m[1]) && !out.includes(m[0])) out.push(m[0])
  }
  return out
}

/** Session params of a new Renamer tab: the Rust defaults plus the UI-only destination. */
export const defaultRenameParams = (): Params =>
  ({ ...structuredClone(RENAME_DEFAULTS), mode: 'inPlace', copyDir: '' }) as unknown as Params

const KEYS = Object.keys(RENAME_DEFAULTS) as (keyof RenameParams)[]

/**
 * Picks exactly the Rust fields from the session params (UI-only keys dropped)
 * and leaves out empty custom-map entries (an empty suffix would produce `Name_`).
 */
export function toRenameParams(params: Params): RenameParams {
  const out: Record<string, unknown> = {}
  for (const k of KEYS) out[k] = structuredClone(k in params ? params[k] : RENAME_DEFAULTS[k])
  const smart = out.smart as SmartParams
  smart.customMap = Object.fromEntries(Object.entries(smart.customMap ?? {}).filter(([, v]) => typeof v === 'string' && v.trim() !== ''))
  return out as unknown as RenameParams
}

export function toExecuteMode(params: Params): ExecuteMode {
  return params.mode === 'copyTo' ? { kind: 'copyTo', dir: typeof params.copyDir === 'string' ? params.copyDir : '' } : { kind: 'inPlace' }
}
