import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

/** Merge Tailwind class names, later classes win (shadcn convention). */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs))
}

export function isPowerOfTwo(n: number): boolean {
  return Number.isInteger(n) && n > 0 && (n & (n - 1)) === 0
}

export function isMultipleOf(n: number, m: number): boolean {
  return Number.isInteger(n) && n % m === 0
}

/** Warnings shown as badges on an image cell. */
export interface DimensionWarnings {
  nonPot: boolean
  notMultipleOf4: boolean
}

export function dimensionWarnings(width: number, height: number): DimensionWarnings {
  return {
    nonPot: !(isPowerOfTwo(width) && isPowerOfTwo(height)),
    notMultipleOf4: !(isMultipleOf(width, 4) && isMultipleOf(height, 4)),
  }
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

/** Locale-independent short byte size, e.g. "1.2 MB". */
export function formatBytes(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${unit === 0 ? value : value.toFixed(1)} ${units[unit]}`
}

let idCounter = 0
/** Unique id for tabs and other client-side entities. */
export function createId(prefix = 'id'): string {
  idCounter++
  const random =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID().slice(0, 8)
      : Math.random().toString(36).slice(2, 10)
  return `${prefix}-${idCounter.toString(36)}-${random}`
}

/** True when keyboard focus is inside a text-editable element (native undo must win). */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  const tag = target.tagName
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (tag === 'INPUT') {
    const type = (target as HTMLInputElement).type
    return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file'].includes(type)
  }
  return false
}

/** Platform modifier: Cmd on macOS, Ctrl elsewhere (either is accepted). */
export function hasModifier(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return e.ctrlKey || e.metaKey
}
