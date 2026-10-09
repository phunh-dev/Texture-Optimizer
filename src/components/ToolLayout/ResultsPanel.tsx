import { SaveIcon, Trash2Icon, TriangleAlertIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Badge } from '@/components/ui/misc'
import { discardTabResults, resultSignature, saveTabResults, useResults, useTabResults } from '@/stores/results'
import { useSession } from '@/stores/session'

/** True when files, params or output settings changed since the results were produced. */
export function useResultsStale(tabId: string): boolean {
  const results = useTabResults(tabId)
  const current = useSession(tabId, (s) => (results ? resultSignature(s) : ''))
  return !!results && current !== results.signature
}

/** Summary + Save… / Discard of the tab's staged (not yet saved) results. */
export function ResultsPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('common')
  const results = useTabResults(tabId)
  const saving = useResults((s) => s.saving[tabId] === true)
  const stale = useResultsStale(tabId)
  const [confirming, setConfirming] = useState(false)
  if (!results) return null

  const warnings = results.items.reduce((n, item) => n + item.warnings.length, 0)
  const summary = [
    t('results.processed', { count: results.items.length }),
    results.failed > 0 ? t('results.failed', { count: results.failed }) : null,
    warnings > 0 ? t('results.warnings', { count: warnings }) : null,
  ]
    .filter(Boolean)
    .join(' · ')

  const onSave = () => {
    if (stale) setConfirming(true)
    else void saveTabResults(tabId)
  }

  return (
    <div className="space-y-2 rounded-lg border border-primary/30 bg-primary/5 p-3" data-testid="results-panel">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-semibold">{t('results.title')}</span>
        {stale ? (
          <Badge variant="warning" data-testid="results-stale">
            {t('results.outdated')}
          </Badge>
        ) : null}
      </div>
      <p className="text-xs tabular-nums text-foreground" data-testid="results-summary">
        {summary}
      </p>
      <p className="text-xs text-muted-foreground">{t('results.notSaved')}</p>
      {stale ? (
        <p className="flex items-start gap-1.5 text-xs text-amber-600 dark:text-amber-400" data-testid="results-stale-warning">
          <TriangleAlertIcon className="mt-px size-3.5 shrink-0" />
          {t('results.outdatedHint')}
        </p>
      ) : null}
      <div className="flex gap-2">
        <Button className="flex-1" onClick={onSave} disabled={saving} data-testid="save-results">
          <SaveIcon />
          {t('results.save')}
        </Button>
        <Button variant="outline" onClick={() => void discardTabResults(tabId)} disabled={saving} data-testid="discard-results">
          <Trash2Icon />
          {t('results.discard')}
        </Button>
      </div>

      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('results.confirmStaleTitle')}</DialogTitle>
            <DialogDescription data-testid="results-confirm-text">{t('results.confirmStale')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              {t('actions.cancel')}
            </Button>
            <Button
              data-testid="confirm-save-results"
              onClick={() => {
                setConfirming(false)
                void saveTabResults(tabId)
              }}
            >
              {t('results.saveAnyway')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
