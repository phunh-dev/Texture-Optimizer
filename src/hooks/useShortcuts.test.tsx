import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { HistoryButtons } from '@/components/ToolLayout/HistoryButtons'
import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { makeFiles } from '@/lib/testing/dom'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'
import { useUi } from '@/stores/ui'

import { useGlobalShortcuts } from './useShortcuts'

function Harness() {
  useGlobalShortcuts()
  return (
    <div>
      <input aria-label="name" />
      <textarea aria-label="notes" />
      <input type="checkbox" aria-label="flag" />
    </div>
  )
}

const ids = (tabId: string) => getSession(tabId)!.getState().files.map((f) => f.id)

describe('keyboard shortcuts', () => {
  let tabId = ''
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useUi.setState({ toolPickerOpen: false })
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('resize')
    const s = getSession(tabId)!.getState()
    s.addFiles(makeFiles(3))
    s.removeFiles(['f1'])
  })
  afterEach(cleanup)

  it('Ctrl+Z undoes, Ctrl+Shift+Z and Ctrl+Y redo, Cmd+Z undoes on macOS', () => {
    render(<Harness />)
    fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true })
    expect(ids(tabId)).toEqual(['f0', 'f1', 'f2'])
    fireEvent.keyDown(document.body, { key: 'Z', ctrlKey: true, shiftKey: true })
    expect(ids(tabId)).toEqual(['f0', 'f2'])
    fireEvent.keyDown(document.body, { key: 'z', metaKey: true })
    expect(ids(tabId)).toEqual(['f0', 'f1', 'f2'])
    fireEvent.keyDown(document.body, { key: 'y', ctrlKey: true })
    expect(ids(tabId)).toEqual(['f0', 'f2'])
  })

  it('does not hijack native text undo inside editable fields', () => {
    render(<Harness />)
    const input = screen.getByRole('textbox', { name: 'name' })
    const notPrevented = fireEvent.keyDown(input, { key: 'z', ctrlKey: true })
    expect(notPrevented).toBe(true)
    expect(ids(tabId)).toEqual(['f0', 'f2'])
    fireEvent.keyDown(screen.getByRole('textbox', { name: 'notes' }), { key: 'y', ctrlKey: true })
    expect(ids(tabId)).toEqual(['f0', 'f2'])
    // A checkbox is not a text field: app undo applies.
    fireEvent.keyDown(screen.getByRole('checkbox', { name: 'flag' }), { key: 'z', ctrlKey: true })
    expect(ids(tabId)).toEqual(['f0', 'f1', 'f2'])
  })

  it('Ctrl+T opens the tool picker and Ctrl+W closes the active tab', () => {
    render(<Harness />)
    fireEvent.keyDown(document.body, { key: 't', ctrlKey: true })
    expect(useUi.getState().toolPickerOpen).toBe(true)
    fireEvent.keyDown(document.body, { key: 'w', ctrlKey: true })
    expect(useTabs.getState().tabs).toHaveLength(0)
  })
})

describe('HistoryButtons', () => {
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    await i18n.changeLanguage('en')
  })
  afterEach(cleanup)

  it('labels Undo/Redo with the step and updates as history moves', () => {
    const tabId = useTabs.getState().openTab('resize')
    render(
      <TooltipProvider>
        <HistoryButtons tabId={tabId} />
      </TooltipProvider>,
    )
    expect(screen.getByTestId('undo-button')).toBeDisabled()
    expect(screen.getByTestId('undo-button')).toHaveAccessibleName('Nothing to undo')

    act(() => {
      const s = getSession(tabId)!.getState()
      s.addFiles(makeFiles(5))
      s.removeFiles(['f0', 'f1', 'f2'])
    })
    expect(screen.getByTestId('undo-button')).toHaveAccessibleName('Undo: Remove 3 images')
    expect(screen.getByTestId('redo-button')).toBeDisabled()

    fireEvent.click(screen.getByTestId('undo-button'))
    expect(ids(tabId)).toEqual(['f0', 'f1', 'f2', 'f3', 'f4'])
    expect(screen.getByTestId('undo-button')).toHaveAccessibleName('Undo: Add 5 images')
    expect(screen.getByTestId('redo-button')).toHaveAccessibleName('Redo: Remove 3 images')

    fireEvent.click(screen.getByTestId('redo-button'))
    expect(ids(tabId)).toEqual(['f3', 'f4'])
  })
})
