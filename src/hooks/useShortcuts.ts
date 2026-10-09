import { useEffect } from 'react'

import { hasModifier, isEditableTarget } from '@/lib/utils'
import { getSession } from '@/stores/session'
import { useTabs } from '@/stores/tabs'
import { useUi } from '@/stores/ui'

/** Undo/redo for the active tab. Returns true when the event was handled. */
export function handleHistoryShortcut(e: KeyboardEvent): boolean {
  if (!hasModifier(e) || e.altKey) return false
  // Inside a text field the browser's own text undo must keep working.
  if (isEditableTarget(e.target)) return false
  const key = e.key.toLowerCase()
  const isUndo = key === 'z' && !e.shiftKey
  const isRedo = (key === 'z' && e.shiftKey) || (key === 'y' && e.ctrlKey && !e.shiftKey)
  if (!isUndo && !isRedo) return false
  const tabId = useTabs.getState().activeTabId
  const session = tabId ? getSession(tabId) : undefined
  if (!session) return false
  e.preventDefault()
  if (isUndo) session.getState().undo()
  else session.getState().redo()
  return true
}

/** Tab management shortcuts: Ctrl+T new tab, Ctrl+W close, Ctrl+(Shift+)Tab cycle. */
export function handleTabShortcut(e: KeyboardEvent): boolean {
  if (!hasModifier(e) || e.altKey) return false
  const tabs = useTabs.getState()
  const key = e.key.toLowerCase()
  if (key === 't' && !e.shiftKey) {
    e.preventDefault()
    useUi.getState().setToolPickerOpen(true)
    return true
  }
  if (key === 'w' && !e.shiftKey && tabs.activeTabId) {
    e.preventDefault()
    tabs.closeTab(tabs.activeTabId)
    return true
  }
  if (e.key === 'Tab' && e.ctrlKey) {
    e.preventDefault()
    tabs.cycleTab(e.shiftKey ? -1 : 1)
    return true
  }
  return false
}

/** Installs the global keyboard shortcuts (window keydown). */
export function useGlobalShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return
      if (handleHistoryShortcut(e)) return
      handleTabShortcut(e)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
}
