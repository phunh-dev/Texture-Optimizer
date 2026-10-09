import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { validateParams } from '@/components/ParamForm'
import { RunPanel } from '@/components/ToolLayout'
import { useSession } from '@/stores/session'

import { packWithDialog } from './run'
import { isValidBaseName, meshSchema } from './schema'

/**
 * Bottom Pack… button (replaces ToolLayout's Run): opens the folder picker
 * first and only starts the job once a folder is chosen, so a cancelled
 * picker never creates a job. Progress / cancel come from RunPanel.
 */
export function PackRunPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation(['mesh', 'common'])
  const files = useSession(tabId, (s) => s.files)
  const params = useSession(tabId, (s) => s.params)
  const invalid = useMemo(() => Object.keys(validateParams(meshSchema, params)).length > 0, [params])
  const baseName = typeof params.baseName === 'string' ? params.baseName : ''
  const [asking, setAsking] = useState(false)

  let reason: string | null = null
  if (files.length === 0) reason = t('mesh:run.noModels')
  else if (invalid) reason = t('common:run.invalidParams')
  else if (!isValidBaseName(baseName)) reason = t('mesh:run.badBaseName')
  // While the picker is open the button is disabled without a message.
  else if (asking) reason = ''

  const onRun = () => {
    setAsking(true)
    void packWithDialog(tabId).finally(() => setAsking(false))
  }

  return (
    <RunPanel tabId={tabId} fileCount={files.length} disabledReason={reason} onRun={onRun} label={t('mesh:run.label', { count: files.length })} />
  )
}
