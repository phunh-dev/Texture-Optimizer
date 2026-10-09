import { act, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm'
import i18n from '@/i18n'
import { previewOp, runOp } from '@/lib/ipc'
import { installDomMocks } from '@/lib/testing/dom'
import { getSession } from '@/stores/session'
import { getTool } from '@/tabs/registry'

import { fieldOf, openTool, previewResult, renderTab, resetTabStores, settlePreview, sizedFile, stubObjectUrls, visibleKeys } from '../_imageOps/testing'
import ResolutionTab from './index'
import { buildResolutionRequest, effectiveRound, predictResolution, resolutionPlacement } from './request'
import { resolutionSchema, type ResolutionParams } from './schema'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  previewOp: vi.fn(),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  originalImageUrl: vi.fn((f: { path: string }) => `orig://${f.path}`),
}))

/** `impl Default for ResolutionParams` (resolution.rs); padColor Color([0,0,0,0]) as a hex string. */
const DEFAULTS = {
  target: 'multipleOf4',
  n: 8,
  round: 'nearest',
  method: 'resample',
  anchor: 'center',
  filter: 'lanczos3',
  keepAspect: false,
  allowNonSquare: true,
  maxSize: 8192,
  padColor: '#00000000',
}
const RUST_DEFAULT_PARAMS = { ...DEFAULTS, padColor: [0, 0, 0, 0] }

const p = (patch: Partial<ResolutionParams> = {}): ResolutionParams => ({ ...(DEFAULTS as ResolutionParams), ...patch })
const widthFor = (n: number, patch: Partial<ResolutionParams>) => {
  const r = predictResolution(n, 1, p(patch))
  if (!r.ok) throw new Error(r.error.code)
  return r.width
}
const size = (w: number, h: number, patch: Partial<ResolutionParams>) => {
  const r = predictResolution(w, h, p(patch))
  return r.ok ? [r.width, r.height] : r.error
}

describe('resolution schema and request', () => {
  it('defaults equal the Rust Default impl', () => {
    expect(schemaDefaults(resolutionSchema)).toEqual(DEFAULTS)
    expect(getTool('resolution').defaultParams()).toEqual(DEFAULTS)
    expect(buildResolutionRequest(DEFAULTS)).toEqual({ kind: 'resolution', params: RUST_DEFAULT_PARAMS })
  })

  it('builds the exact OpRequest JSON with the pad color as [r, g, b, a]', () => {
    expect(
      buildResolutionRequest({
        target: 'multipleOfN',
        n: 10,
        round: 'down',
        method: 'pad',
        anchor: 'bottomRight',
        filter: 'catmullRom',
        keepAspect: true,
        allowNonSquare: false,
        maxSize: 0,
        padColor: '#01020304',
      }),
    ).toEqual({
      kind: 'resolution',
      params: {
        target: 'multipleOfN',
        n: 10,
        round: 'down',
        method: 'pad',
        anchor: 'bottomRight',
        filter: 'catmullRom',
        keepAspect: true,
        allowNonSquare: false,
        maxSize: 0,
        padColor: [1, 2, 3, 4],
      },
    })
    expect(buildResolutionRequest({ ...DEFAULTS, padColor: '#ff8000' })!.params.padColor).toEqual([255, 128, 0, 255])
    expect(buildResolutionRequest({ ...DEFAULTS, n: 0 })).toBeNull()
    expect(buildResolutionRequest({ ...DEFAULTS, padColor: 'red' })).toBeNull()
  })
})

