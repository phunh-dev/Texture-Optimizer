import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { validateParams } from '@/components/ParamForm'
import { RunPanel } from '@/components/ToolLayout'
import { useSession } from '@/stores/session'

import { exportWithDialog } from './export'
import { atlasSchema } from './schema'

/**
 * Bottom Export… button (replaces ToolLayout's Run): opens the Save As
 * dialog first and only starts the job once a location is confirmed, so a
 * cancelled dialog never creates a job. Progress / cancel come from RunPanel.
 */
export function ExportRunPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation(['atlas', 'common'])
  const files = useSession(tabId, (s) => s.files)
  const params = useSession(tabId, (s) => s.params)
  const invalid = useMemo(() => Object.keys(validateParams(atlasSchema, params)).length > 0, [params])
  const [asking, setAsking] = useState(false)

  let reason: string | null = null
  if (files.length === 0) reason = t('common:run.noFiles')
  else if (invalid) reason = t('common:run.invalidParams')
  // While the dialog is open the button is disabled without a message.
  else if (asking) reason = ''

  const onRun = () => {
    setAsking(true)
    void exportWithDialog(tabId).finally(() => setAsking(false))
  }

  return <RunPanel tabId={tabId} fileCount={files.length} disabledReason={reason} onRun={onRun} label={t('atlas:run.export')} />
}
