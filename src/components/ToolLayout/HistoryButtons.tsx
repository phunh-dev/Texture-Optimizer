import { Redo2Icon, Undo2Icon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Tooltip } from '@/components/ui/tooltip'
import { redoTooltip, undoTooltip } from '@/lib/history'
import { requireSession, useSessionHistory } from '@/stores/session'

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)
export const MOD_KEY = isMac ? '⌘' : 'Ctrl'

/** Undo / Redo buttons whose tooltips describe the step ("Undo: Remove 3 images"). */
export function HistoryButtons({ tabId }: { tabId: string }) {
  const { t } = useTranslation('common')
  const history = useSessionHistory(tabId)
  const undoText = undoTooltip(t, history.undoAction)
  const redoText = redoTooltip(t, history.redoAction)
  return (
    <div className="flex items-center">
      <Tooltip content={undoText} shortcut={`${MOD_KEY}+Z`}>
        <span>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={undoText}
            data-testid="undo-button"
            disabled={!history.canUndo}
            onClick={() => requireSession(tabId).getState().undo()}
          >
            <Undo2Icon />
          </Button>
        </span>
      </Tooltip>
      <Tooltip content={redoText} shortcut={`${MOD_KEY}+Shift+Z`}>
        <span>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={redoText}
            data-testid="redo-button"
            disabled={!history.canRedo}
            onClick={() => requireSession(tabId).getState().redo()}
          >
            <Redo2Icon />
          </Button>
        </span>
      </Tooltip>
    </div>
  )
}
