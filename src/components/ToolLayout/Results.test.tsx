// Staged results of the image tools: Run only processes, the user reviews and saves.
import { open, save } from '@tauri-apps/plugin-dialog'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { toast } from 'sonner'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { discardResults, listResults, runOp, saveResults } from '@/lib/ipc'
import type { ImportedFile, JobFinishedEvent, SaveReport } from '@/lib/ipc/types'
import { installDomMocks, makeFile } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { useResults } from '@/stores/results'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { ToolLayout } from './ToolLayout'

vi.mock('sonner', () => ({ toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }))
vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  cancelJob: vi.fn(() => Promise.resolve()),
  listResults: vi.fn(),
  saveResults: vi.fn(),
  discardResults: vi.fn(() => Promise.resolve()),
}))

const STAGE = 'C:/cache/staging/tab/job-1'

const source = (name: string, dir = 'D:/art/ui'): ImportedFile =>
  makeFile({ id: `src-${dir}-${name}`, name, path: `${dir}/${name}`, width: 100, height: 50 })
const staged = (name: string, width = 64, height = 32): ImportedFile =>
  makeFile({ id: `staged-${name}`, name, path: `${STAGE}/${name}`, width, height })

function finished(inputs: ImportedFile[], outputs: (string | null)[], meta: unknown[] = []): JobFinishedEvent {
  return {
    jobId: 'job-1',
    tabId: '',
    cancelled: false,
    results: inputs.map((f, i) => ({
      input: f.path,
      output: outputs[i] ? `${STAGE}/${outputs[i]}` : null,
      error: outputs[i] ? null : { code: 'IMG_DECODE_FAILED', params: { path: f.path } },
      meta: meta[i] ?? null,
    })),
  }
}

const report = (patch: Partial<SaveReport> = {}): SaveReport => ({ destination: 'D:/out', saved: [], skipped: [], failed: [], ...patch })

