// Pattern Renamer tab: template/numbering/case/find-replace/smart naming with a
// live "Current name → New name" plan, in-place rename or copy to a folder,
// and revert through persisted rename logs.
import { useEffect } from 'react'

import { ToolLayout } from '@/components/ToolLayout'
import { requireSession, useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { renameFields } from './fields'
import { RenamePlanContext, useRenamePlan } from './plan'
import { RenameTable } from './RenameTable'
import { RenameRunPanel } from './RunPanel'
import { renameSchema } from './schema'

export default function RenameTab({ tabId }: ToolTabProps) {
  const plan = useRenamePlan(tabId)
  const copy = useSession(tabId, (s) => s.params.mode === 'copyTo')

  // The plan table is this tool's main view: open it by default (once per tab).
  useEffect(() => {
    const s = requireSession(tabId).getState()
    if (!('preview' in s.uiFlags)) s.setUiFlag('preview', true)
  }, [tabId])

  return (
    <RenamePlanContext.Provider value={plan}>
      <ToolLayout
        tabId={tabId}
        fields={renameFields}
        schema={renameSchema}
        showOutput={false}
        preview={(ctx) => <RenameTable plan={plan} copy={copy} hasFiles={ctx.files.length > 0} />}
        runPanel={<RenameRunPanel tabId={tabId} plan={plan} />}
      />
    </RenamePlanContext.Provider>
  )
}
