import { useVirtualizer } from '@tanstack/react-virtual'
import { AlertTriangleIcon, ArrowRightIcon, FileTextIcon } from 'lucide-react'
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { Spinner } from '@/components/ui/misc'
import { translateError } from '@/lib/errors'
import { cn } from '@/lib/utils'

import { fileName, isUnchanged, summarize, type PlanState } from './plan'

export const ROW_HEIGHT = 32

/** Virtualized "Current name → New name" table of the live rename plan. */
export function RenameTable({ plan, copy, hasFiles }: { plan: PlanState; copy: boolean; hasFiles: boolean }) {
  const { t } = useTranslation('rename')
  const scrollRef = useRef<HTMLDivElement>(null)
  const { items, error, loading } = plan
  const summary = summarize(items, copy)
  // eslint-disable-next-line react-hooks/incompatible-library -- virtualizer state is read fresh every render
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    initialRect: { width: 800, height: 600 },
  })

  if (!hasFiles) {
    return (
      <div className="flex size-full flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border text-sm text-muted-foreground">
        <FileTextIcon className="size-6" />
        {t('table.noFiles')}
      </div>
    )
  }

  return (
    <div className="flex size-full flex-col overflow-hidden rounded-lg border border-border bg-card" data-testid="rename-table">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-3 py-2 text-xs" data-testid="rename-summary">
        <span className="font-medium">{t('table.summary.total', { count: summary.total })}</span>
        <span className="text-primary">{t(copy ? 'table.summary.toCopy' : 'table.summary.toRename', { count: summary.changed })}</span>
        {!copy ? <span className="text-muted-foreground">{t('table.summary.unchanged', { count: summary.unchanged })}</span> : null}
        <span className={cn(summary.conflicts > 0 ? 'font-semibold text-destructive' : 'text-muted-foreground')}>
          {t('table.summary.conflicts', { count: summary.conflicts })}
        </span>
        {loading ? (
          <span className="ml-auto flex items-center gap-1.5 text-muted-foreground" role="status">
            <Spinner className="size-3" />
            {t('table.updating')}
          </span>
        ) : null}
      </div>

      {error ? (
        <div role="alert" className="flex items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          <AlertTriangleIcon className="size-4 shrink-0" />
          {translateError(error)}
        </div>
      ) : null}

      <div role="table" aria-label={t('table.label')} aria-rowcount={items.length} className="flex min-h-0 flex-1 flex-col">
        <div role="rowgroup" className="border-b border-border bg-muted/40 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          <div role="row" className="grid grid-cols-[3rem_minmax(0,1fr)_1.5rem_minmax(0,1fr)_minmax(0,12rem)] items-center gap-2 px-3 py-1.5">
            <span role="columnheader">{t('table.index')}</span>
            <span role="columnheader">{t('table.current')}</span>
            <span role="columnheader" aria-hidden />
            <span role="columnheader">{t('table.new')}</span>
            <span role="columnheader">{t('table.status')}</span>
          </div>
        </div>
        <div ref={scrollRef} role="rowgroup" className="min-h-0 flex-1 overflow-y-auto">
          <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((row) => {
              const item = items[row.index]
              const unchanged = isUnchanged(item, copy)
              const conflict = item.conflict
              return (
                <div
                  key={row.key}
                  role="row"
                  aria-rowindex={row.index + 1}
                  data-conflict={conflict ?? undefined}
                  data-unchanged={unchanged || undefined}
                  className={cn(
                    'absolute left-0 top-0 grid w-full grid-cols-[3rem_minmax(0,1fr)_1.5rem_minmax(0,1fr)_minmax(0,12rem)] items-center gap-2 border-b border-border/60 px-3 text-[13px]',
                    conflict && 'bg-destructive/10',
                    unchanged && !conflict && 'opacity-50',
                  )}
                  style={{ height: ROW_HEIGHT, transform: `translateY(${row.start}px)` }}
                >
                  <span role="cell" className="tabular-nums text-xs text-muted-foreground">
                    {row.index + 1}
                  </span>
                  <span role="cell" className="truncate font-mono text-xs" title={item.from}>
                    {fileName(item.from)}
                  </span>
                  <ArrowRightIcon className="size-3.5 text-muted-foreground" aria-hidden />
                  <span role="cell" className={cn('truncate font-mono text-xs', conflict ? 'text-destructive' : !unchanged && 'font-medium text-foreground')} title={item.to}>
                    {fileName(item.to)}
                  </span>
                  <span role="cell" className={cn('truncate text-xs', conflict ? 'font-medium text-destructive' : 'text-muted-foreground')}>
                    {conflict ? (
                      <span className="flex items-center gap-1">
                        <AlertTriangleIcon className="size-3.5 shrink-0" />
                        {t(`conflicts.${conflict}`)}
                      </span>
                    ) : unchanged ? (
                      t('table.unchanged')
                    ) : (
                      t('table.ok')
                    )}
                  </span>
                </div>
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}
