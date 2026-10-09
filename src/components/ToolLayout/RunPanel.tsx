import { PlayIcon, SquareIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Progress, Spinner } from '@/components/ui/misc'
import { isJobActive, useJobs, useTabJob } from '@/stores/jobs'

interface RunPanelProps {
  tabId: string
  fileCount: number
  /** Already translated reason why Run is disabled (shown under the button). */
  disabledReason: string | null
  onRun: () => void
  /** Custom button label (default "Process N images"). */
  label?: string
}

/** Run button, or progress + cancel while the tab's job is running. */
export function RunPanel({ tabId, fileCount, disabledReason, onRun, label }: RunPanelProps) {
  const { t } = useTranslation('common')
  const job = useTabJob(tabId)
  const active = isJobActive(job)

  if (job && active) {
    const pct = job.total > 0 ? Math.round((job.done / job.total) * 100) : null
    const current = job.currentPath?.split(/[\\/]/).pop()
    return (
      <div className="space-y-2.5" data-testid="job-progress">
        <div className="flex items-center justify-between gap-2 text-xs">
          <span className="flex min-w-0 items-center gap-1.5 font-medium">
            <Spinner className="size-3.5 text-primary" />
            <span className="truncate">{job.status === 'starting' ? t('jobs.starting') : (current ?? t('jobs.starting'))}</span>
          </span>
          <span className="shrink-0 tabular-nums text-muted-foreground">{t('jobs.progress', { done: job.done, total: job.total })}</span>
        </div>
        <Progress value={pct} />
        <Button variant="outline" className="w-full" onClick={() => void useJobs.getState().cancel(tabId)} disabled={!job.jobId}>
          <SquareIcon className="fill-current" />
          {t('run.cancel')}
        </Button>
      </div>
    )
  }

  return (
    <div className="space-y-2">
      <Button size="lg" className="w-full" disabled={disabledReason != null} onClick={onRun} data-testid="run-button">
        <PlayIcon className="fill-current" />
        {label ?? (fileCount > 0 ? t('run.runCount', { count: fileCount }) : t('run.label'))}
      </Button>
      {disabledReason ? <p className="text-center text-xs text-muted-foreground">{disabledReason}</p> : null}
    </div>
  )
}
