import { ArrowRightIcon, CircleCheckIcon, TriangleAlertIcon } from 'lucide-react'
import type { ReactNode } from 'react'

import { useLooseT } from '@/i18n/loose'
import { translateError } from '@/lib/errors'
import type { ImportedFile } from '@/lib/ipc/types'
import { cn } from '@/lib/utils'
import { requireSession, useSession } from '@/stores/session'

import { IMAGE_OPS_NS } from './OpPreview'
import type { SizeResult } from './size'

/** Rows rendered at most (the list is a summary, the grid shows everything). */
export const SIZE_SUMMARY_LIMIT = 300

export type RowState = 'changed' | 'unchanged' | 'error' | 'unknown'

export function rowState(file: Pick<ImportedFile, 'width' | 'height'>, r: SizeResult | null): RowState {
  if (!r) return 'unknown'
  if (!r.ok) return 'error'
  return r.width === file.width && r.height === file.height ? 'unchanged' : 'changed'
}

export interface SizeSummaryProps {
  tabId: string
  /** Predicted size per file; null = unknown yet (content dependent). */
  predict: (file: ImportedFile) => SizeResult | null
  /** Shown under the counts (e.g. "measure" action for content-dependent sizes). */
  footer?: ReactNode
}

/** "W×H → W'×H'" for every file, with unchanged files and errors highlighted. Click focuses a file. */
export function SizeSummary({ tabId, predict, footer }: SizeSummaryProps) {
  const t = useLooseT()
  const files = useSession(tabId, (s) => s.files)
  const selectedIds = useSession(tabId, (s) => s.selectedIds)
  const rows = files.map((file) => {
    const result = predict(file)
    return { file, result, state: rowState(file, result) }
  })
  const count = (s: RowState) => rows.filter((r) => r.state === s).length
  const dims = (w: number, h: number) => t(`${IMAGE_OPS_NS}.dimensions`, { width: w, height: h })

  return (
    <section className="space-y-2.5" data-testid="size-summary">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t(`${IMAGE_OPS_NS}.sizes.title`)}</h2>
      {files.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t(`${IMAGE_OPS_NS}.sizes.empty`)}</p>
      ) : (
        <>
          <p className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground" data-testid="size-counts">
            <span>{t(`${IMAGE_OPS_NS}.sizes.changed`, { count: count('changed') })}</span>
            <span>{t(`${IMAGE_OPS_NS}.sizes.unchanged`, { count: count('unchanged') })}</span>
            {count('error') > 0 ? <span className="text-destructive">{t(`${IMAGE_OPS_NS}.sizes.errors`, { count: count('error') })}</span> : null}
            {count('unknown') > 0 ? <span>{t(`${IMAGE_OPS_NS}.sizes.unknown`, { count: count('unknown') })}</span> : null}
          </p>
          <ul className="max-h-64 space-y-0.5 overflow-y-auto rounded-lg border border-border bg-card/60 p-1" aria-label={t(`${IMAGE_OPS_NS}.sizes.title`)}>
            {rows.slice(0, SIZE_SUMMARY_LIMIT).map(({ file, result, state }) => (
              <li key={file.id}>
                <button
                  type="button"
                  data-testid={`size-row-${file.id}`}
                  data-state={state}
                  aria-current={selectedIds[0] === file.id ? 'true' : undefined}
                  onClick={() => requireSession(tabId).getState().select(file.id)}
                  className={cn(
                    'flex w-full flex-col gap-0.5 rounded-md px-2 py-1 text-left text-xs outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring/50',
                    selectedIds[0] === file.id && 'bg-primary/10',
                    state === 'unchanged' && 'text-muted-foreground',
                  )}
                >
                  <span className="flex w-full items-center gap-2">
                    <span className="min-w-0 flex-1 truncate font-medium">{file.name}</span>
                    <span className="flex shrink-0 items-center gap-1 tabular-nums">
                      <span>{dims(file.width, file.height)}</span>
                      <ArrowRightIcon className="size-3 opacity-60" aria-hidden />
                      {result?.ok ? (
                        <span data-testid="size-out" className={cn(state === 'changed' && 'font-semibold text-foreground')}>
                          {dims(result.width, result.height)}
                        </span>
                      ) : state === 'error' ? (
                        <TriangleAlertIcon className="size-3.5 text-destructive" aria-label={t(`${IMAGE_OPS_NS}.sizes.errorLabel`)} />
                      ) : (
                        <span data-testid="size-out">{t(`${IMAGE_OPS_NS}.sizes.unknownSize`)}</span>
                      )}
                      {state === 'unchanged' ? (
                        <CircleCheckIcon className="size-3.5 text-emerald-600 dark:text-emerald-400" aria-label={t(`${IMAGE_OPS_NS}.sizes.unchangedLabel`)} />
                      ) : null}
                    </span>
                  </span>
                  {result && !result.ok ? (
                    <span className="text-[11px] text-destructive" data-testid="size-error">
                      {translateError(result.error)}
                    </span>
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
          {rows.length > SIZE_SUMMARY_LIMIT ? (
            <p className="text-xs text-muted-foreground">{t(`${IMAGE_OPS_NS}.sizes.more`, { count: rows.length - SIZE_SUMMARY_LIMIT })}</p>
          ) : null}
        </>
      )}
      {footer}
    </section>
  )
}
