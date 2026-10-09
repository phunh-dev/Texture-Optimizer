import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { installDomMocks } from '@/lib/testing/dom'
import { useSettings } from '@/stores/settings'
import { useTabs } from '@/stores/tabs'

import { HomeScreen } from './HomeScreen'
import { SettingsPanel } from './SettingsDialog'

function renderUi() {
  return render(
    <TooltipProvider>
      <SettingsPanel />
      <HomeScreen />
    </TooltipProvider>,
  )
}

describe('settings & i18n', () => {
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    localStorage.clear()
    document.documentElement.removeAttribute('data-theme')
    useTabs.setState({ tabs: [], activeTabId: null })
    useSettings.setState({ language: 'en', theme: 'system', recursiveImport: true, loaded: false })
    await i18n.changeLanguage('en')
  })
  afterEach(cleanup)

  it('switching language re-renders UI text at runtime and persists the choice', async () => {
    renderUi()
    expect(screen.getByText('What do you want to optimize today?')).toBeInTheDocument()
    expect(screen.getByText('Resize')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: 'Tiếng Việt' }))
    await waitFor(() => expect(screen.getByText('Hôm nay bạn muốn tối ưu gì?')).toBeInTheDocument())
    expect(screen.getByText('Đổi kích thước')).toBeInTheDocument()
    expect(screen.getByText('Giao diện')).toBeInTheDocument()
    expect(i18n.language).toBe('vi')
    expect(document.documentElement.lang).toBe('vi')
    await waitFor(() => expect(localStorage.getItem('settings.json:language')).toBe('"vi"'))

    fireEvent.click(screen.getByRole('radio', { name: 'English' }))
    await waitFor(() => expect(screen.getByText('What do you want to optimize today?')).toBeInTheDocument())
    expect(localStorage.getItem('settings.json:language')).toBe('"en"')
  })

  it('theme light/dark/system sets data-theme on the root and persists', () => {
    renderUi()
    fireEvent.click(screen.getByRole('radio', { name: 'Dark' }))
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(localStorage.getItem('settings.json:theme')).toBe('"dark"')
    fireEvent.click(screen.getByRole('radio', { name: 'Light' }))
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
    fireEvent.click(screen.getByRole('radio', { name: 'System' }))
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
  })

  it('restores persisted settings on start', async () => {
    localStorage.setItem('settings.json:language', '"vi"')
    localStorage.setItem('settings.json:theme', '"dark"')
    localStorage.setItem('settings.json:recursiveImport', 'false')
    await act(() => useSettings.getState().init())
    expect(i18n.language).toBe('vi')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(useSettings.getState()).toMatchObject({ language: 'vi', theme: 'dark', recursiveImport: false, loaded: true })
  })

  it('defaults to the OS locale when nothing is persisted', async () => {
    await act(() => useSettings.getState().init())
    // jsdom reports en-US
    expect(useSettings.getState().language).toBe('en')
  })

  it('opening a tool from the home screen creates a tab', () => {
    renderUi()
    fireEvent.click(screen.getByTestId('tool-card-atlas'))
    expect(useTabs.getState().tabs.map((t) => t.toolId)).toEqual(['atlas'])
  })
})
