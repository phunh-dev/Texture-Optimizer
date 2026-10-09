// Additive ToolLayout props used by non-image tools (3D Texture Packer).
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { installDomMocks, makeFiles } from '@/lib/testing/dom'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { ToolLayout } from './ToolLayout'

describe('ToolLayout content / countLabel', () => {
  let tabId = ''
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('meshPack')
  })
  afterEach(cleanup)

  it('content replaces the image grid and its size toggle; countLabel replaces the counter', () => {
    getSession(tabId)!.getState().addFiles(makeFiles(2))
    render(
      <TooltipProvider>
        <ToolLayout tabId={tabId} content={(ctx) => <div data-testid="custom">{ctx.files.length} items</div>} countLabel="2 models" />
      </TooltipProvider>,
    )
    expect(screen.getByTestId('custom')).toHaveTextContent('2 items')
    expect(screen.queryByRole('grid')).not.toBeInTheDocument()
    expect(screen.queryByRole('radiogroup', { name: /size/i })).not.toBeInTheDocument()
    expect(screen.getByTestId('file-count')).toHaveTextContent('2 models')
  })

  it('without content the image grid is unchanged', () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    render(
      <TooltipProvider>
        <ToolLayout tabId={tabId} />
      </TooltipProvider>,
    )
    expect(screen.getByRole('grid')).toBeInTheDocument()
    expect(screen.getByTestId('file-count')).toHaveTextContent('1 image')
  })
})
