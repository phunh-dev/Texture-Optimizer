import { PenLineIcon, Undo2Icon } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { validateParams } from '@/components/ParamForm'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Spinner } from '@/components/ui/misc'
import { useSession } from '@/stores/session'

import { executeRename, revertLastRename } from './actions'
import { summarize, type PlanState } from './plan'
import { renameSchema, toExecuteMode, toRenameParams } from './schema'

/** Rename button (with confirmation) and "Revert last rename"; replaces ToolLayout's job Run panel. */
export function RenameRunPanel({ tabId, plan }: { tabId: string; plan: PlanState }) {
  const { t } = useTranslation(['rename', 'common'])
  const files = useSession(tabId, (s) => s.files)
  const params = useSession(tabId, (s) => s.params)
  const errors = useMemo(() => validateParams(renameSchema, params), [params])
  const mode = toExecuteMode(params)
  const copy = mode.kind === 'copyTo'
  const summary = summarize(plan.items, copy)
  const [confirming, setConfirming] = useState(false)
  const [busy, setBusy] = useState(false)
  const [reverting, setReverting] = useState(false)

  let reason: string | null = null
  if (files.length === 0) reason = t('common:run.noFiles')
  else if (Object.keys(errors).length > 0) reason = t('common:run.invalidParams')
  else if (copy && !mode.dir) reason = t('run.chooseFolder')
  else if (plan.loading) reason = t('run.updating')
  else if (plan.error) reason = t('run.fixErrors')
  else if (summary.conflicts > 0) reason = t('run.conflicts', { count: summary.conflicts })
  else if (summary.changed === 0) reason = t('run.nothing')

  const label = copy ? t('run.copy', { count: summary.changed }) : t('run.rename', { count: summary.changed })

  const confirm = async () => {
    setBusy(true)
    // Errors (e.g. the disk changed since the preview) are reported by a toast.
    await executeRename(
      tabId,
      files.map((f) => f.path),
      toRenameParams(params),
      mode,
    )
    setBusy(false)
    setConfirming(false)
  }

  const revertLast = async () => {
    setReverting(true)
    await revertLastRename(tabId)
    setReverting(false)
  }

  return (
    <div className="space-y-2">
      <Button size="lg" className="w-full" disabled={reason != null || busy} onClick={() => setConfirming(true)} data-testid="rename-button">
        <PenLineIcon />
        {label}
      </Button>
      {reason ? <p className="text-center text-xs text-muted-foreground">{reason}</p> : null}
      <Button variant="ghost" size="sm" className="w-full" disabled={reverting} onClick={() => void revertLast()}>
        {reverting ? <Spinner className="size-3.5" /> : <Undo2Icon />}
        {t('actions.revertLast')}
      </Button>

      <Dialog open={confirming} onOpenChange={(o) => !busy && setConfirming(o)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{copy ? t('confirm.titleCopy') : t('confirm.title')}</DialogTitle>
            <DialogDescription>{copy ? t('confirm.descCopy', { dir: mode.kind === 'copyTo' ? mode.dir : '' }) : t('confirm.desc')}</DialogDescription>
          </DialogHeader>
          <ul className="space-y-1 rounded-md border border-border bg-muted/40 p-3 text-sm" data-testid="rename-confirm-counts">
            <li>{copy ? t('confirm.toCopy', { count: summary.changed }) : t('confirm.toRename', { count: summary.changed })}</li>
            {!copy && summary.unchanged > 0 ? <li className="text-muted-foreground">{t('confirm.unchanged', { count: summary.unchanged })}</li> : null}
          </ul>
          <p className="text-xs text-muted-foreground">{t('confirm.revertHint')}</p>
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setConfirming(false)}>
              {t('common:actions.cancel')}
            </Button>
            <Button disabled={busy} onClick={() => void confirm()} data-testid="rename-confirm">
              {busy ? <Spinner className="size-4" /> : null}
              {label}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
