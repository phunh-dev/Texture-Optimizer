import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { toast } from 'sonner'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { importPaths } from '@/lib/import'
import type { ImportedFile } from '@/lib/ipc/types'
import { installDomMocks } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { meshDefaults } from './defaults'
import MeshPackTab from './index'
import { meshPack, meshPreviewPack, meshScan, type ModelInfo } from './ipc'
import { useModelInfos } from './models'
import rustDefaults from './rustDefaults.json'

vi.mock('./ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./ipc')>()),
  meshScan: vi.fn(() => Promise.resolve({ models: [], skipped: [] })),
  meshPreviewPack: vi.fn(() => new Promise(() => undefined)),
  meshPack: vi.fn(() => Promise.resolve('job-9')),
}))

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  thumbnailUrl: vi.fn((file: { path: string }, size: string) => `thumb://localhost/${file.path}?size=${size}`),
}))

vi.mock('sonner', async (importOriginal) => {
  const actual = await importOriginal<typeof import('sonner')>()
  return { ...actual, toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }) }
})

function modelFile(name: string, ext = 'fbx'): ImportedFile {
  return { id: `id-${name}`, path: `C:/models/${name}.${ext}`, name: `${name}.${ext}`, ext, width: 0, height: 0, sizeBytes: 10, mtimeMs: 1 }
}

function modelInfo(file: ImportedFile, overrides: Partial<ModelInfo> = {}): ModelInfo {
  return {
    path: file.path,
    format: file.ext as ModelInfo['format'],
    meshCount: 3,
    vertexCount: 1200,
    embeddedTextureCount: 0,
    warnings: [],
    materials: [
      {
        index: 0,
        name: 'Body',
        meshCount: 2,
        vertexCount: 1000,
        uvChannel: 0,
        uvRange: { min: [0, 0], max: [1, 1], outOfRange: false },
        textures: [
          { channel: 'baseColor', path: 'C:/models/tex/body.png', rawPath: 'tex/body.png', exists: true, embedded: false, uvChannel: 0, width: 512, height: 512, mtimeMs: 5 },
          { channel: 'normal', path: 'C:/models/tex/body_n.png', rawPath: 'tex/body_n.png', exists: false, embedded: false, uvChannel: 0, width: null, height: null, mtimeMs: 0 },
        ],
      },
      {
        index: 1,
        name: 'Floor',
        meshCount: 1,
        vertexCount: 200,
        uvChannel: 0,
        uvRange: { min: [0, 0], max: [4, 4], outOfRange: true },
        textures: [{ channel: 'baseColor', path: 'C:/models/tex/floor.png', rawPath: 'tex/floor.png', exists: true, embedded: false, uvChannel: 0, width: 64, height: 64, mtimeMs: 5 }],
      },
    ],
    ...overrides,
  }
}

function renderTab(tabId: string) {
  return render(
    <TooltipProvider>
      <MeshPackTab tabId={tabId} />
    </TooltipProvider>,
  )
}

function addModels(tabId: string, files: ImportedFile[], infos = files.map((f) => modelInfo(f))) {
  act(() => {
    useModelInfos.getState().put(infos)
    getSession(tabId)!.getState().addFiles(files)
  })
}

