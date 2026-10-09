// ParamForm color fields store '#rrggbb' / '#rrggbbaa'; Rust `Color` is `[r, g, b, a]`.

export type Rgba = [number, number, number, number]

const HEX = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})?$/i

/** '#rrggbb' (alpha 255) or '#rrggbbaa' → [r, g, b, a]. Invalid input → `fallback`. */
export function hexToRgba(hex: unknown, fallback: Rgba = [0, 0, 0, 0]): Rgba {
  const m = typeof hex === 'string' ? HEX.exec(hex.trim()) : null
  if (!m) return [...fallback] as Rgba
  return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16), m[4] ? parseInt(m[4], 16) : 255]
}

/** [r, g, b, a] → '#rrggbbaa' (lowercase). */
export function rgbaToHex([r, g, b, a]: Rgba): string {
  return `#${[r, g, b, a].map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0')).join('')}`
}

export const HEX_COLOR_PATTERN = /^#[0-9a-f]{6}([0-9a-f]{2})?$/i
