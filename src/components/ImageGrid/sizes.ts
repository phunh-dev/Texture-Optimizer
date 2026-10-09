import type { ViewSize } from '@/stores/session'

export interface GridMetrics {
  /** Cell (and square thumbnail) width in px. */
  cell: number
  /** Caption height under the thumbnail. */
  caption: number
  gap: number
}

export const GRID_SIZES: Record<ViewSize, GridMetrics> = {
  small: { cell: 96, caption: 22, gap: 8 },
  medium: { cell: 160, caption: 42, gap: 12 },
  large: { cell: 256, caption: 42, gap: 16 },
}

export const GRID_PADDING = 16

/** Number of columns that fit in `width` (always at least 1). */
export function columnCount(width: number, size: ViewSize): number {
  const { cell, gap } = GRID_SIZES[size]
  const available = width - GRID_PADDING * 2
  return Math.max(1, Math.floor((available + gap) / (cell + gap)))
}