describe('MeshPackTab', { timeout: 30_000 }, () => {
  let tabId = ''
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    useModelInfos.setState({ infos: {}, errors: {}, pending: {} })
    vi.mocked(meshScan).mockClear()
    vi.mocked(meshPack).mockClear()
    vi.mocked(meshPreviewPack).mockClear()
    for (const f of [toast, toast.success, toast.error, toast.info, toast.warning]) vi.mocked(f).mockClear()
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('meshPack')
  })
  afterEach(cleanup)

  it('starts from the defaults and shows the model drop zone', () => {
    renderTab(tabId)
    expect(getSession(tabId)!.getState().params).toEqual(meshDefaults())
    expect(screen.getByTestId('mesh-empty')).toHaveTextContent('Drop 3D models or folders here')
    expect(screen.getByTestId('run-button')).toBeDisabled()
  })

  it('renders model cards with format, counts, materials, texture thumbnails and warnings', () => {
    const [a, b] = [modelFile('crate'), modelFile('barrel', 'obj')]
    addModels(tabId, [a, b], [
      modelInfo(a, { warnings: [{ code: 'MESH_TEXTURE_NOT_FOUND', params: { material: 'Body', channel: 'normal', path: 'tex/body_n.png' } }] }),
      modelInfo(b),
    ])
    renderTab(tabId)
    const cards = screen.getAllByTestId('model-card')
    expect(cards).toHaveLength(2)
    const card = within(cards[0])
    expect(card.getByText('crate.fbx')).toBeInTheDocument()
    expect(card.getByTestId('format-badge')).toHaveTextContent('fbx')
    expect(cards[0]).toHaveTextContent('3 meshes · 1200 verts · 2 materials')
    expect(card.getByTestId('model-warnings')).toHaveTextContent('1 warning')
    const rows = card.getAllByTestId('material-row')
    expect(rows[0]).toHaveTextContent('Body')
    expect(rows[0]).toHaveTextContent('In range')
    expect(rows[1]).toHaveTextContent('Skipped') // UVs 0–4 with the default skip policy
    const thumbs = within(rows[0]).getAllByTestId('texture-thumb')
    expect(thumbs.map((t) => t.dataset.channel)).toEqual(['baseColor', 'normal'])
    expect(thumbs[0].querySelector('img')!.getAttribute('src')).toContain('size=small')
    expect(thumbs[1].dataset.missing).toBe('true')
    expect(screen.getByTestId('file-count')).toHaveTextContent('2 models')
    expect(screen.getByTestId('add-model-card')).toBeInTheDocument()

    // Policy change updates the predicted status live.
    act(() => getSession(tabId)!.getState().setParams({ outOfRange: 'bakeRepeat', maxTiles: 4 }))
    expect(within(screen.getAllByTestId('material-row')[1]).getByText('Repeated 4×4')).toBeInTheDocument()
  })

  it('× removes a model with an Undo toast; undo brings it back', () => {
    const files = [modelFile('crate'), modelFile('barrel')]
    addModels(tabId, files)
    renderTab(tabId)
    fireEvent.click(screen.getByRole('button', { name: 'Remove crate.fbx' }))
    expect(getSession(tabId)!.getState().files.map((f) => f.name)).toEqual(['barrel.fbx'])
    expect(screen.getAllByTestId('model-card')).toHaveLength(1)
    expect(toast).toHaveBeenCalledWith('Removed 1 model', expect.objectContaining({ action: expect.objectContaining({ label: 'Undo' }) }))
    const undo = vi.mocked(toast).mock.calls[0][1] as unknown as { action: { onClick: () => void } }
    act(() => undo.action.onClick())
    expect(getSession(tabId)!.getState().files.map((f) => f.name)).toEqual(['crate.fbx', 'barrel.fbx'])
    expect(screen.getAllByTestId('model-card')).toHaveLength(2)
    // Model details survive the round trip (kept outside the history).
    expect(screen.getAllByTestId('model-card')[0]).toHaveTextContent('Body')
  })

  it('imports through the registered model importer (drag & drop / pickers)', async () => {
    const f = modelFile('crate')
    vi.mocked(meshScan).mockResolvedValueOnce({ models: [{ file: f, info: modelInfo(f) }], skipped: [{ path: 'C:/x.blend', error: { code: 'MESH_FORMAT_UNSUPPORTED', params: { path: 'C:/x.blend' } } }] })
    renderTab(tabId)
    await act(async () => {
      await importPaths(tabId, ['C:/models'], { recursive: true })
    })
    expect(meshScan).toHaveBeenCalledWith(['C:/models'], true)
    expect(getSession(tabId)!.getState().files).toEqual([f])
    expect(toast.warning).toHaveBeenCalledWith('Added 1, skipped 1', expect.anything())
    expect(screen.getAllByTestId('model-card')).toHaveLength(1)
  })

  it('re-scans models whose details are unknown (restored session)', async () => {
    const f = modelFile('crate')
    vi.mocked(meshScan).mockResolvedValueOnce({ models: [{ file: f, info: modelInfo(f) }], skipped: [] })
    act(() => {
      getSession(tabId)!.getState().addFiles([f])
    })
    renderTab(tabId)
    await waitFor(() => expect(screen.getAllByTestId('material-row')).toHaveLength(2))
    expect(meshScan).toHaveBeenCalledWith([f.path], false)
  })

  it('shows every option; conditional ones follow their switches', () => {
    renderTab(tabId)
    for (const g of ['Channels', 'Atlas', 'UV remapping', 'Missing-channel fill', 'Output']) {
      expect(screen.getByRole('button', { name: g })).toBeInTheDocument()
    }
    // Channels multi-select.
    const normal = screen.getByRole('button', { name: 'Normal' })
    expect(normal).toHaveAttribute('data-state', 'on')
    fireEvent.click(normal)
    expect(getSession(tabId)!.getState().params.channels).toEqual(['baseColor', 'metallic', 'roughness', 'occlusion', 'emissive', 'opacity', 'specular', 'height'])
    fireEvent.click(screen.getByRole('button', { name: 'Normal' }))
    expect((getSession(tabId)!.getState().params.channels as string[])[1]).toBe('normal')

    // Max tiles only for bake repeat; custom inset only for "Custom".
    expect(screen.queryByText('Max repeats per axis')).not.toBeInTheDocument()
    act(() => getSession(tabId)!.getState().setParams({ outOfRange: 'bakeRepeat' }))
    expect(screen.getByText('Max repeats per axis')).toBeInTheDocument()
    expect(screen.queryByText('Inset (pixels)')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('radio', { name: 'Custom' }))
    expect(screen.getByText('Inset (pixels)')).toBeInTheDocument()

    // Format, verify and fallback only when rewriting models.
    expect(screen.getByRole('combobox', { name: 'Model format' })).toHaveTextContent('Same as source')
    expect(screen.getByRole('switch', { name: 'Verify geometry' })).toBeEnabled()
    fireEvent.click(screen.getByRole('radio', { name: 'UV remap data' }))
    expect(screen.queryByRole('combobox', { name: 'Model format' })).not.toBeInTheDocument()
    expect(screen.queryByRole('switch', { name: 'Verify geometry' })).not.toBeInTheDocument()
    expect(screen.queryByRole('switch', { name: 'Fall back to UV remap data' })).not.toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Copy source models' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('radio', { name: 'Rewrite models' }))

    // FBX output: the geometry check cannot be switched off unless allowed (Advanced).
    act(() => getSession(tabId)!.getState().setParams({ format: 'fbx' }))
    expect(screen.getByRole('switch', { name: 'Verify geometry' })).toBeDisabled()
    act(() => getSession(tabId)!.getState().setParams({ allowUnverifiedFbx: true }))
    expect(screen.getByRole('switch', { name: 'Verify geometry' })).toBeEnabled()

    // Merged material name only while merging.
    expect(screen.getByText('Merged material name')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('switch', { name: 'Merge materials' }))
    expect(screen.queryByText('Merged material name')).not.toBeInTheDocument()
  })

  it('Run needs an output folder and a valid base name, then starts mesh_pack with the exact request', async () => {
    const files = [modelFile('crate'), modelFile('barrel', 'dae')]
    addModels(tabId, files)
    renderTab(tabId)
    const run = screen.getByTestId('run-button')
    expect(run).toHaveTextContent('Pack 2 models')
    expect(run).toBeDisabled()
    expect(screen.getByText('Choose an output folder first')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Output folder'), { target: { value: 'C:/out' } })
    fireEvent.change(screen.getByLabelText('Base name'), { target: { value: 'a/b' } })
    expect(screen.getByRole('alert')).toHaveTextContent('Use a plain file name')
    expect(run).toBeDisabled()
    fireEvent.change(screen.getByLabelText('Base name'), { target: { value: 'props' } })
    expect(screen.getByText(/props_baseColor\.png, props_normal\.png, props\.report\.json/)).toBeInTheDocument()
    expect(run).toBeEnabled()

    fireEvent.click(run)
    await waitFor(() => expect(meshPack).toHaveBeenCalled())
    expect(meshPack).toHaveBeenCalledWith(tabId, ['C:/models/crate.fbx', 'C:/models/barrel.dae'], rustDefaults, 'C:/out', 'props')
    expect(await screen.findByTestId('job-progress')).toBeInTheDocument()

    act(() =>
      useJobs.getState().handleFinished({
        jobId: 'job-9',
        tabId,
        cancelled: false,
        results: [
          { input: files[0].path, output: 'C:/out/crate.fbx', error: null, meta: { kind: 'model', outcome: 'rewritten', sidecar: null, files: ['C:/out/crate.fbx'], warnings: [] } },
          {
            input: files[1].path,
            output: 'C:/out/barrel.uvremap.json',
            error: null,
            meta: { kind: 'model', outcome: 'fallback', sidecar: 'C:/out/barrel.uvremap.json', files: [], warnings: [{ code: 'MESH_EXPORT_GEOMETRY_CHANGED', params: { format: 'fbx' } }] },
          },
          { input: 'C:/out', output: 'C:/out/props.report.json', error: null, meta: { kind: 'summary', rewritten: 1, fallback: 1, skipped: 0, failed: 0, files: ['a', 'b', 'c'], warnings: [] } },
        ],
      }),
    )
    expect(toast.success).toHaveBeenCalledWith('Packed: 1 rewritten, 1 fallback, 0 skipped', expect.anything())
    const last = screen.getByTestId('mesh-last-run')
    expect(last).toHaveTextContent('Rewritten: 1')
    expect(last).toHaveTextContent('Fallback: 1')
    expect(last).toHaveTextContent('3 files written')
  })

  it('a failed run shows the translated error', async () => {
    addModels(tabId, [modelFile('crate')])
    act(() => getSession(tabId)!.getState().setParams({ outputDir: 'C:/out' }))
    renderTab(tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(meshPack).toHaveBeenCalled())
    act(() =>
      useJobs.getState().handleFinished({
        jobId: 'job-9',
        tabId,
        cancelled: false,
        results: [{ input: 'C:/out', output: null, error: { code: 'MESH_WORKER_CRASHED', params: { exitCode: 3 } }, meta: { kind: 'summary' } }],
      }),
    )
    expect(toast.error).toHaveBeenCalledWith('Packing failed', {
      description: 'The 3D worker process stopped unexpectedly (exit code 3). The model may be corrupt or unsupported',
    })
  })

  it('switches to Vietnamese at runtime', async () => {
    addModels(tabId, [modelFile('crate')])
    renderTab(tabId)
    await act(async () => {
      await i18n.changeLanguage('vi')
    })
    expect(screen.getByTestId('file-count')).toHaveTextContent('1 model')
    expect(screen.getByRole('button', { name: 'Kênh texture' })).toBeInTheDocument()
    expect(screen.getAllByTestId('material-row')[0]).toHaveTextContent('Trong 0–1')
  })
})