describe('predicted sizes mirror resolution::target_size', () => {
  it('multiple of 4 table', () => {
    const table: [number, number, number, number][] = [
      [1, 4, 4, 4],
      [3, 4, 4, 4],
      [4, 4, 4, 4],
      [6, 8, 8, 4],
      [1023, 1024, 1024, 1020],
      [1025, 1024, 1028, 1024],
      [4097, 4096, 4100, 4096],
    ]
    for (const [n, near, up, down] of table) {
      expect(widthFor(n, { round: 'nearest' }), `${n} nearest`).toBe(near)
      expect(widthFor(n, { round: 'up' }), `${n} up`).toBe(up)
      expect(widthFor(n, { round: 'down' }), `${n} down`).toBe(down)
    }
  })

  it('multiple of N table', () => {
    const table: [number, number, number, number][] = [
      [1, 10, 10, 10],
      [3, 10, 10, 10],
      [15, 20, 20, 10],
      [1023, 1020, 1030, 1020],
      [1025, 1030, 1030, 1020],
      [4097, 4100, 4100, 4090],
    ]
    for (const [n, near, up, down] of table) {
      for (const [round, e] of [
        ['nearest', near],
        ['up', up],
        ['down', down],
      ] as const) {
        expect(widthFor(n, { target: 'multipleOfN', n: 10, round }), `${n} ${round}`).toBe(e)
      }
    }
  })

  it('POT table (nearest / up / down)', () => {
    const table: [number, number, number, number][] = [
      [1, 1, 1, 1],
      [3, 4, 4, 2],
      [4, 4, 4, 4],
      [1023, 1024, 1024, 512],
      [1025, 1024, 2048, 1024],
      [4097, 4096, 8192, 4096],
    ]
    for (const [n, near, up, down] of table) {
      expect(widthFor(n, { target: 'pot', round: 'nearest' }), `${n} nearest`).toBe(near)
      expect(widthFor(n, { target: 'pot', round: 'up' }), `${n} up`).toBe(up)
      expect(widthFor(n, { target: 'pot', round: 'down' }), `${n} down`).toBe(down)
    }
  })

  it('pad always rounds up and crop always rounds down', () => {
    expect(size(1025, 3, { target: 'pot', method: 'pad', round: 'down' })).toEqual([2048, 4])
    expect(size(1025, 3, { target: 'pot', method: 'crop', round: 'up' })).toEqual([1024, 2])
    expect(effectiveRound({ method: 'pad', round: 'down' })).toBe('up')
    expect(effectiveRound({ method: 'crop', round: 'up' })).toBe('down')
    expect(effectiveRound({ method: 'resample', round: 'down' })).toBe('down')
  })

  it('max size clamps; 0 = unlimited; impossible cap is an error', () => {
    expect(size(5000, 3, { target: 'pot', round: 'up', maxSize: 4096 })).toEqual([4096, 4])
    expect(size(1500, 1001, { round: 'up', maxSize: 1002 })).toEqual([1000, 1000])
    expect(size(9000, 3, { target: 'pot', round: 'up', maxSize: 0 })).toEqual([16384, 4])
    expect(size(100, 10, { target: 'pot', maxSize: 64 })).toEqual([64, 8])
    expect(size(100, 10, { target: 'multipleOfN', n: 10, maxSize: 5 })).toEqual({
      code: 'INVALID_PARAMS',
      params: { param: 'maxSize', reason: 'smaller than the smallest valid size' },
    })
  })

  it('square vs non-square POT', () => {
    expect(size(100, 30, { target: 'pot' })).toEqual([128, 32])
    expect(size(100, 30, { target: 'pot', allowNonSquare: false })).toEqual([128, 128])
    expect(size(64, 64, { target: 'pot', allowNonSquare: false })).toEqual([64, 64])
    expect(size(100, 30, { target: 'pot', allowNonSquare: false, maxSize: 64 })).toEqual([64, 64])
    // allowNonSquare has no effect on multiples.
    expect(size(100, 30, { allowNonSquare: false })).toEqual([100, 32])
  })

  it('invalid n', () => {
    expect(size(4, 4, { target: 'multipleOfN', n: 0 })).toEqual({ code: 'INVALID_PARAMS', params: { param: 'n', reason: 'must be >= 1' } })
  })
})

describe('preview placement', () => {
  const req = (params: Record<string, unknown>) => ({ kind: 'resolution' as const, params })
  it('pad / crop place the original 1:1 by anchor', () => {
    expect(resolutionPlacement(5, 6, 8, 8, req({ method: 'pad', anchor: 'center' }))).toEqual({ x: -1, y: -1, width: 8, height: 8 })
    expect(resolutionPlacement(5, 6, 8, 8, req({ method: 'pad', anchor: 'bottomRight' }))).toEqual({ x: -3, y: -2, width: 8, height: 8 })
    expect(resolutionPlacement(10, 11, 8, 8, req({ method: 'crop', anchor: 'center' }))).toEqual({ x: 1, y: 1, width: 8, height: 8 })
    expect(resolutionPlacement(10, 11, 8, 8, req({ method: 'crop', anchor: 'topLeft' }))).toEqual({ x: 0, y: 0, width: 8, height: 8 })
  })

  it('resample stretches, or letterboxes with keepAspect', () => {
    expect(resolutionPlacement(1023, 3, 1024, 4, req({ method: 'resample' }))).toEqual({ x: 0, y: 0, width: 1023, height: 3 })
    // 4x2 → 6x6 keeping aspect: content 6x3 at (0, 1) for center → original box offset (0, -2/3).
    const r = resolutionPlacement(4, 2, 6, 6, req({ method: 'resample', keepAspect: true, anchor: 'center' }))
    expect(r.x).toBeCloseTo(0)
    expect(r.y).toBeCloseTo(-2 / 3)
    expect(r.width).toBeCloseTo(4)
    expect(r.height).toBeCloseTo(4)
  })
})

