import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { cancelJob, runOp } from '@/lib/ipc'
import { installDomMocks, makeFiles } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { CompareView } from './CompareView'
import { ToolLayout, type ToolLayoutProps } from './ToolLayout'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  cancelJob: vi.fn(() => Promise.resolve()),
}))

function renderLayout(props: Partial<ToolLayoutProps> & { tabId: string }) {
  return render(
    <TooltipProvider>
      <ToolLayout {...props} />
    </TooltipProvider>,
  )
}

describe('ToolLayout', () => {
  let tabId = ''
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    vi.mocked(runOp).mockClear()
    vi.mocked(cancelJob).mockClear()
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('resize')
  })
  afterEach(cleanup)

  it('disables Run with a reason until the tool can run', () => {
    renderLayout({ tabId })
    expect(screen.getByTestId('run-button')).toBeDisabled()
    expect(screen.getByText('This tool is not available yet')).toBeInTheDocument()
    cleanup()
    renderLayout({ tabId, buildRequest: () => ({ kind: 'resize', params: {} }) })
    expect(screen.getByText('Add images first')).toBeInTheDocument()
  })

  it('Run calls runOp with the built request, paths and output; progress + cancel are shown', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(2))
    getSession(tabId)!.getState().setParams({ percent: 50 })
    renderLayout({ tabId, buildRequest: (ctx) => ({ kind: 'resize', params: { percent: ctx.params.percent } }) })
    const run = screen.getByTestId('run-button')
    expect(run).toHaveTextContent('Process 2 images')
    fireEvent.click(run)
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    expect(runOp).toHaveBeenCalledWith(
      tabId,
      { kind: 'resize', params: { percent: 50 } },
      ['C:/textures/tex_0.png', 'C:/textures/tex_1.png'],
      expect.objectContaining({ mode: { kind: 'suffix', suffix: '_opt' }, format: 'keep', jpgQuality: 90 }),
    )
    expect(await screen.findByTestId('job-progress')).toBeInTheDocument()
    act(() => useJobs.getState().handleProgress({ jobId: 'job-1', tabId, done: 1, total: 2, currentPath: 'C:/textures/tex_0.png' }))
    expect(screen.getByText('1 / 2')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Cancel job' }))
    await waitFor(() => expect(cancelJob).toHaveBeenCalledWith('job-1'))
  })

  it('output settings are bound to the session and undoable; quality only for JPEG', () => {
    renderLayout({ tabId })
    fireEvent.click(screen.getByRole('radio', { name: 'Overwrite' }))
    expect(getSession(tabId)!.getState().output.mode).toEqual({ kind: 'inPlace' })
    expect(screen.getByText('Original files will be overwritten.')).toBeInTheDocument()
    act(() => getSession(tabId)!.getState().undo())
    expect(getSession(tabId)!.getState().output.mode).toEqual({ kind: 'suffix', suffix: '_opt' })

    expect(screen.queryByRole('slider', { name: 'Quality' })).not.toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setOutput({ format: 'jpg' }))
    expect(screen.getByRole('slider', { name: 'Quality' })).toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setOutput({ format: 'webp' }))
    expect(screen.queryByRole('slider', { name: 'Quality' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: 'Folder' }))
    expect(getSession(tabId)!.getState().output.mode).toEqual({ kind: 'folder', path: '' })
    expect(screen.getAllByText('No folder selected').length).toBeGreaterThan(0)
  })

  it('shows the preview slot behind an Images / Preview switch', () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    renderLayout({ tabId, preview: (ctx) => <div data-testid="preview">{ctx.focusFile?.name}</div> })
    expect(screen.queryByTestId('preview')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('radio', { name: 'Preview' }))
    expect(screen.getByTestId('preview')).toHaveTextContent('tex_0.png')
    fireEvent.click(screen.getByRole('radio', { name: 'Images' }))
    expect(screen.getByRole('grid')).toBeInTheDocument()
  })
})

describe('CompareView', () => {
  beforeAll(() => installDomMocks())
  afterEach(cleanup)

  it('renders before/after layers with a keyboard-movable divider and pixelated toggle', () => {
    const { container } = render(
      <TooltipProvider>
        <CompareView before="blob:before" after="blob:after" />
      </TooltipProvider>,
    )
    const imgs = container.querySelectorAll('img')
    expect([...imgs].map((i) => i.getAttribute('src'))).toEqual(['blob:before', 'blob:after'])
    const divider = screen.getByRole('slider', { name: 'Comparison divider' })
    expect(divider).toHaveAttribute('aria-valuenow', '50')
    fireEvent.keyDown(divider, { key: 'ArrowLeft' })
    expect(divider).toHaveAttribute('aria-valuenow', '48')
    fireEvent.click(screen.getByRole('switch', { name: 'Pixelated' }))
    expect(imgs[0].style.imageRendering).toBe('pixelated')
    fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }))
    expect(screen.getByText('125%')).toBeInTheDocument()
  })
})
