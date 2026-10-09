import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm'
import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { previewOp, runOp } from '@/lib/ipc'
import type { OpRequest, PreviewResult } from '@/lib/ipc/types'
import { installDomMocks, makeFiles } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'
import { getTool } from '@/tabs/registry'

import BgRemoveTab from '.'
import { PICK_FLAG } from './fields'
import { ORIGINAL_REQUEST, PREVIEW_DEBOUNCE_MS } from './Preview'
import { bgRemoveSchema, buildBgRemoveRequest, defaultBgRemoveParams } from './schema'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  previewOp: vi.fn(),
  thumbnailUrl: vi.fn(() => 'thumb://localhost/x'),
}))

/** Literal copy of `impl Default for BgRemoveParams` (crates/texopt-core/src/ops/bg_remove/mod.rs). */
const RUST_DEFAULTS = {
  mode: 'auto',
  color: [255, 255, 255, 255],
  checkerCellSize: null,
  fill: 'floodFromEdges',
  tolerance: 10,
  metric: 'rgb',
  feather: 0,
  defringe: false,
  defringeStrength: 100,
}

const CHECKER_META = { removedPixels: 512, detectedMode: 'checker', checkerCellSize: 16, bgColors: [[204, 204, 204, 255], [255, 255, 255, 255]] }

function result(meta: unknown): PreviewResult {
  return { png: new ArrayBuffer(8), width: 64, height: 32, meta }
}

let tabId = ''
const session = () => getSession(tabId)!.getState()
const bgCalls = () => vi.mocked(previewOp).mock.calls.filter(([, , req]) => req.kind === 'bgRemove')

function renderTab() {
  return render(
    <TooltipProvider>
      <BgRemoveTab tabId={tabId} />
    </TooltipProvider>,
  )
}

