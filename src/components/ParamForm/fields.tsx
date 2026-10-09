import { Link2Icon, Unlink2Icon } from 'lucide-react'
import { useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Input } from '@/components/ui/input'
import { NumberInput } from '@/components/ui/number-input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Tooltip } from '@/components/ui/tooltip'
import { useLooseT } from '@/i18n/loose'
import { clamp, cn } from '@/lib/utils'

import {
  ANCHORS,
  type Anchor,
  type AnchorField,
  type ColorField,
  type MultiSelectField,
  type NumberField,
  type SegmentedField,
  type SelectField,
  type SizeField,
  type SizeValue,
  type SliderField,
  type TextField,
} from './types'

export interface FieldProps<F, V> {
  field: F
  id: string
  value: V
  disabled: boolean
  invalid: boolean
  describedBy?: string
  /** `coalesce` merges continuous edits into one undo step. */
  onChange: (value: V, options?: { coalesce?: boolean }) => void
  /** Ends a coalesced edit (pointer up / blur). */
  onCommit: () => void
}

const num = (v: unknown, fallback = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : fallback)

export function NumberFieldControl({ field, id, value, disabled, invalid, describedBy, onChange, onCommit }: FieldProps<NumberField, unknown>) {
  const t = useLooseT()
  return (
    <NumberInput
      id={id}
      value={num(value, NaN)}
      min={field.min}
      max={field.max}
      step={field.step}
      unit={field.unitKey ? t(field.unitKey) : undefined}
      disabled={disabled}
      aria-invalid={invalid}
      aria-describedby={describedBy}
      onValueChange={(v) => onChange(v, { coalesce: true })}
      onCommit={onCommit}
    />
  )
}

export function SliderFieldControl({ field, id, value, disabled, invalid, describedBy, onChange, onCommit }: FieldProps<SliderField, unknown>) {
  const t = useLooseT()
  const current = num(value, field.min)
  const label = t(field.labelKey)
  return (
    <div className="flex items-center gap-3">
      <Slider
        aria-label={label}
        min={field.min}
        max={field.max}
        step={field.step ?? 1}
        value={[clamp(current, field.min, field.max)]}
        disabled={disabled}
        onValueChange={([v]) => onChange(v, { coalesce: true })}
        onValueCommit={onCommit}
        onPointerUp={onCommit}
        className="flex-1"
      />
      <NumberInput
        id={id}
        className="w-24 shrink-0"
        value={current}
        min={field.min}
        max={field.max}
        step={field.step ?? 1}
        unit={field.unitKey ? t(field.unitKey) : undefined}
        disabled={disabled}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        hideSteppers
        onValueChange={(v) => onChange(v, { coalesce: true })}
        onCommit={onCommit}
      />
    </div>
  )
}

