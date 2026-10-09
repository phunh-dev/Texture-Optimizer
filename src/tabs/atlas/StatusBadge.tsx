import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

import type { SpriteStatus } from './ipc'

/** Tailwind classes per merge status (badge background + overlay stroke). */
export const STATUS_STYLES: Record<SpriteStatus, { badge: string; stroke: string }> = {
  new: { badge: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300', stroke: 'stroke-emerald-500' },
  replaced: { badge: 'bg-amber-500/15 text-amber-700 dark:text-amber-300', stroke: 'stroke-amber-500' },
  kept: { badge: 'bg-sky-500/15 text-sky-700 dark:text-sky-300', stroke: 'stroke-sky-500' },
  removed: { badge: 'bg-destructive/15 text-destructive', stroke: 'stroke-destructive' },
}

export function StatusBadge({ status, count, className }: { status: SpriteStatus; count?: number; className?: string }) {
  const { t } = useTranslation('atlas')
  return (
    <span
      data-status={status}
      className={cn(
        'inline-flex shrink-0 items-center rounded-full px-1.5 py-px text-[10px] font-semibold leading-4 tabular-nums',
        STATUS_STYLES[status].badge,
        className,
      )}
    >
      {count === undefined ? t(`status.${status}`) : t(`statusCount.${status}`, { count })}
    </span>
  )
}