describe('Background Remover tab', () => {
  beforeAll(() => {
    installDomMocks()
    let n = 0
    URL.createObjectURL = vi.fn(() => `blob:url-${++n}`)
    URL.revokeObjectURL = vi.fn()
  })
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    vi.mocked(runOp).mockClear()
    vi.mocked(previewOp).mockReset()
    vi.mocked(previewOp).mockImplementation((_tab: string, _path: string, req: OpRequest) =>
      Promise.resolve(req.kind === 'bgRemove' ? result(CHECKER_META) : result(null)),
    )
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('bgRemove')
  })
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('schema defaults, registry defaults and session params equal the Rust defaults', () => {
    expect(schemaDefaults(bgRemoveSchema)).toEqual(RUST_DEFAULTS)
    expect(defaultBgRemoveParams()).toEqual(RUST_DEFAULTS)
    expect(getTool('bgRemove').defaultParams()).toEqual(RUST_DEFAULTS)
    expect(session().params).toEqual(RUST_DEFAULTS)
  })

  it('builds the exact request JSON (Rust field order, UI-only keys dropped)', () => {
    const req = buildBgRemoveRequest({ ...RUST_DEFAULTS, mode: 'color', color: [1, 2, 3, 255], checkerCellSize: 8, junk: true })
    expect(JSON.stringify(req)).toBe(
      '{"kind":"bgRemove","params":{"mode":"color","color":[1,2,3,255],"checkerCellSize":8,"fill":"floodFromEdges","tolerance":10,"metric":"rgb","feather":0,"defringe":false,"defringeStrength":100}}',
    )
  })

  it('Run sends the bgRemove request through runOp', async () => {
    session().addFiles(makeFiles(2))
    session().setParams({ tolerance: 25, metric: 'lab' })
    renderTab()
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    const [tab, request, paths] = vi.mocked(runOp).mock.calls[0]
    expect(tab).toBe(tabId)
    expect(request).toEqual({ kind: 'bgRemove', params: { ...RUST_DEFAULTS, tolerance: 25, metric: 'lab' } })
    expect(paths).toEqual(['C:/textures/tex_0.png', 'C:/textures/tex_1.png'])
  })

  it('shows conditional fields: color for mode=color, cell size for checker, strength when defringing', () => {
    renderTab()
    expect(screen.queryByRole('button', { name: 'Pick color from image' })).not.toBeInTheDocument()
    expect(screen.queryByRole('switch', { name: 'Auto-detect' })).not.toBeInTheDocument()
    expect(screen.queryByRole('slider', { name: 'Defringe strength' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: 'Color' }))
    expect(session().params.mode).toBe('color')
    expect(screen.getByRole('button', { name: 'Pick color from image' })).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: 'Background color (hex)' })).toHaveValue('#ffffff')
    fireEvent.change(screen.getByRole('textbox', { name: 'Background color (hex)' }), { target: { value: '#10ff20' } })
    expect(session().params.color).toEqual([16, 255, 32, 255])

    fireEvent.click(screen.getByRole('radio', { name: 'Checker' }))
    expect(screen.queryByRole('button', { name: 'Pick color from image' })).not.toBeInTheDocument()
    const auto = screen.getByRole('switch', { name: 'Auto-detect' })
    expect(auto).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('spinbutton', { name: 'Checker cell size' })).toBeDisabled()
    fireEvent.click(auto)
    expect(session().params.checkerCellSize).toBe(16)
    expect(screen.getByRole('spinbutton', { name: 'Checker cell size' })).toBeEnabled()
    fireEvent.click(screen.getByRole('switch', { name: 'Auto-detect' }))
    expect(session().params.checkerCellSize).toBeNull()

    fireEvent.click(screen.getByRole('switch', { name: 'Defringe' }))
    expect(screen.getByRole('slider', { name: 'Defringe strength' })).toBeInTheDocument()
  })

  it('quick presets apply a full parameter set as one undo step', () => {
    renderTab()
    fireEvent.click(screen.getByRole('button', { name: 'White bg' }))
    expect(session().params).toEqual({ ...RUST_DEFAULTS, mode: 'white', tolerance: 8, defringe: true })
    expect(session().lastAction).toEqual({ key: 'applyPreset' })
    fireEvent.click(screen.getByRole('button', { name: 'Checker bg' }))
    expect(session().params).toEqual({ ...RUST_DEFAULTS, mode: 'checker', tolerance: 6 })
    fireEvent.click(screen.getByRole('button', { name: 'Auto' }))
    expect(session().params).toEqual(RUST_DEFAULTS)
  })

  it('debounces the live preview and shows the detection info from meta', async () => {
    vi.useFakeTimers()
    session().addFiles(makeFiles(1))
    session().setUiFlag('preview', true)
    renderTab()
    await act(async () => {})
    // The full-res original is only fetched over IPC while the eyedropper is armed.
    expect(previewOp).not.toHaveBeenCalledWith(tabId, 'C:/textures/tex_0.png', ORIGINAL_REQUEST)

    act(() => session().setParams({ tolerance: 20 }))
    act(() => session().setParams({ tolerance: 30 }))
    act(() => session().setParams({ tolerance: 40 }))
    expect(bgCalls()).toHaveLength(0)
    await act(async () => {
      vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS - 10)
    })
    expect(bgCalls()).toHaveLength(0)
    await act(async () => {
      vi.advanceTimersByTime(20)
    })
    expect(bgCalls()).toHaveLength(1)
    expect(bgCalls()[0][2]).toEqual({ kind: 'bgRemove', params: { ...RUST_DEFAULTS, tolerance: 40 } })

    await act(async () => {}) // preview promise resolves
    const meta = screen.getByTestId('bgremove-meta')
    expect(meta).toHaveTextContent('Detected: Checkerboard')
    expect(meta).toHaveTextContent('Cell 16 px')
    // 512 of 64×32 pixels.
    expect(meta).toHaveTextContent('25.0% removed')
    expect(within(meta).getByRole('img', { name: 'Background color #CCCCCC' })).toBeInTheDocument()
    expect(within(meta).getByRole('img', { name: 'Background color #FFFFFF' })).toBeInTheDocument()
  })

  it('shows translated preview errors', async () => {
    vi.mocked(previewOp).mockImplementation((_t: string, _p: string, req: OpRequest) =>
      req.kind === 'bgRemove' ? Promise.reject({ code: 'BG_CHECKER_NOT_DETECTED', params: {} }) : Promise.resolve(result(null)),
    )
    session().addFiles(makeFiles(1))
    session().setParams({ mode: 'checker' })
    session().setUiFlag('preview', true)
    renderTab()
    expect(await screen.findByRole('alert')).toHaveTextContent('No checkerboard background was detected on the image border')
  })

  it('eyedropper: clicking the before image samples the full-res original and sets color', async () => {
    const getImageData = vi.fn(() => ({ data: new Uint8ClampedArray([12, 34, 56, 200]) }))
    const drawImage = vi.fn()
    const getContext = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(
      () => ({ drawImage, getImageData }) as unknown as CanvasRenderingContext2D,
    )
    const bitmap = { width: 400, height: 300, close: vi.fn() }
    globalThis.createImageBitmap = vi.fn(() => Promise.resolve(bitmap)) as unknown as typeof createImageBitmap

    session().addFiles(makeFiles(1))
    session().setParams({ mode: 'color' })
    renderTab()
    // Arming the eyedropper switches to the preview.
    fireEvent.click(screen.getByRole('button', { name: 'Pick color from image' }))
    expect(session().uiFlags[PICK_FLAG]).toBe(true)
    expect(session().uiFlags.preview).toBe(true)
    expect(screen.getByRole('button', { name: 'Pick color from image' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByText(/Click the “Before” image/)).toBeInTheDocument()

    const view = await screen.findByRole('img', { name: 'Before / after comparison' })
    await waitFor(() => expect(view).toHaveAttribute('data-pick-mode', 'true'))
    // Zoom in once (125% around the 1000×800 center) so the click is mapped through zoom + pan.
    fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }))
    fireEvent.pointerDown(view, { button: 0, clientX: 100, clientY: 50, pointerId: 1 })

    await waitFor(() => expect(session().params.color).toEqual([12, 34, 56, 255]))
    expect(previewOp).toHaveBeenCalledWith(tabId, 'C:/textures/tex_0.png', ORIGINAL_REQUEST)
    expect(createImageBitmap).toHaveBeenCalledWith(expect.any(Blob))
    expect(drawImage).toHaveBeenCalledWith(bitmap, 0, 0)
    // (100 + 125) / 1.25 = 180, (50 + 100) / 1.25 = 120
    expect(getImageData).toHaveBeenCalledWith(180, 120, 1, 1)
    expect(session().uiFlags[PICK_FLAG]).toBe(false)
    getContext.mockRestore()
  })
})
