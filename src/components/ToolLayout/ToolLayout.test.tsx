import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
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
    cleanup()
    // Tools whose inputs are not images (e.g. 3D models) can say so.
    renderLayout({ tabId, run: () => Promise.resolve('job-x'), noFilesReason: 'Add models first' })
    expect(screen.getByText('Add models first')).toBeInTheDocument()
    expect(screen.queryByText('Add images first')).not.toBeInTheDocument()
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
      expect.objectContaining({ format: 'keep', jpgQuality: 90, conflict: 'autoRename' }),
    )
    // No destination is sent: runs are staged, the user saves afterwards.
    expect(vi.mocked(runOp).mock.calls[0][3]).not.toHaveProperty('mode')
    expect(await screen.findByTestId('job-progress')).toBeInTheDocument()
    act(() => useJobs.getState().handleProgress({ jobId: 'job-1', tabId, done: 1, total: 2, currentPath: 'C:/textures/tex_0.png' }))
    expect(screen.getByText('1 / 2')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Cancel job' }))
    await waitFor(() => expect(cancelJob).toHaveBeenCalledWith('job-1'))
  })

  it('output settings are bound to the session and undoable; quality only for JPEG', () => {
    renderLayout({ tabId })
    fireEvent.click(screen.getByRole('radio', { name: 'Smallest' }))
    expect(getSession(tabId)!.getState().output.pngCompression).toBe('best')
    act(() => getSession(tabId)!.getState().undo())
    expect(getSession(tabId)!.getState().output.pngCompression).toBe('default')

    expect(screen.queryByRole('slider', { name: 'Quality' })).not.toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setOutput({ format: 'jpg' }))
    expect(screen.getByRole('slider', { name: 'Quality' })).toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setOutput({ format: 'webp' }))
    expect(screen.queryByRole('slider', { name: 'Quality' })).not.toBeInTheDocument()
  })

  it('the output panel no longer offers destination modes; the conflict rule applies when saving', () => {
    renderLayout({ tabId, buildRequest: () => ({ kind: 'resize', params: {} }) })
    const panel = screen.getByTestId('output-settings')
    for (const name of ['Destination', 'Overwrite', 'Folder', 'Suffix', 'File name suffix', 'Original files will be overwritten.']) {
      expect(within(panel).queryByRole('radio', { name })).not.toBeInTheDocument()
      expect(within(panel).queryByText(name)).not.toBeInTheDocument()
    }
    expect(within(panel).queryByRole('button', { name: 'Choose folder' })).not.toBeInTheDocument()
    expect(within(panel).getByText('If the file exists')).toBeInTheDocument()
    expect(within(panel).getByTestId('output-save-hint')).toHaveTextContent('Nothing is written until you click Save…')
    expect(getSession(tabId)!.getState().output).not.toHaveProperty('mode')
  })

  it('Run is labelled as processing only, with a "not saved" hint', () => {
    getSession(tabId)!.getState().addFiles(makeFiles(3))
    renderLayout({ tabId, buildRequest: () => ({ kind: 'resize', params: {} }) })
    const run = screen.getByTestId('run-button')
    expect(run).toHaveTextContent('Process 3 images')
    expect(run).toHaveAttribute('title', 'Processes into a temporary area. Nothing is saved until you click Save…')
  })

  it('tools with their own output (showOutput=false) keep the plain Run: no Original / Result toggle', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    const run = vi.fn(() => Promise.resolve('job-x'))
    renderLayout({ tabId, run, showOutput: false })
    expect(screen.queryByTestId('results-toggle')).not.toBeInTheDocument()
    expect(screen.getByTestId('run-button')).not.toHaveAttribute('title')
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(run).toHaveBeenCalled())
    expect(runOp).not.toHaveBeenCalled()
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

  it('runPanel replaces the Run button area', () => {
    renderLayout({ tabId, runPanel: <div data-testid="custom-run" /> })
    expect(screen.getByTestId('custom-run')).toBeInTheDocument()
    expect(screen.queryByTestId('run-button')).not.toBeInTheDocument()
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

  it('pickMode reports image pixel coordinates through zoom/pan instead of panning', () => {
    const onPick = vi.fn()
    const { container, rerender } = render(
      <TooltipProvider>
        <CompareView before="blob:before" after="blob:after" onPickBefore={onPick} />
      </TooltipProvider>,
    )
    const view = screen.getByRole('img', { name: 'Before / after comparison' })
    fireEvent.pointerDown(view, { button: 0, clientX: 10, clientY: 10 })
    expect(onPick).not.toHaveBeenCalled()

    rerender(
      <TooltipProvider>
        <CompareView before="blob:before" after="blob:after" pickMode onPickBefore={onPick} />
      </TooltipProvider>,
    )
    expect(view).toHaveAttribute('data-pick-mode', 'true')
    // Natural size 1×1 in jsdom after load: clicks outside the image are ignored.
    fireEvent.load(container.querySelector('img')!)
    fireEvent.pointerDown(view, { button: 0, clientX: 990, clientY: 790 })
    expect(onPick).not.toHaveBeenCalled()
    // Fit zooms the 1×1 image to the 64× maximum, centred at (500, 400): it covers 468..532 × 368..432.
    fireEvent.pointerDown(view, { button: 0, clientX: 500, clientY: 400 })
    expect(onPick).toHaveBeenCalledWith(0, 0)
  })
})
