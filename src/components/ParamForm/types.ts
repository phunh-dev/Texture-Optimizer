// Declarative field descriptors for ParamForm. Every visible string is an
// i18n key (e.g. 'resize:params.width.label'), never text.
import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'

import type { Params } from '@/stores/session'

/** A full i18n key including namespace, e.g. 'resize:params.mode.label'. */
export type I18nKey = string

export type VisibleIf = (params: Params) => boolean

interface FieldBase {
  /** Param key in the session's `params` object. */
  key: string
  labelKey: I18nKey
  /** Optional help text under the control. */
  descKey?: I18nKey
  visibleIf?: VisibleIf
  disabledIf?: VisibleIf
}

export interface NumberField extends FieldBase {
  kind: 'number'
  min?: number
  max?: number
  step?: number
  /** i18n key of the unit suffix, e.g. 'common:units.px'. */
  unitKey?: I18nKey
}

export interface SliderField extends FieldBase {
  kind: 'slider'
  min: number
  max: number
  step?: number
  unitKey?: I18nKey
}

export interface OptionDescriptor {
  value: string
  labelKey: I18nKey
  /** Optional icon (segmented control). */
  icon?: LucideIcon
}

export interface SelectField extends FieldBase {
  kind: 'select'
  options: OptionDescriptor[]
}

export interface SegmentedField extends FieldBase {
  kind: 'segmented'
  options: OptionDescriptor[]
}

/** Any subset of `options`; value is a string[] kept in option order. */
export interface MultiSelectField extends FieldBase {
  kind: 'multiSelect'
  options: OptionDescriptor[]
}

export interface SwitchField extends FieldBase {
  kind: 'switch'
}

/** Value is a hex string '#rrggbb' or '#rrggbbaa' (when `alpha`). */
export interface ColorField extends FieldBase {
  kind: 'color'
  alpha?: boolean
}

export interface TextField extends FieldBase {
  kind: 'text'
  placeholderKey?: I18nKey
  /** Use a monospace font (templates, regexes). */
  monospace?: boolean
}

export type Anchor = 'topLeft' | 'top' | 'topRight' | 'left' | 'center' | 'right' | 'bottomLeft' | 'bottom' | 'bottomRight'
export const ANCHORS: readonly Anchor[] = ['topLeft', 'top', 'topRight', 'left', 'center', 'right', 'bottomLeft', 'bottom', 'bottomRight']

/** 3×3 anchor picker; value is one of ANCHORS. */
export interface AnchorField extends FieldBase {
  kind: 'anchor'
}

/** Width × height pair; value is `{ width: number, height: number }`. */
export interface SizeField extends FieldBase {
  kind: 'size'
  min?: number
  max?: number
  step?: number
  unitKey?: I18nKey
  /** Optional boolean param that stores the link (keep ratio) toggle. Local state otherwise. */
  linkKey?: string
}

/** Props handed to a custom field renderer (same binding as built-in controls). */
export interface CustomFieldProps {
  tabId: string
  /** DOM id for the control (the label's htmlFor points at it). */
  id: string
  value: unknown
  params: Params
  disabled: boolean
  invalid: boolean
  describedBy?: string
  /** `coalesce` merges continuous edits into one undo step. */
  onChange: (value: unknown, options?: { coalesce?: boolean }) => void
  onCommit: () => void
}

/**
 * Tool-specific control rendered by the tool itself (e.g. a rule list or an
 * eyedropper). Without `labelKey` it is rendered bare (no label / description shell).
 */
export interface CustomField extends Omit<FieldBase, 'labelKey'> {
  kind: 'custom'
  labelKey?: I18nKey
  render: (props: CustomFieldProps) => ReactNode
}

export interface GroupField {
  kind: 'group'
  /** Stable id (used for the collapsed state). */
  id: string
  labelKey: I18nKey
  descKey?: I18nKey
  fields: FieldDescriptor[]
  collapsible?: boolean
  defaultOpen?: boolean
  visibleIf?: VisibleIf
}

export type FieldDescriptor =
  | NumberField
  | SliderField
  | SelectField
  | SegmentedField
  | MultiSelectField
  | SwitchField
  | ColorField
  | TextField
  | AnchorField
  | SizeField
  | CustomField
  | GroupField

export interface SizeValue {
  width: number
  height: number
}

/** Identity helper that gives descriptor arrays full type checking. */
export function defineFields(fields: FieldDescriptor[]): FieldDescriptor[] {
  return fields
}