export function SelectFieldControl({ field, id, value, disabled, invalid, describedBy, onChange }: FieldProps<SelectField, unknown>) {
  const t = useLooseT()
  return (
    <Select value={typeof value === 'string' ? value : undefined} onValueChange={(v) => onChange(v)} disabled={disabled}>
      <SelectTrigger id={id} size="sm" aria-invalid={invalid} aria-describedby={describedBy}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {field.options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {t(o.labelKey)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

export function SegmentedFieldControl({ field, id, value, disabled, describedBy, onChange }: FieldProps<SegmentedField, unknown>) {
  const t = useLooseT()
  return (
    <ToggleGroup
      id={id}
      type="single"
      className="flex w-full"
      value={typeof value === 'string' ? value : ''}
      disabled={disabled}
      aria-label={t(field.labelKey)}
      aria-describedby={describedBy}
      onValueChange={(v) => {
        if (v) onChange(v)
      }}
    >
      {field.options.map(({ value: v, labelKey, icon: Icon }) => (
        <ToggleGroupItem key={v} value={v} aria-label={t(labelKey)}>
          {Icon ? <Icon /> : null}
          <span className="truncate">{t(labelKey)}</span>
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}

export function MultiSelectFieldControl({ field, id, value, disabled, describedBy, onChange }: FieldProps<MultiSelectField, unknown>) {
  const t = useLooseT()
  const selected = Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : []
  return (
    <ToggleGroup
      id={id}
      type="multiple"
      className="flex w-full flex-wrap"
      value={selected}
      disabled={disabled}
      aria-label={t(field.labelKey)}
      aria-describedby={describedBy}
      onValueChange={(v: string[]) => {
        const set = new Set(v)
        onChange(field.options.map((o) => o.value).filter((o) => set.has(o)))
      }}
    >
      {field.options.map(({ value: v, labelKey, icon: Icon }) => (
        <ToggleGroupItem key={v} value={v} aria-label={t(labelKey)} className="flex-none">
          {Icon ? <Icon /> : null}
          <span className="truncate">{t(labelKey)}</span>
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}

export function SwitchFieldControl({ id, value, disabled, describedBy, onChange }: FieldProps<unknown, unknown>) {
  return <Switch id={id} checked={value === true} disabled={disabled} aria-describedby={describedBy} onCheckedChange={(v) => onChange(v)} />
}

export function TextFieldControl({ field, id, value, disabled, invalid, describedBy, onChange, onCommit }: FieldProps<TextField, unknown>) {
  const t = useLooseT()
  return (
    <Input
      id={id}
      className={cn('h-8', field.monospace && 'font-mono text-[13px]')}
      value={typeof value === 'string' ? value : ''}
      placeholder={field.placeholderKey ? t(field.placeholderKey) : undefined}
      disabled={disabled}
      aria-invalid={invalid}
      aria-describedby={describedBy}
      spellCheck={false}
      onChange={(e) => onChange(e.target.value, { coalesce: true })}
      onBlur={onCommit}
    />
  )
}

// ---------------------------------------------------------------------------
// Color: '#rrggbb' or '#rrggbbaa'

function parseHex(value: unknown): { rgb: string; alpha: number } {
  const s = typeof value === 'string' ? value.trim() : ''
  const m = /^#?([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(s)
  if (!m) return { rgb: '#000000', alpha: 255 }
  return { rgb: `#${m[1].toLowerCase()}`, alpha: m[2] ? parseInt(m[2], 16) : 255 }
}

function toHex(rgb: string, alpha: number, withAlpha: boolean): string {
  return withAlpha ? `${rgb}${Math.round(clamp(alpha, 0, 255)).toString(16).padStart(2, '0')}` : rgb
}

export function ColorFieldControl({ field, id, value, disabled, invalid, describedBy, onChange, onCommit }: FieldProps<ColorField, unknown>) {
  const { t } = useTranslation('common')
  const { rgb, alpha } = parseHex(value)
  const withAlpha = !!field.alpha
  const [draft, setDraft] = useState<string | null>(null)
  const display = draft ?? (typeof value === 'string' ? value : toHex(rgb, alpha, withAlpha))
  return (
    <div className="flex items-center gap-2">
      <label
        className="bg-checker relative size-8 shrink-0 overflow-hidden rounded-md border border-border shadow-xs focus-within:ring-2 focus-within:ring-ring/50"
        title={t('params.pickColor')}
      >
        <span className="absolute inset-0" style={{ backgroundColor: rgb, opacity: alpha / 255 }} />
        <input
          type="color"
          aria-label={t('params.pickColor')}
          className="absolute inset-0 size-full cursor-pointer opacity-0"
          value={rgb}
          disabled={disabled}
          onChange={(e) => onChange(toHex(e.target.value, alpha, withAlpha), { coalesce: true })}
          onBlur={onCommit}
        />
      </label>
      <Input
        id={id}
        className="h-8 flex-1 font-mono text-[13px] uppercase"
        aria-label={t('params.hex')}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        value={display}
        disabled={disabled}
        spellCheck={false}
        onChange={(e) => {
          setDraft(e.target.value)
          const parsed = /^#?([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(e.target.value.trim())
          if (parsed) {
            const a = parsed[2] ? parseInt(parsed[2], 16) : withAlpha ? alpha : 255
            onChange(toHex(`#${parsed[1].toLowerCase()}`, a, withAlpha), { coalesce: true })
          }
        }}
        onBlur={() => {
          setDraft(null)
          onCommit()
        }}
      />
      {withAlpha ? (
        <NumberInput
          className="w-20 shrink-0"
          aria-label={t('params.alpha')}
          value={Math.round((alpha / 255) * 100)}
          min={0}
          max={100}
          unit={t('units.percent')}
          hideSteppers
          disabled={disabled}
          onValueChange={(pct) => onChange(toHex(rgb, (clamp(pct, 0, 100) / 100) * 255, true), { coalesce: true })}
          onCommit={onCommit}
        />
      ) : null}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Anchor: 3×3 picker

export function AnchorFieldControl({ field, id, value, disabled, describedBy, onChange }: FieldProps<AnchorField, unknown>) {
  const { t } = useTranslation('common')
  const lt = useLooseT()
  const current = (ANCHORS as readonly string[]).includes(value as string) ? (value as Anchor) : 'center'
  const refs = useRef<(HTMLButtonElement | null)[]>([])
  const move = (index: number) => {
    const next = ANCHORS[index]
    onChange(next)
    refs.current[index]?.focus()
  }
  return (
    <div
      id={id}
      role="radiogroup"
      aria-label={lt(field.labelKey)}
      aria-describedby={describedBy}
      className="grid w-fit grid-cols-3 gap-1 rounded-lg bg-muted p-1"
      onKeyDown={(e) => {
        const i = ANCHORS.indexOf(current)
        const delta = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: 3, ArrowUp: -3 }[e.key]
        if (delta === undefined) return
        e.preventDefault()
        const n = i + delta
        if (n >= 0 && n < 9) move(n)
      }}
    >
      {ANCHORS.map((anchor, i) => {
        const checked = anchor === current
        return (
          <Tooltip key={anchor} content={t(`params.anchor.${anchor}`)}>
            <button
              ref={(el) => {
                refs.current[i] = el
              }}
              type="button"
              role="radio"
              aria-checked={checked}
              aria-label={t(`params.anchor.${anchor}`)}
              tabIndex={checked ? 0 : -1}
              disabled={disabled}
              onClick={() => onChange(anchor)}
              className={cn(
                'flex size-7 items-center justify-center rounded-md outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60',
                checked ? 'bg-primary text-primary-foreground shadow-sm' : 'hover:bg-card',
              )}
            >
              <span className={cn('rounded-full', checked ? 'size-2.5 bg-primary-foreground' : 'size-1.5 bg-foreground/40')} />
            </button>
          </Tooltip>
        )
      })}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Size: width × height with link toggle

export function SizeFieldControl({
  field,
  id,
  value,
  disabled,
  invalid,
  describedBy,
  onChange,
  onCommit,
  linked,
  onLinkedChange,
}: FieldProps<SizeField, unknown> & { linked: boolean; onLinkedChange: (linked: boolean) => void }) {
  const { t } = useTranslation('common')
  const lt = useLooseT()
  const size: SizeValue =
    value && typeof value === 'object' ? { width: num((value as SizeValue).width), height: num((value as SizeValue).height) } : { width: 0, height: 0 }
  const ratio = useRef(size.width > 0 && size.height > 0 ? size.height / size.width : 1)
  const unit = field.unitKey ? lt(field.unitKey) : undefined

  const setWidth = (width: number) => {
    const height = linked ? Math.max(1, Math.round(width * ratio.current)) : size.height
    onChange({ width, height }, { coalesce: true })
  }
  const setHeight = (height: number) => {
    const width = linked ? Math.max(1, Math.round(height / ratio.current)) : size.width
    onChange({ width, height }, { coalesce: true })
  }

  return (
    <div className="flex items-center gap-1.5">
      <NumberInput
        id={id}
        className="flex-1"
        aria-label={t('params.width')}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        value={size.width}
        min={field.min}
        max={field.max}
        step={field.step}
        unit={unit}
        disabled={disabled}
        onValueChange={setWidth}
        onCommit={onCommit}
      />
      <Tooltip content={t('params.linkSize')}>
        <button
          type="button"
          aria-label={t('params.linkSize')}
          aria-pressed={linked}
          disabled={disabled}
          onClick={() => {
            if (!linked && size.width > 0 && size.height > 0) ratio.current = size.height / size.width
            onLinkedChange(!linked)
          }}
          className={cn(
            'flex size-8 shrink-0 items-center justify-center rounded-md outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50',
            linked ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent',
          )}
        >
          {linked ? <Link2Icon className="size-4" /> : <Unlink2Icon className="size-4" />}
        </button>
      </Tooltip>
      <NumberInput
        className="flex-1"
        aria-label={t('params.height')}
        aria-invalid={invalid}
        value={size.height}
        min={field.min}
        max={field.max}
        step={field.step}
        unit={unit}
        disabled={disabled}
        onValueChange={setHeight}
        onCommit={onCommit}
      />
    </div>
  )
}

export function FieldShell({
  id,
  label,
  description,
  error,
  inline,
  children,
  descId,
}: {
  id: string
  label: string
  description?: string
  error?: string
  inline?: boolean
  children: ReactNode
  descId: string
}) {
  return (
    <div className="space-y-1.5" data-field-id={id}>
      <div className={cn(inline ? 'flex items-center justify-between gap-3' : 'space-y-1.5')}>
        <label htmlFor={id} className="block text-[13px] font-medium leading-tight text-foreground/90">
          {label}
        </label>
        {children}
      </div>
      {description ? (
        <p id={descId} className="text-xs leading-snug text-muted-foreground">
          {description}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-xs font-medium text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}
