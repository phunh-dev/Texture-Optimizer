import { open, save } from '@tauri-apps/plugin-dialog'
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
import { atlasExport, atlasLoadProject, atlasPreview, type AtlasExportSummary, type AtlasProjectSummary } from './ipc'
import { atlasDefaults, buildAtlasRequest } from './schema'

vi.mock('./ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./ipc')>()),
  atlasPreview: vi.fn(() => new Promise(() => undefined)),
  atlasExport: vi.fn(() => Promise.resolve('job-7')),
  atlasLoadProject: vi.fn(() => Promise.resolve(null)),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(() => Promise.resolve(null)),
  save: vi.fn(() => Promise.resolve(null)),
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

function deferred<T>() {
  let resolve!: (v: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}

function projectSummary(path: string, overrides: Partial<AtlasProjectSummary> = {}): AtlasProjectSummary {
  return {
    path,
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
    ...overrides,
  }
}

function exportSummary(dir: string, base: string): AtlasExportSummary {
  return {
    kind: 'atlasExport',
    outputDir: dir,
    projectPath: `${dir}/${base}.texatlas.json`,
    written: [`${dir}/${base}.png`, `${dir}/tex_0.tres`, `${dir}/tex_1.tres`, `${dir}/${base}.texatlas.json`],
    deleted: [`${dir}/old.tres`],
    warnings: [],
    stats: { pages: [], spriteCount: 2, frameCount: 2, occupancy: 0.5 },
    plan: [],
  }
}

// Each test mounts the whole tab (every param group open); allow for slow CI machines.
describe('AtlasTab', { timeout: 30_000 }, () => {
  let tabId = ''
  const params = () => getSession(tabId)!.getState().params
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    vi.mocked(atlasExport).mockClear()
    vi.mocked(atlasLoadProject).mockReset()
    vi.mocked(atlasLoadProject).mockResolvedValue(null)
    vi.mocked(atlasPreview).mockClear()
    vi.mocked(save).mockReset()
    vi.mocked(save).mockResolvedValue(null)
    vi.mocked(open).mockReset()
    vi.mocked(open).mockResolvedValue(null)
    vi.mocked(toast.success).mockClear()
    vi.mocked(toast.error).mockClear()
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('atlas')
  })
  afterEach(cleanup)

  it('a new atlas tab starts from the schema defaults', () => {
    expect(params()).toEqual(atlasDefaults())
  })

  it('groups every parameter; the heuristic list follows the algorithm', () => {
    renderTab(tabId)
    for (const g of ['Packing', 'Atlas size', 'Spacing', 'Sprites', 'Pages', 'Color', 'Exporter']) {
      expect(screen.getByRole('button', { name: g })).toBeInTheDocument()
    }
    expect(screen.getByRole('combobox', { name: 'Placement heuristic' })).toHaveTextContent('Best short side fit')
    fireEvent.click(screen.getByRole('radio', { name: 'Skyline' }))
    expect(params().algorithm).toBe('skyline')
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

    act(() => getSession(tabId)!.getState().setParams({ exporter: 'imageOnly' }))
    expect(exporterSelect).toHaveTextContent('Atlas image only')
    for (const label of ['File extension', 'Pivot X', 'Frames layout', 'Pixels per unit', 'Resource folder']) {
      expect(screen.queryByText(label)).not.toBeInTheDocument()
    }
    expect(screen.getByRole('switch', { name: 'Allow rotation' })).toBeDisabled()
    expect(
      screen.getByText('Without metadata nobody can tell a sprite was rotated, so rotation is turned off for an image-only export.'),
    ).toBeInTheDocument()
    // The generic image output panel is replaced by the atlas output section.
    expect(screen.queryByTestId('output-settings')).not.toBeInTheDocument()
    expect(screen.getByTestId('atlas-output')).toBeInTheDocument()
  })

  it('has no output folder / base name fields: Export… only needs images', () => {
    renderTab(tabId)
    expect(screen.queryByLabelText('Output folder')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Base name')).not.toBeInTheDocument()
    const run = screen.getByTestId('run-button')
    expect(run).toHaveTextContent('Export…')
    expect(run).toBeDisabled()
    expect(screen.getByText('Add images first')).toBeInTheDocument()
    act(() => {
      getSession(tabId)!.getState().addFiles(makeFiles(2))
    })
    expect(run).toBeEnabled()
    // Invalid parameters still block the export.
    act(() => getSession(tabId)!.getState().setParams({ 'godot.resPath': 'C:/x', exporter: 'godot' }))
    expect(run).toBeDisabled()
    expect(screen.getByText('Fix the highlighted parameters first')).toBeInTheDocument()
  })

  it('Export… asks where to save (default: the target atlas) and exports there only after confirmation', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(2))
    getSession(tabId)!.getState().setParams({ outputDir: 'D:/out', baseName: 'ui', exporter: 'godot', padding: 4 })
    const choice = deferred<string | null>()
    vi.mocked(save).mockReturnValueOnce(choice.promise)
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: 'D:/out/ui.png', filters: [{ name: 'PNG image', extensions: ['png'] }] }),
    )
    // Nothing runs while the dialog is open.
    expect(atlasExport).not.toHaveBeenCalled()
    expect(useJobs.getState().byTab[tabId]).toBeUndefined()

    await act(async () => choice.resolve('E:/game/atlases/icons.png'))
    await waitFor(() => expect(atlasExport).toHaveBeenCalledTimes(1))
    expect(atlasExport).toHaveBeenCalledWith(
      tabId,
      ['C:/textures/tex_0.png', 'C:/textures/tex_1.png'],
      buildAtlasRequest({ ...atlasDefaults(), outputDir: 'E:/game/atlases', baseName: 'icons', exporter: 'godot', padding: 4 }),
    )
    await waitFor(() => expect(useJobs.getState().byTab[tabId]).toMatchObject({ jobId: 'job-7', status: 'running' }))
    expect(await screen.findByTestId('job-progress')).toBeInTheDocument()
    act(() => useJobs.getState().handleProgress({ jobId: 'job-7', tabId, done: 3, total: 5, currentPath: 'E:/game/atlases/icons.png' }))
    expect(screen.getByText('3 / 5')).toBeInTheDocument()
    // The target only follows once the export succeeded.
    expect(params()).toMatchObject({ outputDir: 'D:/out', baseName: 'ui' })

    const summary = exportSummary('E:/game/atlases', 'icons')
    const before = useAtlasRefresh.getState().versions[tabId] ?? 0
    act(() =>
      useJobs.getState().handleFinished({
        jobId: 'job-7',
        tabId,
        cancelled: false,
        results: [
          { input: summary.projectPath!, output: summary.projectPath, error: null, meta: summary },
          { input: summary.projectPath!, output: 'E:/game/atlases/icons.png', error: null, meta: null },
        ],
      }),
    )
    expect(useJobs.getState().byTab[tabId].status).toBe('done')
    expect(toast.success).toHaveBeenCalledWith('Atlas exported to E:/game/atlases', {
      description: '4 files written\n1 outdated file removed',
    })
    expect(useAtlasRefresh.getState().versions[tabId]).toBe(before + 1)
    // After a successful export the saved atlas becomes the target (next default, incremental).
    expect(params()).toMatchObject({ outputDir: 'E:/game/atlases', baseName: 'icons' })
    expect(screen.getByLabelText('Target atlas')).toHaveValue('E:/game/atlases/icons.png')
  })

  it('without a target the dialog starts in the first image folder; Windows paths are split correctly', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1, { path: 'C:\\art\\ui\\tex_0.png' }))
    vi.mocked(save).mockResolvedValueOnce('D:\\out\\sheet.png')
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(atlasExport).toHaveBeenCalled())
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: 'C:\\art\\ui\\atlas.png' }))
    expect(atlasExport).toHaveBeenCalledWith(
      tabId,
      ['C:\\art\\ui\\tex_0.png'],
      buildAtlasRequest({ ...atlasDefaults(), outputDir: 'D:\\out', baseName: 'sheet' }),
    )
  })

  it('cancelling the Save As dialog writes nothing', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(save).toHaveBeenCalled())
    await act(async () => undefined)
    expect(atlasExport).not.toHaveBeenCalled()
    expect(useJobs.getState().byTab[tabId]).toBeUndefined()
    expect(toast.error).not.toHaveBeenCalled()
    expect(params()).toEqual({ ...atlasDefaults() })
    expect(screen.getByTestId('run-button')).toBeEnabled()
  })

  it('an invalid file name from the dialog shows the translated error and exports nothing', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    vi.mocked(save).mockResolvedValueOnce('D:/out/CON.png')
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith('This name is reserved by Windows'))
    expect(atlasExport).not.toHaveBeenCalled()
    expect(useJobs.getState().byTab[tabId]).toBeUndefined()
    expect(params().outputDir).toBe('')
  })

  it('a failed export shows the translated error and keeps the target', async () => {
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    vi.mocked(save).mockResolvedValueOnce('D:/out/atlas.png')
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
    expect(params().outputDir).toBe('')
  })

  it('picking a target atlas drives the existing-atlas summary and the preview; it can be cleared', async () => {
    vi.mocked(atlasLoadProject).mockImplementation((path) =>
      Promise.resolve(path === 'D:/proj/ui.texatlas.json' ? projectSummary(path) : null),
    )
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    getSession(tabId)!.getState().setUiFlag('preview', true)
    vi.mocked(open).mockResolvedValueOnce('D:/proj/ui.texatlas.json')
    renderTab(tabId)
    const target = screen.getByLabelText('Target atlas')
    expect(target).toHaveValue('')
    expect(screen.getByRole('button', { name: 'Clear target atlas' })).toBeDisabled()

    fireEvent.click(screen.getByRole('button', { name: 'Choose target atlas' }))
    await waitFor(() => expect(params()).toMatchObject({ outputDir: 'D:/proj', baseName: 'ui' }))
    expect(open).toHaveBeenCalledWith(
      expect.objectContaining({ multiple: false, directory: false, filters: [{ name: 'Atlas image or project', extensions: ['png', 'json'] }] }),
    )
    expect(target).toHaveValue('D:/proj/ui.png')
    const box = screen.getByTestId('atlas-existing')
    await waitFor(() => expect(box).toHaveTextContent('3 sprites on 1 page(s)'))
    expect(box).toHaveTextContent('Last exported for Unity')
    expect(box).toHaveTextContent('1 kept')
    expect(box).toHaveTextContent('1 replaced')
    expect(box).toHaveTextContent('1 new')
    expect(atlasLoadProject).toHaveBeenCalledWith('D:/proj/ui.texatlas.json', ['C:/textures/tex_0.png'], false)
    await waitFor(() =>
      expect(atlasPreview).toHaveBeenLastCalledWith(
        tabId,
        ['C:/textures/tex_0.png'],
        buildAtlasRequest({ ...atlasDefaults(), outputDir: 'D:/proj', baseName: 'ui' }),
      ),
    )

    // The target is the default location of Export….
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: 'D:/proj/ui.png' })))

    fireEvent.click(screen.getByRole('button', { name: 'Clear target atlas' }))
    expect(params().outputDir).toBe('')
    expect(target).toHaveValue('')
    expect(screen.queryByTestId('atlas-existing')).not.toBeInTheDocument()
    await waitFor(() =>
      expect(atlasPreview).toHaveBeenLastCalledWith(tabId, ['C:/textures/tex_0.png'], buildAtlasRequest({ ...atlasDefaults(), baseName: 'ui' })),
    )
  })

  it('picking a page of a multi-page atlas targets that atlas; an invalid name is refused', async () => {
    vi.mocked(atlasLoadProject).mockImplementation((path) =>
      Promise.resolve(path === 'D:/proj/hud.texatlas.json' ? projectSummary(path, { pages: [{ width: 64, height: 64 }, { width: 64, height: 64 }] }) : null),
    )
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    vi.mocked(open).mockResolvedValueOnce('D:/proj/hud_1.png')
    renderTab(tabId)
    fireEvent.click(screen.getByRole('button', { name: 'Choose target atlas' }))
    await waitFor(() => expect(params()).toMatchObject({ outputDir: 'D:/proj', baseName: 'hud' }))

    // A plain image (no project) becomes the target as is: a new atlas there.
    vi.mocked(open).mockResolvedValueOnce('D:/other/sheet_2.png')
    fireEvent.click(screen.getByRole('button', { name: 'Choose target atlas' }))
    await waitFor(() => expect(params()).toMatchObject({ outputDir: 'D:/other', baseName: 'sheet_2' }))
    await waitFor(() => expect(screen.getByTestId('atlas-existing')).toHaveTextContent('No atlas here yet; a new one will be created.'))

    vi.mocked(open).mockResolvedValueOnce('D:/proj/aux.png')
    fireEvent.click(screen.getByRole('button', { name: 'Choose target atlas' }))
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith('This name is reserved by Windows'))
    expect(params()).toMatchObject({ outputDir: 'D:/other', baseName: 'sheet_2' })
  })

  it('incremental options are bound to the session', () => {
    renderTab(tabId)
    fireEvent.click(screen.getByRole('radio', { name: 'Repack optimally' }))
    expect(params().incrementalMode).toBe('repackOptimal')
    fireEvent.click(screen.getByRole('switch', { name: 'Remove sprites not in the current list' }))
    expect(params().removeMissing).toBe(true)
    act(() => getSession(tabId)!.getState().undo())
    expect(params().removeMissing).toBe(false)
  })

  it('image only hides the incremental options and explains why', async () => {
    vi.mocked(atlasLoadProject).mockImplementation((path) => Promise.resolve(projectSummary(path)))
    getSession(tabId)!.getState().addFiles(makeFiles(1))
    getSession(tabId)!.getState().setParams({ outputDir: 'D:/out' })
    renderTab(tabId)
    await waitFor(() => expect(screen.getByTestId('atlas-existing')).toHaveTextContent('3 sprites'))
    expect(screen.getByRole('radio', { name: 'Keep positions' })).toBeInTheDocument()

    act(() => getSession(tabId)!.getState().setParams({ exporter: 'imageOnly' }))
    expect(screen.queryByRole('radio', { name: 'Keep positions' })).not.toBeInTheDocument()
    expect(screen.queryByRole('radio', { name: 'Repack optimally' })).not.toBeInTheDocument()
    expect(screen.queryByRole('switch', { name: 'Remove sprites not in the current list' })).not.toBeInTheDocument()
    expect(screen.queryByTestId('atlas-existing')).not.toBeInTheDocument()
    expect(screen.getByTestId('atlas-image-only-note')).toHaveTextContent(
      'Writes only the packed image: atlas.png (atlas_0.png, atlas_1.png… with several pages), without metadata or a project file.',
    )
    expect(screen.getByTestId('atlas-image-only-note')).toHaveTextContent('always packed from scratch')
    // The target still sets where Export… starts.
    expect(screen.getByLabelText('Target atlas')).toHaveValue('D:/out/atlas.png')

    vi.mocked(save).mockResolvedValueOnce('D:/out/sheet.png')
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(atlasExport).toHaveBeenCalled())
    expect(vi.mocked(atlasExport).mock.calls[0][2]).toMatchObject({
      exporter: { kind: 'imageOnly', options: {} },
      outputDir: 'D:/out',
      baseName: 'sheet',
    })
  })
})
