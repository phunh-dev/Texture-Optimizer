// Option lists shared by the image-op tabs (labels in the `resize:imageOps` namespace).
import type { OptionDescriptor } from '@/components/ParamForm'

export const FILTERS = ['nearest', 'bilinear', 'catmullRom', 'mitchell', 'lanczos3'] as const
export type Filter = (typeof FILTERS)[number]

export const SNAPS = ['none', 'multipleOf4', 'pot'] as const

export const ROUNDS = ['nearest', 'up', 'down'] as const

export const ANCHOR_VALUES = ['topLeft', 'top', 'topRight', 'left', 'center', 'right', 'bottomLeft', 'bottom', 'bottomRight'] as const

export const filterOptions: OptionDescriptor[] = FILTERS.map((value) => ({ value, labelKey: `resize:imageOps.filters.${value}` }))

export const snapOptions: OptionDescriptor[] = SNAPS.map((value) => ({ value, labelKey: `resize:imageOps.snaps.${value}` }))

export const roundOptions: OptionDescriptor[] = ROUNDS.map((value) => ({ value, labelKey: `resize:imageOps.rounds.${value}` }))

/** Upper bound for any side typed by the user (Rust MAX_DIMENSION). */
export const MAX_SIDE = 32768
