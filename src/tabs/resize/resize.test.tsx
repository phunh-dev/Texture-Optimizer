import { act, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm'
import i18n from '@/i18n'
import { originalImageUrl, previewOp, runOp } from '@/lib/ipc'
import { installDomMocks } from '@/lib/testing/dom'
import { getSession } from '@/stores/session'
import { getTool } from '@/tabs/registry'

import {
  fieldOf,
  openTool,
  previewResult,
  renderTab,
  resetTabStores,
  settlePreview,
  sizedFile,
  stubObjectUrls,
  visibleKeys,
} from '../_imageOps/testing'
import ResizeTab from './index'
import { buildResizeRequest, predictResize } from './request'
import { resizeSchema, type ResizeParams } from './schema'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  previewOp: vi.fn(),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  originalImageUrl: vi.fn((f: { path: string }) => `orig://${f.path}`),
}))

/** Copied from `impl Default for ResizeParams` (crates/texopt-core/src/ops/resize.rs). */
const RUST_DEFAULTS = {
  mode: 'percent',
  percent: 50,
  width: 1024,
  height: 1024,
  longestSide: 1024,
  keepAspect: true,
  filter: 'lanczos3',
  linearSpace: false,
  premultiplyAlpha: true,
  snap: 'none',
}

const params = (patch: Partial<ResizeParams> = {}): ResizeParams => ({ ...(RUST_DEFAULTS as ResizeParams), ...patch })
const size = (w: number, h: number, patch: Partial<ResizeParams>) => {
  const r = predictResize(w, h, params(patch))
  return r.ok ? [r.width, r.height] : r.error
}

describe('resize schema and request', () => {
  it('defaults equal the Rust Default impl and feed the registry', () => {
    expect(schemaDefaults(resizeSchema)).toEqual(RUST_DEFAULTS)
    expect(getTool('resize').defaultParams()).toEqual(RUST_DEFAULTS)
  })

  it('builds the exact OpRequest JSON', () => {
    expect(buildResizeRequest(RUST_DEFAULTS)).toEqual({ kind: 'resize', params: RUST_DEFAULTS })
    expect(
      buildResizeRequest({
        ...RUST_DEFAULTS,
        mode: 'fitHeight',
        percent: 12.5,
        width: 7,
        height: 9,
        longestSide: 300,
        keepAspect: false,
        filter: 'mitchell',
        linearSpace: true,
        premultiplyAlpha: false,
        snap: 'pot',
        stray: 'ignored',
      }),
    ).toEqual({
      kind: 'resize',
      params: {
        mode: 'fitHeight',
        percent: 12.5,
        width: 7,
        height: 9,
        longestSide: 300,
        keepAspect: false,
        filter: 'mitchell',
        linearSpace: true,
        premultiplyAlpha: false,
        snap: 'pot',
      },
    })
    expect(buildResizeRequest({ ...RUST_DEFAULTS, percent: 0 })).toBeNull()
    expect(buildResizeRequest({ ...RUST_DEFAULTS, width: 0 })).toBeNull()
  })
})

