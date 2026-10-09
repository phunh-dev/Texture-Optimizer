// Custom ParamForm controls of the Pattern Renamer.
import { ArrowDownIcon, ArrowUpIcon, CaseSensitiveIcon, FolderOpenIcon, PlusIcon, RegexIcon, XIcon } from 'lucide-react'
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import type { CustomFieldProps } from '@/components/ParamForm'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Tooltip } from '@/components/ui/tooltip'
import { pickOutputFolder } from '@/lib/import'
import { cn } from '@/lib/utils'

import { useRenamePlanContext } from './plan'
import { ENGINE_PRESETS, NEW_RULE, TEXTURE_TYPES, TOKENS, type FindReplaceRule, type SmartParams } from './schema'

// ---------------------------------------------------------------------------
// Template with token chips

export function TemplateControl({ id, value, invalid, describedBy, onChange, onCommit }: CustomFieldProps) {
  const { t } = useTranslation('rename')
  const ref = useRef<HTMLInputElement>(null)
  const template = typeof value === 'string' ? value : ''

  const insert = (token: string) => {
    const el = ref.current
    const text = `{${token}}`
    const start = el?.selectionStart ?? template.length
    const end = el?.selectionEnd ?? template.length
    const next = template.slice(0, start) + text + template.slice(end)
    onChange(next)
    onCommit()
    // Put the caret right after the inserted token.
    requestAnimationFrame(() => {
      if (!el) return
      el.focus()
      el.setSelectionRange(start + text.length, start + text.length)
    })
  }

  return (
    <div className="space-y-2">
      <Input
        ref={ref}
        id={id}
        className="h-8 font-mono text-[13px]"
        value={template}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value, { coalesce: true })}
        onBlur={onCommit}
      />
      <div className="flex flex-wrap gap-1" role="group" aria-label={t('params.template.tokens')}>
        {TOKENS.map((token) => (
          <Tooltip key={token} content={t(`params.template.token.${token}`)}>
            <button
              type="button"
              aria-label={t('params.template.insert', { token: `{${token}}` })}
              className="rounded-md border border-border bg-muted px-1.5 py-0.5 font-mono text-[11px] text-foreground/80 transition-colors hover:border-primary/50 hover:bg-primary/10 hover:text-primary"
              onMouseDown={(e) => e.preventDefault() /* keep the caret in the input */}
              onClick={() => insert(token)}
            >
              {`{${token}}`}
            </button>
          </Tooltip>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Find / replace rule list

const asRules = (v: unknown): FindReplaceRule[] => (Array.isArray(v) ? (v as FindReplaceRule[]) : [])

export function RulesControl({ value, onChange, onCommit }: CustomFieldProps) {
  const { t } = useTranslation('rename')
  const rules = asRules(value)
  const { error } = useRenamePlanContext()
  const badIndex = error?.code === 'RENAME_INVALID_REGEX' ? Number(error.params.index) : -1

  const update = (i: number, patch: Partial<FindReplaceRule>, coalesce = false) =>
    onChange(
      rules.map((r, j) => (j === i ? { ...r, ...patch } : r)),
      coalesce ? { coalesce: true } : undefined,
    )
  const move = (i: number, delta: number) => {
    const j = i + delta
    if (j < 0 || j >= rules.length) return
    const next = [...rules]
    ;[next[i], next[j]] = [next[j], next[i]]
    onChange(next)
  }

  return (
    <div className="space-y-2">
      {rules.length === 0 ? <p className="text-xs text-muted-foreground">{t('params.findReplace.empty')}</p> : null}
      <ol className="space-y-2">
        {rules.map((rule, i) => {
          const n = i + 1
          return (
            <li key={i} className="space-y-1.5 rounded-md border border-border bg-card p-2" data-testid={`rule-${i}`}>
              <div className="flex items-center gap-1">
                <span className="w-5 text-center text-[11px] font-semibold tabular-nums text-muted-foreground">{n}</span>
                <Input
                  className="h-7 flex-1 font-mono text-xs"
                  aria-label={t('params.findReplace.find', { n })}
                  aria-invalid={badIndex === i}
                  placeholder={t('params.findReplace.findPlaceholder')}
                  value={rule.find}
                  spellCheck={false}
                  onChange={(e) => update(i, { find: e.target.value }, true)}
                  onBlur={onCommit}
                />
                <Tooltip content={t('params.findReplace.regex')}>
                  <button
                    type="button"
                    aria-label={t('params.findReplace.regex')}
                    aria-pressed={rule.regex}
                    onClick={() => update(i, { regex: !rule.regex })}
                    className={toggleClass(rule.regex)}
                  >
                    <RegexIcon className="size-3.5" />
                  </button>
                </Tooltip>
                <Tooltip content={t('params.findReplace.caseSensitive')}>
                  <button
                    type="button"
                    aria-label={t('params.findReplace.caseSensitive')}
                    aria-pressed={rule.caseSensitive}
                    onClick={() => update(i, { caseSensitive: !rule.caseSensitive })}
                    className={toggleClass(rule.caseSensitive)}
                  >
                    <CaseSensitiveIcon className="size-3.5" />
                  </button>
                </Tooltip>
              </div>
              <div className="flex items-center gap-1">
                <span className="w-5" />
                <Input
                  className="h-7 flex-1 font-mono text-xs"
                  aria-label={t('params.findReplace.replace', { n })}
                  placeholder={t('params.findReplace.replacePlaceholder')}
                  value={rule.replace}
                  spellCheck={false}
                  onChange={(e) => update(i, { replace: e.target.value }, true)}
                  onBlur={onCommit}
                />
                <Button variant="ghost" size="icon-xs" aria-label={t('params.findReplace.moveUp', { n })} disabled={i === 0} onClick={() => move(i, -1)}>
                  <ArrowUpIcon />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t('params.findReplace.moveDown', { n })}
                  disabled={i === rules.length - 1}
                  onClick={() => move(i, 1)}
                >
                  <ArrowDownIcon />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t('params.findReplace.remove', { n })}
                  className="hover:text-destructive"
                  onClick={() => onChange(rules.filter((_, j) => j !== i))}
                >
                  <XIcon />
                </Button>
              </div>
              {badIndex === i ? (
                <p role="alert" className="pl-6 text-xs font-medium text-destructive">
                  {t('params.findReplace.invalidRegex', { detail: String(error?.params.detail ?? '') })}
                </p>
              ) : null}
            </li>
          )
        })}
      </ol>
      <Button variant="outline" size="sm" className="w-full" onClick={() => onChange([...rules, { ...NEW_RULE }])}>
        <PlusIcon />
        {t('params.findReplace.add')}
      </Button>
    </div>
  )
}

const toggleClass = (on: boolean) =>
  cn(
    'flex size-7 shrink-0 items-center justify-center rounded-md outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50',
    on ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-accent',
  )

// ---------------------------------------------------------------------------
// Smart (engine convention) naming

const asSmart = (v: unknown): SmartParams =>
  v && typeof v === 'object' ? (v as SmartParams) : { enabled: false, preset: 'unreal', customMap: {} }

export function SmartControl({ id, value, onChange, onCommit }: CustomFieldProps) {
  const { t } = useTranslation('rename')
  const smart = asSmart(value)
  const set = (patch: Partial<SmartParams>, coalesce = false) => onChange({ ...smart, ...patch }, coalesce ? { coalesce: true } : undefined)
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <label htmlFor={id} className="text-[13px] font-medium">
          {t('params.smart.enabled')}
        </label>
        <Switch id={id} checked={smart.enabled} onCheckedChange={(v) => set({ enabled: v })} />
      </div>
      <p className="text-xs text-muted-foreground">{t('params.smart.desc')}</p>
      {smart.enabled ? (
        <>
          <div className="space-y-1.5">
            <label htmlFor={`${id}-preset`} className="block text-[13px] font-medium">
              {t('params.smart.preset')}
            </label>
            <Select value={smart.preset} onValueChange={(v) => set({ preset: v as SmartParams['preset'] })}>
              <SelectTrigger id={`${id}-preset`} size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ENGINE_PRESETS.map((p) => (
                  <SelectItem key={p} value={p}>
                    {t(`params.smart.presets.${p}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="font-mono text-[11px] text-muted-foreground">{t(`params.smart.examples.${smart.preset}`)}</p>
          </div>
          {smart.preset === 'custom' ? (
            <div className="space-y-1.5" role="group" aria-label={t('params.smart.customMap')}>
              <p className="text-[13px] font-medium">{t('params.smart.customMap')}</p>
              <div className="grid grid-cols-[auto_1fr] items-center gap-x-2 gap-y-1">
                {TEXTURE_TYPES.map((type) => (
                  <label key={type} className="contents">
                    <span className="text-xs text-muted-foreground">{t(`params.smart.types.${type}`)}</span>
                    <Input
                      className="h-7 font-mono text-xs"
                      aria-label={t('params.smart.suffixFor', { type: t(`params.smart.types.${type}`) })}
                      value={smart.customMap[type] ?? ''}
                      spellCheck={false}
                      onChange={(e) => set({ customMap: { ...smart.customMap, [type]: e.target.value } }, true)}
                      onBlur={onCommit}
                    />
                  </label>
                ))}
              </div>
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Destination folder (copy mode)

export function FolderControl({ id, value, invalid, describedBy, onChange }: CustomFieldProps) {
  const { t } = useTranslation('rename')
  const dir = typeof value === 'string' ? value : ''
  const choose = async () => {
    const picked = await pickOutputFolder()
    if (picked) onChange(picked)
  }
  return (
    <div className="flex items-center gap-2">
      <div
        id={id}
        aria-invalid={invalid}
        aria-describedby={describedBy}
        title={dir || undefined}
        className={cn('h-8 min-w-0 flex-1 truncate rounded-md border border-border bg-muted/40 px-2 text-xs leading-8', !dir && 'text-muted-foreground')}
      >
        {dir || t('params.copyDir.none')}
      </div>
      <Button variant="outline" size="sm" onClick={() => void choose()}>
        <FolderOpenIcon />
        {t('params.copyDir.choose')}
      </Button>
    </div>
  )
}
