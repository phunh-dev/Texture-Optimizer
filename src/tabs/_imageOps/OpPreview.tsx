import { ArrowRightIcon, ImageIcon, SlidersHorizontalIcon, TriangleAlertIcon } from 'lucide-react'
import { useEffect, useEffectEvent, type ReactNode } from 'react'

import { CompareView, type CompareRect, type ToolContext } from '@/components/ToolLayout'
import { Spinner } from '@/components/ui/misc'
import { useLooseT } from '@/i18n/loose'
import { translateError } from '@/lib/errors'
import { originalImageUrl } from '@/lib/ipc'
import type { ImportedFile, OpRequest, PreviewResult } from '@/lib/ipc/types'
import { cn } from '@/lib/utils'

import { useOpPreview } from './usePreview'

/** Shared UI strings of the image-op tabs live in the `resize` namespace. */
export const IMAGE_OPS_NS = 'resize:imageOps'

export interface OpPreviewProps {
  ctx: ToolContext
  /** Request for the current params, or null when they are invalid. */
  request: OpRequest | null
  /**
   * Where the result is drawn over the original, in original pixel coordinates.
   * `request` is the one that produced `result` (it may lag behind the params while loading).
   */
  placement?: (file: ImportedFile, result: PreviewResult, request: OpRequest) => CompareRect | undefined
  /** Called once per settled successful preview. */
  onResult?: (file: ImportedFile, result: PreviewResult, request: OpRequest) => void
  /** Extra info rendered in the header (e.g. trim offsets). */
  details?: (file: ImportedFile, result: PreviewResult) => ReactNode
}

function safeOriginalUrl(file: ImportedFile): string {
  try {
    return originalImageUrl(file)
  } catch {
    return ''
  }
}

function Message({ icon, children, tone = 'muted' }: { icon: ReactNode; children: ReactNode; tone?: 'muted' | 'error' }) {
  return (
    <div
      role={tone === 'error' ? 'alert' : undefined}
      className={cn(
        'flex size-full flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border p-6 text-center text-sm',
        tone === 'error' ? 'text-destructive' : 'text-muted-foreground',
      )}
      data-testid={tone === 'error' ? 'preview-error' : 'preview-message'}
    >
      {icon}
      <div className="max-w-md">{children}</div>
    </div>
  )
}

/** Debounced live preview of an image op on the focused file, as a before/after comparison. */
export function OpPreview({ ctx, request, placement, onResult, details }: OpPreviewProps) {
  const t = useLooseT()
  const file = ctx.focusFile
  const preview = useOpPreview(ctx.tabId, file, request)
  const shownRequest = preview.key ? (JSON.parse(preview.key) as { request: OpRequest }).request : null

  const report = useEffectEvent(() => {
    if (file && preview.result && shownRequest && !preview.stale) onResult?.(file, preview.result, shownRequest)
  })
  const settledKey = preview.stale ? null : preview.key
  useEffect(() => {
    if (settledKey) report()
  }, [settledKey])

  if (!file) {
    return <Message icon={<ImageIcon className="size-6" />}>{t(`${IMAGE_OPS_NS}.noFile`)}</Message>
  }
  if (!request) {
    return <Message icon={<SlidersHorizontalIcon className="size-6" />}>{t(`${IMAGE_OPS_NS}.invalidParams`)}</Message>
  }

  const dims = (w: number, h: number) => t(`${IMAGE_OPS_NS}.dimensions`, { width: w, height: h })
  const result = preview.result

  let body: ReactNode
  if (preview.error) {
    body = (
      <Message icon={<TriangleAlertIcon className="size-6" />} tone="error">
        <p className="font-medium">{t(`${IMAGE_OPS_NS}.previewFailed`)}</p>
        <p className="mt-1 text-xs" data-testid="preview-error-message">
          {translateError(preview.error)}
        </p>
      </Message>
    )
  } else if (result && preview.url && shownRequest) {
    body = (
      <CompareView
        key={file.id}
        before={safeOriginalUrl(file)}
        after={preview.url}
        beforeSize={{ width: file.width, height: file.height }}
        beforeLabel={dims(file.width, file.height)}
        afterLabel={dims(result.width, result.height)}
        afterRect={placement?.(file, result, shownRequest)}
        className={cn(preview.stale && 'opacity-90')}
      />
    )
  } else {
    body = (
      <Message icon={<Spinner className="size-6" />}>
        <span>{t(`${IMAGE_OPS_NS}.rendering`)}</span>
      </Message>
    )
  }

  return (
    <div className="flex size-full min-h-0 flex-col gap-2" data-testid="op-preview">
      <div className="flex min-h-6 items-center gap-2 text-xs text-muted-foreground">
        <span className="truncate font-medium text-foreground" data-testid="preview-file">
          {file.name}
        </span>
        <span className="flex shrink-0 items-center gap-1 tabular-nums" data-testid="preview-dims">
          <span>{dims(file.width, file.height)}</span>
          {result ? (
            <>
              <ArrowRightIcon className="size-3" aria-hidden />
              <span>{dims(result.width, result.height)}</span>
            </>
          ) : null}
        </span>
        {file && result ? details?.(file, result) : null}
        {preview.loading ? (
          <span className="ml-auto flex shrink-0 items-center gap-1.5" data-testid="preview-loading">
            <Spinner className="size-3.5 text-primary" />
            {t(`${IMAGE_OPS_NS}.updating`)}
          </span>
        ) : null}
      </div>
      <div className="relative min-h-0 flex-1">{body}</div>
    </div>
  )
}