describe('predicted sizes mirror resize::target_size', () => {
  it('percent (tables from ops_resize.rs)', () => {
    expect(size(100, 50, { percent: 50 })).toEqual([50, 25])
    expect(size(100, 50, { percent: 200 })).toEqual([200, 100])
    expect(size(10, 10, { percent: 33.3 })).toEqual([3, 3])
    expect(size(10, 10, { percent: 1 })).toEqual([1, 1])
    expect(size(1, 1, { percent: 10 })).toEqual([1, 1])
    expect(size(8, 8, { percent: 25 })).toEqual([2, 2])
  })

  it('exact with and without keepAspect', () => {
    expect(size(100, 50, { mode: 'exact', width: 30, height: 40, keepAspect: false })).toEqual([30, 40])
    expect(size(100, 50, { mode: 'exact', width: 30, height: 40, keepAspect: true })).toEqual([30, 15])
    expect(size(100, 50, { mode: 'exact', width: 400, height: 100, keepAspect: true })).toEqual([200, 100])
    expect(size(37, 21, { mode: 'exact', width: 37, height: 21, keepAspect: false })).toEqual([37, 21])
  })

  it('fit width / fit height', () => {
    expect(size(100, 50, { mode: 'fitWidth', width: 40 })).toEqual([40, 20])
    expect(size(100, 50, { mode: 'fitWidth', width: 40, keepAspect: false })).toEqual([40, 50])
    expect(size(100, 50, { mode: 'fitHeight', height: 10 })).toEqual([20, 10])
    expect(size(100, 50, { mode: 'fitHeight', height: 10, keepAspect: false })).toEqual([100, 10])
  })

  it('longest side', () => {
    expect(size(100, 50, { mode: 'longestSide', longestSide: 64 })).toEqual([64, 32])
    expect(size(50, 100, { mode: 'longestSide', longestSide: 64 })).toEqual([32, 64])
    expect(size(100, 50, { mode: 'longestSide', longestSide: 64, keepAspect: false })).toEqual([64, 50])
    expect(size(50, 100, { mode: 'longestSide', longestSide: 64, keepAspect: false })).toEqual([50, 64])
  })

  it('snap after resize, incl. 1023 / 1025 / 3', () => {
    expect(size(100, 50, { percent: 50, snap: 'multipleOf4' })).toEqual([52, 24])
    expect(size(100, 50, { percent: 50, snap: 'pot' })).toEqual([64, 32])
    expect(size(100, 50, { percent: 1, snap: 'multipleOf4' })).toEqual([4, 4])
    expect(size(1023, 1025, { percent: 100, snap: 'pot' })).toEqual([1024, 1024])
    expect(size(1023, 1025, { percent: 100, snap: 'multipleOf4' })).toEqual([1024, 1024])
    expect(size(3, 3, { percent: 100, snap: 'multipleOf4' })).toEqual([4, 4])
    expect(size(3, 3, { percent: 100, snap: 'pot' })).toEqual([4, 4])
  })

  it('invalid params and limits', () => {
    expect(size(10, 10, { percent: 0 })).toEqual({ code: 'INVALID_PARAMS', params: { param: 'percent', reason: 'must be > 0' } })
    expect(size(10, 10, { percent: Number.NaN })).toMatchObject({ code: 'INVALID_PARAMS' })
    expect(size(10, 10, { mode: 'exact', width: 0 })).toMatchObject({ code: 'INVALID_PARAMS', params: { param: 'width' } })
    expect(size(10, 10, { mode: 'longestSide', longestSide: 0 })).toMatchObject({ params: { param: 'longestSide' } })
    expect(size(10, 10, { percent: 1_000_000 })).toEqual({ code: 'IMG_TOO_LARGE', params: { max: 32768, width: 100000, height: 100000 } })
  })
})