describe('staged results in ToolLayout', () => {
  let tabId = ''
  const session = () => getSession(tabId)!.getState()

  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    useResults.setState({ byTab: {}, saving: {} })
    vi.mocked(runOp).mockClear()
    vi.mocked(listResults).mockReset()
    vi.mocked(saveResults).mockReset()
    vi.mocked(discardResults).mockClear()
    vi.mocked(open).mockReset()
    vi.mocked(save).mockReset()
    for (const fn of [toast.success, toast.error, toast.warning, toast.info]) vi.mocked(fn).mockClear()
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('resize')
  })
  afterEach(cleanup)

  function renderTool() {
    return render(
      <TooltipProvider>
        <ToolLayout tabId={tabId} buildRequest={(ctx) => ({ kind: 'resize', params: { percent: ctx.params.percent ?? 50 } })} />
      </TooltipProvider>,
    )
  }

  /** Add files, Run, and finish the job with the given staged outputs. */
  async function runAndFinish(inputs: ImportedFile[], outputs: (string | null)[], listed: ImportedFile[], meta: unknown[] = []) {
    act(() => {
      session().addFiles(inputs)
    })
    vi.mocked(listResults).mockResolvedValue(listed.map((file) => ({ file, sidecar: null })))
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    await waitFor(() => expect(useJobs.getState().byTab[tabId]?.jobId).toBe('job-1'))
    act(() => useJobs.getState().handleFinished({ ...finished(inputs, outputs, meta), tabId }))
    await screen.findByTestId('results-panel')
  }

  it('Run only processes (never asks where to save); the Result view is disabled until there are results', async () => {
    renderTool()
    const toggle = screen.getByTestId('results-toggle')
    expect(within(toggle).getByRole('radio', { name: 'Result' })).toBeDisabled()
    expect(within(toggle).getByRole('radio', { name: 'Original' })).toHaveAttribute('data-state', 'on')

    await runAndFinish([source('a.png'), source('b.png')], ['a.png', 'b.png'], [staged('a.png'), staged('b.png')])
    expect(save).not.toHaveBeenCalled()
    expect(open).not.toHaveBeenCalled()
    expect(saveResults).not.toHaveBeenCalled()
    expect(listResults).toHaveBeenCalledWith(tabId, 'job-1')
    expect(toast.success).toHaveBeenCalledWith('Processed 2 images', { description: 'Not saved yet: review the results, then click Save…' })
  })

  it('shows a results state with summary and an Original / Result toggle over the staged thumbnails', async () => {
    renderTool()
    const warn = { outputWarnings: [{ code: 'OUTPUT_FORMAT_CHANGED', params: { path: `${STAGE}/c.png`, from: 'jpg', to: 'png' } }] }
    await runAndFinish(
      [source('a.png'), source('broken.png'), source('c.jpg')],
      ['a.png', null, 'c.png'],
      [staged('a.png', 60, 30), staged('c.png', 32, 32)],
      [null, null, warn],
    )
    const panel = screen.getByTestId('results-panel')
    expect(within(panel).getByText('Results — not saved yet')).toBeInTheDocument()
    expect(within(panel).getByTestId('results-summary')).toHaveTextContent('2 processed · 1 failed · 1 warning')
    expect(within(panel).getByText(/temporary folder/)).toBeInTheDocument()
    expect(screen.queryByTestId('results-stale')).not.toBeInTheDocument()

    // The Result view is shown after a run: staged files, same cell UI (size + warnings).
    const results = screen.getByRole('grid', { name: 'Results' })
    expect(screen.getByTestId('results-toggle').querySelector('[data-state="on"]')).toHaveTextContent('Result')
    const cells = within(results).getAllByRole('gridcell')
    expect(cells.map((c) => c.getAttribute('aria-label'))).toEqual(['a.png', 'c.png'])
    expect(within(cells[0]).getByText('60 × 30')).toBeInTheDocument()
    expect(within(cells[0]).getByTestId('badge-npot')).toBeInTheDocument()
    expect(within(cells[1]).getByTestId('badge-note')).toHaveAttribute('aria-label', 'Kept as PNG to preserve transparency (JPEG cannot store it)')
    expect(within(results).queryByTestId('remove-badge')).not.toBeInTheDocument()
    expect(within(results).queryByTestId('add-tile')).not.toBeInTheDocument()

    fireEvent.click(within(screen.getByTestId('results-toggle')).getByRole('radio', { name: 'Original' }))
    const originals = screen.getByRole('grid', { name: 'Images' })
    const names = within(originals)
      .getAllByRole('gridcell')
      .map((c) => c.getAttribute('aria-label'))
      .filter(Boolean)
    expect(names).toEqual(['a.png', 'broken.png', 'c.jpg'])
    expect(within(originals).getByTestId('add-tile')).toBeInTheDocument()
    // Results are not part of undo history.
    expect(session().lastAction?.key).toBe('addFiles')
    fireEvent.click(within(screen.getByTestId('results-toggle')).getByRole('radio', { name: 'Result' }))
    expect(screen.getByRole('grid', { name: 'Results' })).toBeInTheDocument()
  })

  it('Save… with one result opens Save As (source folder + staged name) and saves to the chosen path', async () => {
    renderTool()
    await runAndFinish([source('hero.jpg')], ['hero.png'], [staged('hero.png')])
    vi.mocked(save).mockResolvedValue('D:/out/Hero final.png')
    vi.mocked(saveResults).mockResolvedValue(
      report({ destination: 'D:/out', saved: [{ from: `${STAGE}/hero.png`, to: 'D:/out/Hero final.png', sidecar: null }] }),
    )
    fireEvent.click(screen.getByTestId('save-results'))
    await waitFor(() => expect(saveResults).toHaveBeenCalled())
    expect(save).toHaveBeenCalledWith({ defaultPath: 'D:/art/ui/hero.png', filters: [{ name: 'PNG image', extensions: ['png'] }] })
    expect(open).not.toHaveBeenCalled()
    expect(saveResults).toHaveBeenCalledWith(tabId, 'job-1', { kind: 'file', path: 'D:/out/Hero final.png' }, 'autoRename', session().output)
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Saved 1 file to D:/out'))
  })

  it('Save… with several results asks for a folder (default: folder of the first source) and reports skipped files', async () => {
    renderTool()
    await runAndFinish([source('a.png', 'D:\\art\\ui'), source('b.png', 'E:/other')], ['a.png', 'b.png'], [staged('a.png'), staged('b.png')])
    act(() => session().setOutput({ conflict: 'skip' }))
    // An output change after the run makes the results outdated; confirm to save anyway.
    vi.mocked(open).mockResolvedValue('D:/export')
    vi.mocked(saveResults).mockResolvedValue(
      report({ destination: 'D:/export', saved: [{ from: `${STAGE}/b.png`, to: 'D:/export/b.png', sidecar: null }], skipped: ['D:/export/a.png'] }),
    )
    fireEvent.click(screen.getByTestId('save-results'))
    fireEvent.click(await screen.findByTestId('confirm-save-results'))
    await waitFor(() => expect(saveResults).toHaveBeenCalled())
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, defaultPath: 'D:\\art\\ui' })
    expect(save).not.toHaveBeenCalled()
    expect(saveResults).toHaveBeenCalledWith(tabId, 'job-1', { kind: 'folder', path: 'D:/export' }, 'skip', session().output)
    await waitFor(() => expect(toast.warning).toHaveBeenCalledWith('Saved 1 file to D:/export, skipped 1 that already exist'))

    // Saving can be repeated to another place.
    vi.mocked(open).mockResolvedValue('F:/backup')
    vi.mocked(saveResults).mockResolvedValue(report({ destination: 'F:/backup', saved: [{ from: 'x', to: 'y', sidecar: null }, { from: 'x2', to: 'y2', sidecar: null }] }))
    fireEvent.click(screen.getByTestId('save-results'))
    fireEvent.click(await screen.findByTestId('confirm-save-results'))
    await waitFor(() => expect(saveResults).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Saved 2 files to F:/backup'))
  })

  it('cancelling the native dialog does nothing', async () => {
    renderTool()
    await runAndFinish([source('a.png')], ['a.png'], [staged('a.png')])
    vi.mocked(save).mockResolvedValue(null)
    fireEvent.click(screen.getByTestId('save-results'))
    await waitFor(() => expect(save).toHaveBeenCalled())
    await act(async () => {
      await Promise.resolve()
    })
    expect(saveResults).not.toHaveBeenCalled()
    expect(toast.success).toHaveBeenCalledTimes(1) // only the run summary
    expect(toast.error).not.toHaveBeenCalled()
    expect(screen.getByTestId('results-panel')).toBeInTheDocument()
  })

  it('cancelling the folder picker does nothing', async () => {
    renderTool()
    await runAndFinish([source('a.png'), source('b.png')], ['a.png', 'b.png'], [staged('a.png'), staged('b.png')])
    vi.mocked(open).mockResolvedValue(null)
    fireEvent.click(screen.getByTestId('save-results'))
    await waitFor(() => expect(open).toHaveBeenCalled())
    await act(async () => {
      await Promise.resolve()
    })
    expect(saveResults).not.toHaveBeenCalled()
    expect(toast.error).not.toHaveBeenCalled()
  })

  it('marks results as outdated after a param change; Save then asks for confirmation', async () => {
    renderTool()
    await runAndFinish([source('a.png'), source('b.png')], ['a.png', 'b.png'], [staged('a.png'), staged('b.png')])
    expect(screen.queryByTestId('results-stale')).not.toBeInTheDocument()
    act(() => session().setParams({ percent: 25 }))
    expect(screen.getByTestId('results-stale')).toHaveTextContent('Outdated')
    expect(screen.getByTestId('results-stale-warning')).toHaveTextContent('Files or settings changed after processing')

    vi.mocked(open).mockResolvedValue(null)
    fireEvent.click(screen.getByTestId('save-results'))
    expect(await screen.findByTestId('results-confirm-text')).toHaveTextContent('These results were made before your latest changes')
    expect(open).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByTestId('results-confirm-text')).not.toBeInTheDocument())
    expect(open).not.toHaveBeenCalled()

    // Undo restores the run's params: the results are current again (results themselves are not undone).
    act(() => session().undo())
    expect(screen.queryByTestId('results-stale')).not.toBeInTheDocument()
    expect(screen.getByTestId('results-panel')).toBeInTheDocument()

    // Adding a file also makes them outdated.
    act(() => {
      session().addFiles([source('c.png')])
    })
    expect(screen.getByTestId('results-stale')).toBeInTheDocument()
  })

  it('Discard deletes the staged results and returns to the originals', async () => {
    renderTool()
    await runAndFinish([source('a.png'), source('b.png')], ['a.png', 'b.png'], [staged('a.png'), staged('b.png')])
    fireEvent.click(screen.getByTestId('discard-results'))
    await waitFor(() => expect(discardResults).toHaveBeenCalledWith(tabId))
    expect(screen.queryByTestId('results-panel')).not.toBeInTheDocument()
    expect(screen.getByRole('grid', { name: 'Images' })).toBeInTheDocument()
    expect(within(screen.getByTestId('results-toggle')).getByRole('radio', { name: 'Result' })).toBeDisabled()
  })

  it('a new Run replaces the previous results', async () => {
    renderTool()
    await runAndFinish([source('a.png'), source('b.png')], ['a.png', 'b.png'], [staged('a.png'), staged('b.png')])
    act(() => useJobs.setState({ byTab: {} }))
    vi.mocked(runOp).mockResolvedValueOnce('job-2')
    fireEvent.click(screen.getByTestId('run-button'))
    // Old results are dropped as soon as the new run starts.
    await waitFor(() => expect(screen.queryByTestId('results-panel')).not.toBeInTheDocument())
    expect(runOp).toHaveBeenCalledTimes(2)
  })
})
