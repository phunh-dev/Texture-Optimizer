import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { toast } from 'sonner'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { installDomMocks, makeFiles } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { useAtlasRefresh } from './hooks'
import AtlasTab from './index'
import { atlasExport, atlasLoadProject, atlasPreview, type AtlasExportSummary } from './ipc'
import { atlasDefaults, buildAtlasRequest } from './schema'

vi.mock('./ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./ipc')>()),
  atlasPreview: vi.fn(() => new Promise(() => undefined)),
  atlasExport: vi.fn(() => Promise.resolve('job-7')),
  atlasLoadProject: vi.fn(() => Promise.resolve(null)),
}))

vi.mock('sonner', async (importOriginal) => {
  const actual = await importOriginal<typeof import('sonner')>()
  return { ...actual, toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }) }
})

function renderTab(tabId: string) {
  return render(
    <TooltipProvider>
      <AtlasTab tabId={tabId} />
    </TooltipProvider>,
  )
}

// Each test mounts the whole tab (every param group open); allow for slow CI machines.
describe('AtlasTab', { timeout: 30_000 }, () => {
  let tabId = ''
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    vi.mocked(atlasExport).mockClear()
    vi.mocked(atlasLoadProject).mockClear()
    vi.mocked(atlasPreview).mockClear()
    vi.mocked(toast.success).mockClear()
    vi.mocked(toast.error).mockClear()
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('atlas')
  })
  afterEach(cleanup)

  it('a new atlas tab starts from the schema defaults', () => {
    expect(getSession(tabId)!.getState().params).toEqual(atlasDefaults())
  })

  it('groups every parameter; the heuristic list follows the algorithm', () => {
    renderTab(tabId)
    for (const g of ['Packing', 'Atlas size', 'Spacing', 'Sprites', 'Pages', 'Color', 'Exporter']) {
      expect(screen.getByRole('button', { name: g })).toBeInTheDocument()
    }
    expect(screen.getByRole('combobox', { name: 'Placement heuristic' })).toHaveTextContent('Best short side fit')
    fireEvent.click(screen.getByRole('radio', { name: 'Skyline' }))
    expect(getSession(tabId)!.getState().params.algorithm).toBe('skyline')
    expect(screen.getAllByRole('combobox', { name: 'Placement heuristic' })).toHaveLength(1)
    expect(screen.getByRole('combobox', { name: 'Placement heuristic' })).toHaveTextContent('Bottom left')
    // Trim threshold only while trimming.
    expect(screen.getByRole('slider', { name: 'Trim alpha threshold' })).toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setParams({ trim: false }))
    expect(screen.queryByRole('slider', { name: 'Trim alpha threshold' })).not.toBeInTheDocument()
  })

  it('shows only the selected exporter options and greys out unsupported rotation', () => {
    renderTab(tabId)
    const exporterSelect = screen.getByRole('combobox', { name: 'Export for' })
    expect(exporterSelect).toHaveTextContent('Generic JSON (TexturePacker)')
    expect(screen.getByText('Frames layout')).toBeInTheDocument()
    expect(screen.queryByText('Pixels per unit')).not.toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Allow rotation' })).toBeEnabled()

    act(() => getSession(tabId)!.getState().setParams({ exporter: 'unity' }))
    for (const label of ['Unity version', 'Pixels per unit', 'Filter mode', 'Compression', 'Max texture size', 'Generate mipmaps', 'Pivot', 'Keep pivot of trimmed sprites']) {
      expect(screen.getByText(label)).toBeInTheDocument()
    }
    expect(screen.queryByText('Frames layout')).not.toBeInTheDocument()
    expect(screen.queryByText('Resource folder')).not.toBeInTheDocument()
    expect(screen.queryByText('Pivot X')).not.toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setParams({ 'unity.pivot': 'custom' }))
    expect(screen.getByText('Pivot X')).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Allow rotation' })).toBeDisabled()
    expect(screen.getByText('Unity sprites cannot be rotated in a texture, so rotation is turned off for this exporter.')).toBeInTheDocument()

    act(() => getSession(tabId)!.getState().setParams({ exporter: 'godot' }))
    for (const label of ['Godot version', 'Resource folder', '.tres subfolder', 'Filter clip']) {
      expect(screen.getByText(label)).toBeInTheDocument()
    }
    expect(screen.queryByText('Pixels per unit')).not.toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Allow rotation' })).toBeDisabled()

    act(() => getSession(tabId)!.getState().setParams({ exporter: 'unreal' }))
    expect(screen.getByText('File extension')).toBeInTheDocument()
    expect(screen.getByText('Pivot X')).toBeInTheDocument()
    expect(screen.queryByText('Resource folder')).not.toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Allow rotation' })).toBeEnabled()
    // The generic image output panel is replaced by the atlas output section.
    expect(screen.queryByTestId('output-settings')).not.toBeInTheDocument()
    expect(screen.getByTestId('atlas-output')).toBeInTheDocument()
  })

  it('validates the base name and needs an output folder before exporting', () => {
    getSession(tabId)!.getState().addFiles(makeFiles(2))
    renderTab(tabId)
    const run = screen.getByTestId('run-button')
    expect(run).toHaveTextContent('Export atlas')
    expect(run).toBeDisabled()
    expect(screen.getByText('Choose an output folder first')).toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setParams({ outputDir: 'D:/out' }))
    expect(run).toBeEnabled()

    const base = screen.getByLabelText('Base name')
    fireEvent.change(base, { target: { value: 'ui:icons' } })
    expect(getSession(tabId)!.getState().params.baseName).toBe('ui:icons')
    expect(screen.getByRole('alert')).toHaveTextContent('Base name cannot contain')
    expect(run).toBeDisabled()
    fireEvent.change(base, { target: { value: 'CON' } })
    expect(screen.getByRole('alert')).toHaveTextContent('This name is reserved by Windows')
    fireEvent.change(base, { target: { value: 'ui_icons' } })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByText('Writes ui_icons.png, ui_icons.texatlas.json and the metadata.')).toBeInTheDocument()
    expect(run).toBeEnabled()
  })

  it('incremental options are bound to the session', () => {
    renderTab(tabId)
    fireEvent.click(screen.getByRole('radio', { name: 'Repack optimally' }))
    expect(getSession(tabId)!.getState().params.incrementalMode).toBe('repackOptimal')
    fireEvent.click(screen.getByRole('switch', { name: 'Remove sprites not in the current list' }))
    expect(getSession(tabId)!.getState().params.removeMissing).toBe(true)
    act(() => getSession(tabId)!.getState().undo())
    expect(getSession(tabId)!.getState().params.removeMissing).toBe(false)
  })

  it('summarises the existing atlas at the output', async () => {
    vi.mocked(atlasLoadProject).mockResolvedValue({
      path: 'D:/out/atlas.texatlas.json',
      pages: [{ width: 256, height: 256 }],
      spriteCount: 3,
      frameCount: 3,
      exporter: 'unity',
      params: buildAtlasRequest(atlasDefaults()).params,
      stats: { pages: [], spriteCount: 3, frameCount: 3, occupancy: 0.5 },
      plan: [
        { name: 'a', status: 'kept', sourcePath: null },
        { name: 'b', status: 'replaced', sourcePath: 'C:/textures/b.png' },
        { name: 'c', status: 'new', sourcePath: 'C:/textures/c.png' },
      ],
    })
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    getSession(tabId)!.getState().setParams({ outputDir: 'D:/out' })
    renderTab(tabId)
    const box = screen.getByTestId('atlas-existing')
    await waitFor(() => expect(box).toHaveTextContent('3 sprites on 1 page(s)'))
    expect(box).toHaveTextContent('Last exported for Unity')
    expect(box).toHaveTextContent('1 kept')
    expect(box).toHaveTextContent('1 replaced')
    expect(box).toHaveTextContent('1 new')
    expect(atlasLoadProject).toHaveBeenCalledWith('D:/out/atlas.texatlas.json', ['C:/textures/tex_0.png'], false)
  })

  it('export starts the job with the exact request, tracks it and reports the result', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(2))
    getSession(tabId)!.getState().setParams({ outputDir: 'D:/out', exporter: 'godot', padding: 4 })
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(atlasExport).toHaveBeenCalled())
    expect(atlasExport).toHaveBeenCalledWith(
      tabId,
      ['C:/textures/tex_0.png', 'C:/textures/tex_1.png'],
      buildAtlasRequest({ ...atlasDefaults(), outputDir: 'D:/out', exporter: 'godot', padding: 4 }),
    )
    await waitFor(() => expect(useJobs.getState().byTab[tabId]).toMatchObject({ jobId: 'job-7', status: 'running' }))
    expect(await screen.findByTestId('job-progress')).toBeInTheDocument()
    act(() => useJobs.getState().handleProgress({ jobId: 'job-7', tabId, done: 3, total: 5, currentPath: 'D:/out/atlas.png' }))
    expect(screen.getByText('3 / 5')).toBeInTheDocument()

    const summary: AtlasExportSummary = {
      kind: 'atlasExport',
      outputDir: 'D:/out',
      projectPath: 'D:/out/atlas.texatlas.json',
      written: ['D:/out/atlas.png', 'D:/out/tex_0.tres', 'D:/out/tex_1.tres', 'D:/out/atlas.texatlas.json'],
      deleted: ['D:/out/old.tres'],
      warnings: [],
      stats: { pages: [], spriteCount: 2, frameCount: 2, occupancy: 0.5 },
      plan: [],
    }
    const before = useAtlasRefresh.getState().versions[tabId] ?? 0
    act(() =>
      useJobs.getState().handleFinished({
        jobId: 'job-7',
        tabId,
        cancelled: false,
        results: [
          { input: summary.projectPath, output: summary.projectPath, error: null, meta: summary },
          { input: summary.projectPath, output: 'D:/out/atlas.png', error: null, meta: null },
        ],
      }),
    )
    expect(useJobs.getState().byTab[tabId].status).toBe('done')
    expect(toast.success).toHaveBeenCalledWith('Atlas exported to D:/out', {
      description: '4 files written\n1 outdated file removed',
    })
    expect(useAtlasRefresh.getState().versions[tabId]).toBe(before + 1)
  })

  it('a failed export shows the translated error', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    getSession(tabId)!.getState().setParams({ outputDir: 'D:/out' })
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(useJobs.getState().byTab[tabId]?.jobId).toBe('job-7'))
    act(() =>
      useJobs.getState().handleFinished({
        jobId: 'job-7',
        tabId,
        cancelled: false,
        results: [{ input: 'D:/out/atlas.texatlas.json', output: null, error: { code: 'ATLAS_EMPTY', params: {} }, meta: null }],
      }),
    )
    expect(toast.error).toHaveBeenCalledWith('Atlas export failed', { description: 'The atlas has no sprites' })
  })
})