describe('ResizeTab', () => {
  let tabId = ''
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetTabStores()
    vi.mocked(previewOp).mockReset()
    vi.mocked(runOp).mockClear()
    stubObjectUrls()
    await i18n.changeLanguage('en')
    tabId = openTool('resize').tabId
  })
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  const session = () => getSession(tabId)!.getState()

  it('shows only the fields relevant to the mode', () => {
    renderTab(ResizeTab, tabId)
    expect(visibleKeys(tabId)).toEqual(['mode', 'percent', 'snap', 'filter', 'linearSpace', 'premultiplyAlpha'])
    act(() => session().setParams({ mode: 'exact' }))
    expect(visibleKeys(tabId)).toEqual(['mode', 'width', 'height', 'keepAspect', 'snap', 'filter', 'linearSpace', 'premultiplyAlpha'])
    act(() => session().setParams({ mode: 'fitWidth' }))
    expect(visibleKeys(tabId)).toEqual(['mode', 'width', 'keepAspect', 'snap', 'filter', 'linearSpace', 'premultiplyAlpha'])
    act(() => session().setParams({ mode: 'fitHeight' }))
    expect(visibleKeys(tabId)).toEqual(['mode', 'height', 'keepAspect', 'snap', 'filter', 'linearSpace', 'premultiplyAlpha'])
    act(() => session().setParams({ mode: 'longestSide' }))
    expect(visibleKeys(tabId)).toEqual(['mode', 'longestSide', 'keepAspect', 'snap', 'filter', 'linearSpace', 'premultiplyAlpha'])
    expect(within(fieldOf(tabId, 'longestSide')!).getByText('Longest side')).toBeInTheDocument()
  })

  it('lists predicted output sizes and highlights unchanged files', () => {
    session().addFiles([sizedFile(100, 50), sizedFile(1, 1), sizedFile(1023, 1025)])
    renderTab(ResizeTab, tabId)
    const row = (w: number, h: number) => screen.getByTestId(`size-row-id-tex_${w}x${h}.png`)
    expect(row(100, 50)).toHaveAttribute('data-state', 'changed')
    const out = (w: number, h: number) => within(row(w, h)).getByTestId('size-out').textContent
    expect(out(100, 50)).toBe('50×25')
    expect(row(1, 1)).toHaveAttribute('data-state', 'unchanged')
    expect(within(row(1, 1)).getByLabelText('Unchanged')).toBeInTheDocument()
    expect(screen.getByTestId('size-counts')).toHaveTextContent('2 will change')
    expect(screen.getByTestId('size-counts')).toHaveTextContent('1 unchanged')
    act(() => session().setParams({ mode: 'longestSide', longestSide: 1024 }))
    expect(out(1023, 1025)).toBe('1022×1024')
    // Clicking a row focuses that file (for the preview).
    fireEvent.click(row(1, 1))
    expect(session().selectedIds).toEqual(['id-tex_1x1.png'])
  })

  it('debounces previews of the focused file and shows before / after with dimensions', async () => {
    vi.useFakeTimers()
    vi.mocked(previewOp).mockImplementation((_tab, _path, req) => {
      const p = req.params as { percent: number }
      return Promise.resolve(previewResult(Math.round(512 * (p.percent / 100)), Math.round(256 * (p.percent / 100))))
    })
    const a = sizedFile(512, 256, { name: 'a.png' })
    const b = sizedFile(64, 64, { name: 'b.png' })
    session().addFiles([a, b])
    session().setUiFlag('preview', true)
    renderTab(ResizeTab, tabId)

    // Rapid changes: a single request with the latest params, only after the debounce.
    act(() => session().setParams({ percent: 30 }))
    act(() => session().setParams({ percent: 25 }))
    await settlePreview(249)
    expect(previewOp).not.toHaveBeenCalled()
    expect(screen.getByTestId('preview-loading')).toBeInTheDocument()
    await settlePreview(1)
    expect(previewOp).toHaveBeenCalledTimes(1)
    expect(previewOp).toHaveBeenCalledWith(tabId, a.path, { kind: 'resize', params: { ...RUST_DEFAULTS, percent: 25 } })

    const imgs = screen.getByTestId('compare-view').querySelectorAll('img')
    expect(imgs[0]).toHaveAttribute('src', `orig://${a.path}`)
    expect(originalImageUrl).toHaveBeenCalledWith(a)
    expect(imgs[1].getAttribute('src')).toMatch(/^blob:preview-/)
    // The result is stretched over the original for a like-for-like comparison.
    expect(imgs[1].style.width).toBe('512px')
    expect(imgs[1].style.height).toBe('256px')
    expect(screen.getByTestId('compare-before-label')).toHaveTextContent('512×256')
    expect(screen.getByTestId('compare-after-label')).toHaveTextContent('128×64')
    expect(screen.queryByTestId('preview-loading')).not.toBeInTheDocument()

    // Focus another file: preview follows the selection.
    act(() => session().select(b.id))
    await settlePreview()
    expect(previewOp).toHaveBeenCalledTimes(2)
    expect(vi.mocked(previewOp).mock.calls[1][1]).toBe(b.path)
    expect(screen.getByTestId('preview-file')).toHaveTextContent('b.png')
  })

  it('shows translated preview errors', async () => {
    vi.useFakeTimers()
    vi.mocked(previewOp).mockRejectedValue({ code: 'IMG_TOO_LARGE', params: { max: 32768, width: 40000, height: 4 } })
    session().addFiles([sizedFile(20000, 2)])
    session().setUiFlag('preview', true)
    renderTab(ResizeTab, tabId)
    await settlePreview()
    expect(screen.getByTestId('preview-error')).toHaveTextContent('Preview failed')
    expect(screen.getByTestId('preview-error-message')).toHaveTextContent('Image size 40000×4 exceeds the maximum 32768px')
    await act(() => i18n.changeLanguage('vi'))
    expect(screen.getByTestId('preview-error')).toHaveTextContent('Không thể xem trước')
  })

  it('does not request a preview while params are invalid', async () => {
    vi.useFakeTimers()
    session().addFiles([sizedFile(10, 10)])
    session().setUiFlag('preview', true)
    session().setParams({ percent: 0 })
    renderTab(ResizeTab, tabId)
    await settlePreview()
    expect(previewOp).not.toHaveBeenCalled()
    expect(screen.getByTestId('preview-message')).toHaveTextContent('Fix the highlighted parameters to see a preview.')
  })

  it('Run sends the resize request with the session output settings', async () => {
    const files = [sizedFile(100, 50), sizedFile(64, 64)]
    session().addFiles(files)
    session().setParams({ mode: 'exact', width: 300, height: 200, keepAspect: false, filter: 'nearest' })
    renderTab(ResizeTab, tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    expect(runOp).toHaveBeenCalledWith(
      tabId,
      { kind: 'resize', params: { ...RUST_DEFAULTS, mode: 'exact', width: 300, height: 200, keepAspect: false, filter: 'nearest' } },
      files.map((f) => f.path),
      session().output,
    )
  })
})
