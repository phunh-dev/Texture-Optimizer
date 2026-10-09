import { PipetteIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { defineFields, type CustomFieldProps } from '@/components/ParamForm'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { NumberInput } from '@/components/ui/number-input'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { requireSession, useSession } from '@/stores/session'

import { BG_MODES, fromHex, MAX_CELL_SIZE, MAX_FEATHER, toHex, type Rgba } from './schema'

/** Session UI flag: eyedropper armed (click on the preview samples `color`). */
export const PICK_FLAG = 'bgRemove:pick'
/** Cell size proposed when the user turns auto-detection off. */
export const DEFAULT_MANUAL_CELL = 16

const asRgba = (v: unknown): Rgba =>
  Array.isArray(v) && v.length === 4 && v.every((n) => typeof n === 'number') ? (v as Rgba) : [255, 255, 255, 255]

export function ColorPickControl({ tabId, id, value, disabled, describedBy, onChange, onCommit }: CustomFieldProps) {
  const { t } = useTranslation('bgremove')
  const color = asRgba(value)
  const hex = toHex(color)
  const [draft, setDraft] = useState<string | null>(null)
  const picking = useSession(tabId, (s) => s.uiFlags[PICK_FLAG] === true)

  const togglePick = () => {
    const s = requireSession(tabId).getState()
    s.setUiFlag(PICK_FLAG, !picking)
    // Sampling happens on the preview, so show it.
    if (!picking) s.setUiFlag('preview', true)
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <label
          className="relative size-8 shrink-0 overflow-hidden rounded-md border border-border shadow-xs focus-within:ring-2 focus-within:ring-ring/50"
          title={t('params.color.swatch')}
        >
          <span className="absolute inset-0" style={{ backgroundColor: hex }} data-testid="bg-color-swatch" />
          <input
            type="color"
            aria-label={t('params.color.swatch')}
            className="absolute inset-0 size-full cursor-pointer opacity-0"
            value={hex}
            disabled={disabled}
            onChange={(e) => {
              const c = fromHex(e.target.value)
              if (c) onChange(c, { coalesce: true })
            }}
            onBlur={onCommit}
          />
        </label>
        <Input
          id={id}
          className="h-8 flex-1 font-mono text-[13px] uppercase"
          aria-label={t('params.color.hex')}
          aria-describedby={describedBy}
          value={draft ?? hex}
          disabled={disabled}
          spellCheck={false}
          onChange={(e) => {
            setDraft(e.target.value)
            const c = fromHex(e.target.value)
            if (c) onChange(c, { coalesce: true })
          }}
          onBlur={() => {
            setDraft(null)
            onCommit()
          }}
        />
      </div>
      <Button
        type="button"
        className="w-full"
        variant={picking ? 'default' : 'outline'}
        size="sm"
        aria-pressed={picking}
        disabled={disabled}
        onClick={togglePick}
      >
        <PipetteIcon />
        {t('params.color.pick')}
      </Button>
      {picking ? (
        <p className="text-xs font-medium text-primary" role="status">
          {t('params.color.picking')}
        </p>
      ) : null}
    </div>
  )
}

export function CellSizeControl({ id, value, disabled, describedBy, onChange, onCommit }: CustomFieldProps) {
  const { t } = useTranslation(['bgremove', 'common'])
  const auto = value === null || value === undefined
  const size = typeof value === 'number' ? value : DEFAULT_MANUAL_CELL
  return (
    <div className="flex items-center gap-3">
      <div className="flex items-center gap-2">
        <Switch
          id={`${id}-auto`}
          checked={auto}
          disabled={disabled}
          aria-label={t('params.checkerCellSize.auto')}
          onCheckedChange={(v) => onChange(v ? null : DEFAULT_MANUAL_CELL)}
        />
        <label htmlFor={`${id}-auto`} className="text-xs text-muted-foreground">
          {t('params.checkerCellSize.auto')}
        </label>
      </div>
      <NumberInput
        id={id}
        className={cn('flex-1', auto && 'opacity-50')}
        aria-describedby={describedBy}
        value={size}
        min={1}
        max={MAX_CELL_SIZE}
        step={1}
        unit={t('common:units.px')}
        disabled={disabled || auto}
        onValueChange={(v) => onChange(v, { coalesce: true })}
        onCommit={onCommit}
      />
    </div>
  )
}

export const bgRemoveFields = defineFields([
  {
    kind: 'group',
    id: 'bgRemove.background',
    labelKey: 'bgremove:groups.background',
    fields: [
      {
        kind: 'segmented',
        key: 'mode',
        labelKey: 'bgremove:params.mode.label',
        descKey: 'bgremove:params.mode.desc',
        options: BG_MODES.map((m) => ({ value: m, labelKey: `bgremove:params.mode.${m}` })),
      },
      {
        kind: 'custom',
        key: 'color',
        labelKey: 'bgremove:params.color.label',
        visibleIf: (p) => p.mode === 'color',
        render: (props) => <ColorPickControl {...props} />,
      },
      {
        kind: 'custom',
        key: 'checkerCellSize',
        labelKey: 'bgremove:params.checkerCellSize.label',
        descKey: 'bgremove:params.checkerCellSize.desc',
        visibleIf: (p) => p.mode === 'checker',
        render: (props) => <CellSizeControl {...props} />,
      },
    ],
  },
  {
    kind: 'group',
    id: 'bgRemove.matching',
    labelKey: 'bgremove:groups.matching',
    fields: [
      {
        kind: 'segmented',
        key: 'fill',
        labelKey: 'bgremove:params.fill.label',
        descKey: 'bgremove:params.fill.desc',
        options: [
          { value: 'floodFromEdges', labelKey: 'bgremove:params.fill.floodFromEdges' },
          { value: 'global', labelKey: 'bgremove:params.fill.global' },
        ],
      },
      {
        kind: 'slider',
        key: 'tolerance',
        labelKey: 'bgremove:params.tolerance.label',
        descKey: 'bgremove:params.tolerance.desc',
        min: 0,
        max: 100,
        step: 1,
      },
      {
        kind: 'segmented',
        key: 'metric',
        labelKey: 'bgremove:params.metric.label',
        descKey: 'bgremove:params.metric.desc',
        options: [
          { value: 'rgb', labelKey: 'bgremove:params.metric.rgb' },
          { value: 'lab', labelKey: 'bgremove:params.metric.lab' },
        ],
      },
    ],
  },
  {
    kind: 'group',
    id: 'bgRemove.edges',
    labelKey: 'bgremove:groups.edges',
    fields: [
      {
        kind: 'slider',
        key: 'feather',
        labelKey: 'bgremove:params.feather.label',
        descKey: 'bgremove:params.feather.desc',
        min: 0,
        max: MAX_FEATHER,
        step: 1,
        unitKey: 'common:units.px',
      },
      { kind: 'switch', key: 'defringe', labelKey: 'bgremove:params.defringe.label', descKey: 'bgremove:params.defringe.desc' },
      {
        kind: 'slider',
        key: 'defringeStrength',
        labelKey: 'bgremove:params.defringeStrength.label',
        min: 0,
        max: 100,
        step: 1,
        unitKey: 'common:units.percent',
        visibleIf: (p) => p.defringe === true,
      },
    ],
  },
])