describe('ResolutionTab', () => {
  let tabId = ''
  const session = () => getSession(tabId)!.getState()
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetTabStores()
    vi.mocked(previewOp).mockReset()
    vi.mocked(runOp).mockClear()
    stubObjectUrls()
    await i18n.changeLanguage('en')
    tabId = openTool('resolution').tabId
  })
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('shows conditional fields per target and method', () => {
    renderTab(ResolutionTab, tabId)
    expect(visibleKeys(tabId)).toEqual(['target', 'maxSize', 'method', 'round', 'filter', 'keepAspect'])
    act(() => session().setParams({ target: 'multipleOfN' }))
    expect(visibleKeys(tabId)).toContain('n')
    expect(visibleKeys(tabId)).not.toContain('allowNonSquare')
    act(() => session().setParams({ target: 'pot' }))
    expect(visibleKeys(tabId)).toContain('allowNonSquare')
    expect(visibleKeys(tabId)).not.toContain('n')
    act(() => session().setParams({ keepAspect: true }))
    expect(visibleKeys(tabId)).toEqual(['target', 'allowNonSquare', 'maxSize', 'method', 'round', 'filter', 'keepAspect', 'anchor', 'padColor'])
    act(() => session().setParams({ method: 'pad' }))
    expect(visibleKeys(tabId)).toEqual(['target', 'allowNonSquare', 'maxSize', 'method', 'round', 'anchor', 'padColor'])
    act(() => session().setParams({ method: 'crop' }))
    expect(visibleKeys(tabId)).toEqual(['target', 'allowNonSquare', 'maxSize', 'method', 'round', 'anchor'])
  })

  it('disables rounding for pad and crop', () => {
    renderTab(ResolutionTab, tabId)
    const roundButtons = () => within(fieldOf(tabId, 'round')!).getAllByRole('radio')
    expect(roundButtons()).toHaveLength(3)
    roundButtons().forEach((b) => expect(b).toBeEnabled())
    fireEvent.click(screen.getByRole('radio', { name: 'Pad' }))
    expect(session().params.method).toBe('pad')
    roundButtons().forEach((b) => expect(b).toBeDisabled())
    fireEvent.click(screen.getByRole('radio', { name: 'Crop' }))
    roundButtons().forEach((b) => expect(b).toBeDisabled())
    fireEvent.click(screen.getByRole('radio', { name: 'Resample' }))
    roundButtons().forEach((b) => expect(b).toBeEnabled())
  })

  it('lists predicted sizes (1023 / 1025 / 3) and marks valid files unchanged', () => {
    session().addFiles([sizedFile(1023, 1025), sizedFile(3, 3), sizedFile(64, 32)])
    session().setParams({ target: 'pot' })
    renderTab(ResolutionTab, tabId)
    const out = (w: number, h: number) => within(screen.getByTestId(`size-row-id-tex_${w}x${h}.png`)).getByTestId('size-out').textContent
    expect(out(1023, 1025)).toBe('1024×1024')
    expect(out(3, 3)).toBe('4×4')
    expect(screen.getByTestId('size-row-id-tex_64x32.png')).toHaveAttribute('data-state', 'unchanged')
    act(() => session().setParams({ method: 'crop' }))
    expect(out(1023, 1025)).toBe('512×1024')
    expect(out(3, 3)).toBe('2×2')
  })

  it('previews the focused file with pad placement and shows translated errors', async () => {
    vi.useFakeTimers()
    vi.mocked(previewOp).mockResolvedValueOnce(previewResult(8, 8))
    const file = sizedFile(5, 6)
    session().addFiles([sizedFile(100, 100), file])
    session().select(file.id)
    session().setParams({ target: 'pot', method: 'pad', anchor: 'bottomRight', padColor: '#ff000080' })
    session().setUiFlag('preview', true)
    renderTab(ResolutionTab, tabId)
    await settlePreview()
    expect(previewOp).toHaveBeenCalledTimes(1)
    expect(previewOp).toHaveBeenCalledWith(tabId, file.path, {
      kind: 'resolution',
      params: { ...RUST_DEFAULT_PARAMS, target: 'pot', method: 'pad', anchor: 'bottomRight', padColor: [255, 0, 0, 128] },
    })
    const after = screen.getByTestId('compare-view').querySelectorAll('img')[1]
    expect(after.style.transform).toMatch(/scale\([^)]*\) translate\(-3px, -2px\)$/)
    expect(after.style.width).toBe('8px')

    vi.mocked(previewOp).mockRejectedValueOnce({ code: 'INVALID_PARAMS', params: { param: 'maxSize', reason: 'too small' } })
    act(() => session().setParams({ target: 'multipleOfN', n: 10, maxSize: 5 }))
    await settlePreview()
    expect(screen.getByTestId('preview-error-message')).toHaveTextContent('Invalid parameter "maxSize": too small')
  })

  it('Run sends the resolution request', async () => {
    const files = [sizedFile(1023, 1025)]
    session().addFiles(files)
    session().setParams({ method: 'crop', anchor: 'top' })
    renderTab(ResolutionTab, tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    expect(runOp).toHaveBeenCalledWith(
      tabId,
      { kind: 'resolution', params: { ...RUST_DEFAULT_PARAMS, method: 'crop', anchor: 'top' } },
      [files[0].path],
      session().output,
    )
  })
})
