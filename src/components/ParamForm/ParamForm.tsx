import { ChevronRightIcon, SlidersHorizontalIcon } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import type { z } from 'zod'

import { useLooseT } from '@/i18n/loose'
import { cn } from '@/lib/utils'
import { requireSession, useSession, type Params } from '@/stores/session'

import {
  AnchorFieldControl,
  ColorFieldControl,
  MultiSelectFieldControl,
  FieldShell,
  NumberFieldControl,
  SegmentedFieldControl,
  SelectFieldControl,
  SizeFieldControl,
  SliderFieldControl,
  SwitchFieldControl,
  TextFieldControl,
} from './fields'
import type { FieldDescriptor, GroupField, SizeField } from './types'
import { validateParams, type FieldError } from './validation'

export interface ParamFormProps {
  tabId: string
  fields: FieldDescriptor[]
  /** zod schema used for inline validation (messages are i18n keys). */
  schema?: z.ZodType
  className?: string
}

/** Renders controls from declarative descriptors, bound to the tab's session params. */
export function ParamForm({ tabId, fields, schema, className }: ParamFormProps) {
  const { t } = useTranslation('common')
  const params = useSession(tabId, (s) => s.params)
  const errors = useMemo(() => validateParams(schema, params), [schema, params])

  if (fields.length === 0) {
    return (
      <div className={cn('flex flex-col items-center gap-2 rounded-lg border border-dashed border-border px-4 py-8 text-center', className)}>
        <SlidersHorizontalIcon className="size-5 text-muted-foreground" />
        <p className="text-sm text-muted-foreground">{t('params.empty')}</p>
      </div>
    )
  }

  return (
    <div className={cn('space-y-4', className)} data-testid="param-form">
      <FieldList tabId={tabId} fields={fields} params={params} errors={errors} />
    </div>
  )
}

interface ListProps {
  tabId: string
  fields: FieldDescriptor[]
  params: Params
  errors: Record<string, FieldError>
}

function FieldList({ tabId, fields, params, errors }: ListProps) {
  return (
    <>
      {fields.map((field) => {
        if (field.visibleIf && !field.visibleIf(params)) return null
        if (field.kind === 'group') {
          return <Group key={`group:${field.id}`} tabId={tabId} group={field} params={params} errors={errors} />
        }
        return <Field key={field.key} tabId={tabId} field={field} params={params} error={errors[field.key]} />
      })}
    </>
  )
}

function Group({ tabId, group, params, errors }: { tabId: string; group: GroupField; params: Params; errors: Record<string, FieldError> }) {
  const t = useLooseT()
  // Stored in the session's UI flags so it survives sleep and dies with the tab.
  const flag = useSession(tabId, (s) => s.uiFlags[`group:${group.id}`])
  const open = flag ?? group.defaultOpen ?? true
  const collapsible = group.collapsible ?? true
  const contentId = `group-${tabId}-${group.id}`
  const toggle = () => requireSession(tabId).getState().setUiFlag(`group:${group.id}`, !open)
  return (
    <section className="rounded-lg border border-border bg-card/60" data-testid={`group-${group.id}`}>
      {collapsible ? (
        <button
          type="button"
          aria-expanded={open}
          aria-controls={contentId}
          onClick={toggle}
          className="flex w-full items-center gap-2 rounded-lg px-3 py-2.5 text-left text-[13px] font-semibold outline-none transition-colors hover:bg-accent/60 focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          <ChevronRightIcon className={cn('size-4 text-muted-foreground transition-transform duration-200', open && 'rotate-90')} />
          {t(group.labelKey)}
        </button>
      ) : (
        <h3 className="px-3 pb-1 pt-2.5 text-[13px] font-semibold">{t(group.labelKey)}</h3>
      )}
      {open || !collapsible ? (
        <div id={contentId} className="space-y-4 px-3 pb-3 pt-1">
          {group.descKey ? <p className="text-xs text-muted-foreground">{t(group.descKey)}</p> : null}
          <FieldList tabId={tabId} fields={group.fields} params={params} errors={errors} />
        </div>
      ) : null}
    </section>
  )
}

type LeafField = Exclude<FieldDescriptor, GroupField>

function Field({ tabId, field, params, error }: { tabId: string; field: LeafField; params: Params; error?: FieldError }) {
  const t = useLooseT()
  const id = `param-${tabId}-${field.key}`
  const descId = `${id}-desc`
  const value = params[field.key]
  const disabled = field.disabledIf?.(params) ?? false
  const invalid = !!error

  const onChange = (next: unknown, options?: { coalesce?: boolean }) => {
    requireSession(tabId).getState().setParams({ [field.key]: next }, options)
  }
  const onCommit = () => requireSession(tabId).getState().commitParams()

  const common = {
    id,
    value,
    disabled,
    invalid,
    describedBy: field.descKey ? descId : undefined,
    onChange,
    onCommit,
  }

  let control
  switch (field.kind) {
    case 'number':
      control = <NumberFieldControl field={field} {...common} />
      break
    case 'slider':
      control = <SliderFieldControl field={field} {...common} />
      break
    case 'select':
      control = <SelectFieldControl field={field} {...common} />
      break
    case 'segmented':
      control = <SegmentedFieldControl field={field} {...common} />
      break
    case 'multiSelect':
      control = <MultiSelectFieldControl field={field} {...common} />
      break
    case 'switch':
      control = <SwitchFieldControl field={field} {...common} />
      break
    case 'color':
      control = <ColorFieldControl field={field} {...common} />
      break
    case 'text':
      control = <TextFieldControl field={field} {...common} />
      break
    case 'anchor':
      control = <AnchorFieldControl field={field} {...common} />
      break
    case 'size':
      control = <SizeControl tabId={tabId} field={field} params={params} common={common} />
      break
  }

  return (
    <FieldShell
      id={id}
      descId={descId}
      label={t(field.labelKey)}
      description={field.descKey ? t(field.descKey) : undefined}
      error={error ? t(error.key, error.values) : undefined}
      inline={field.kind === 'switch'}
    >
      {control}
    </FieldShell>
  )
}

function SizeControl({
  tabId,
  field,
  params,
  common,
}: {
  tabId: string
  field: SizeField
  params: Params
  common: Omit<Parameters<typeof SizeFieldControl>[0], 'field' | 'linked' | 'onLinkedChange'>
}) {
  const localLinked = useSession(tabId, (s) => s.uiFlags[`link:${field.key}`]) ?? true
  const linked = field.linkKey ? params[field.linkKey] === true : localLinked
  const setLinked = (v: boolean) => {
    if (field.linkKey) requireSession(tabId).getState().setParams({ [field.linkKey]: v })
    else requireSession(tabId).getState().setUiFlag(`link:${field.key}`, v)
  }
  return <SizeFieldControl field={field} {...common} linked={linked} onLinkedChange={setLinked} />
}
