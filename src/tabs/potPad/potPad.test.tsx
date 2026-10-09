import { act, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm'
import i18n from '@/i18n'
import { previewOp, runOp } from '@/lib/ipc'
import { installDomMocks } from '@/lib/testing/dom'
import { getSession } from '@/stores/session'
import { getTool } from '@/tabs/registry'

import { openTool, previewResult, renderTab, resetTabStores, settlePreview, sizedFile, stubObjectUrls, visibleKeys } from '../_imageOps/testing'
import PotPadTab from './index'
import { buildPotPadRequest, potPadPlacement, predictPotPad } from './request'
import { potPadSchema, type PotPadParams } from './schema'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  previewOp: vi.fn(),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  originalImageUrl: vi.fn((f: { path: string }) => `orig://${f.path}`),
}))

/** `impl Default for PotPadParams` (pot_pad.rs); color Color([0,0,0,255]) as a hex string. */
const DEFAULTS = {
  target: 'nextPot',
  width: 1024,
  height: 1024,
  anchor: 'center',
  fill: 'transparent',
  color: '#000000ff',
  minSize: 0,
  maxSize: 8192,
}
const RUST_DEFAULT_PARAMS = { ...DEFAULTS, color: [0, 0, 0, 255] }

const size = (w: number, h: number, patch: Partial<PotPadParams> = {}) => {
  const r = predictPotPad(w, h, { ...(DEFAULTS as PotPadParams), ...patch })
  return r.ok ? [r.width, r.height] : r.error
}

describe('potPad schema and request', () => {
  it('defaults equal the Rust Default impl', () => {
    expect(schemaDefaults(potPadSchema)).toEqual(DEFAULTS)
    expect(getTool('potPad').defaultParams()).toEqual(DEFAULTS)
    expect(buildPotPadRequest(DEFAULTS)).toEqual({ kind: 'potPad', params: RUST_DEFAULT_PARAMS })
  })

  it('builds the exact OpRequest JSON (serde names, color as [r, g, b, a])', () => {
    expect(
      buildPotPadRequest({
        target: 'fixed',
        width: 512,
        height: 256,
        anchor: 'topRight',
        fill: 'edgeExtend',
        color: '#05060708',
        minSize: 32,
        maxSize: 4096,
      }),
    ).toEqual({
      kind: 'potPad',
      params: { target: 'fixed', width: 512, height: 256, anchor: 'topRight', fill: 'edgeExtend', color: [5, 6, 7, 8], minSize: 32, maxSize: 4096 },
    })
    expect(buildPotPadRequest({ ...DEFAULTS, target: 'squarePot', fill: 'color' })!.params).toMatchObject({ target: 'squarePot', fill: 'color' })
    expect(buildPotPadRequest({ ...DEFAULTS, width: 0 })).toBeNull()
  })
})

describe('predicted sizes mirror pot_pad::target_size', () => {
  it('targets', () => {
    expect(size(100, 30)).toEqual([128, 32])
    expect(size(100, 30, { target: 'squarePot' })).toEqual([128, 128])
    expect(size(100, 30, { target: 'fixed', width: 256, height: 64 })).toEqual([256, 64])
    expect(size(1023, 1025)).toEqual([1024, 2048])
    expect(size(3, 1)).toEqual([4, 1])
  })

  it('already POT stays unchanged', () => {
    expect(size(64, 32)).toEqual([64, 32])
    expect(size(64, 64, { target: 'squarePot' })).toEqual([64, 64])
    expect(size(64, 32, { target: 'squarePot' })).toEqual([64, 64])
    expect(size(64, 32, { target: 'fixed', width: 64, height: 32 })).toEqual([64, 32])
  })

  it('min size', () => {
    expect(size(10, 10, { minSize: 100 })).toEqual([128, 128])
    expect(size(10, 10, { minSize: 4 })).toEqual([16, 16])
    expect(size(10, 10, { target: 'fixed', width: 20, height: 20, minSize: 100 })).toEqual([20, 20])
  })

  it('too large errors carry max / width / height', () => {
    expect(size(5000, 10, { maxSize: 4096 })).toEqual({ code: 'IMG_TOO_LARGE', params: { max: 4096, width: 8192, height: 16 } })
    expect(size(300, 100, { target: 'fixed', width: 256, height: 256 })).toEqual({ code: 'IMG_TOO_LARGE', params: { max: 256, width: 300, height: 100 } })
    expect(size(4, 4, { target: 'fixed', width: 9000, height: 16 })).toEqual({ code: 'IMG_TOO_LARGE', params: { max: 8192, width: 9000, height: 16 } })
    expect(size(9000, 3, { maxSize: 0 })).toEqual([16384, 4])
  })

  it('places the original by anchor for the preview', () => {
    const req = (anchor: string) => ({ kind: 'potPad' as const, params: { anchor } })
    expect(potPadPlacement(5, 6, 8, 8, req('center'))).toEqual({ x: -1, y: -1, width: 8, height: 8 })
    expect(potPadPlacement(5, 6, 8, 8, req('topLeft'))).toEqual({ x: 0, y: 0, width: 8, height: 8 })
    expect(potPadPlacement(5, 6, 8, 8, req('bottom'))).toEqual({ x: -1, y: -2, width: 8, height: 8 })
  })
})

describe('PotPadTab', () => {
  let tabId = ''
  const session = () => getSession(tabId)!.getState()
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetTabStores()
    vi.mocked(previewOp).mockReset()
    vi.mocked(runOp).mockClear()
    stubObjectUrls()
    await i18n.changeLanguage('en')
    tabId = openTool('potPad').tabId
  })
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('shows size fields for fixed and the color only for fill = color', () => {
    renderTab(PotPadTab, tabId)
    expect(visibleKeys(tabId)).toEqual(['target', 'minSize', 'maxSize', 'anchor', 'fill'])
    fireEvent.click(screen.getByRole('radio', { name: 'Fixed size' }))
    expect(visibleKeys(tabId)).toEqual(['target', 'width', 'height', 'maxSize', 'anchor', 'fill'])
    fireEvent.click(screen.getByRole('radio', { name: 'Color' }))
    expect(session().params.fill).toBe('color')
    expect(visibleKeys(tabId)).toEqual(['target', 'width', 'height', 'maxSize', 'anchor', 'fill', 'color'])
    fireEvent.click(screen.getByRole('radio', { name: 'Edge extend' }))
    expect(visibleKeys(tabId)).not.toContain('color')
  })

  it('lists predicted sizes, unchanged POT files and IMG_TOO_LARGE errors', () => {
    session().addFiles([sizedFile(1023, 1025), sizedFile(64, 32), sizedFile(5000, 10)])
    session().setParams({ maxSize: 4096 })
    renderTab(PotPadTab, tabId)
    const row = (w: number, h: number) => screen.getByTestId(`size-row-id-tex_${w}x${h}.png`)
    expect(within(row(1023, 1025)).getByTestId('size-out')).toHaveTextContent('1024×2048')
    expect(row(64, 32)).toHaveAttribute('data-state', 'unchanged')
    expect(row(5000, 10)).toHaveAttribute('data-state', 'error')
    expect(within(row(5000, 10)).getByTestId('size-error')).toHaveTextContent('Image size 8192×16 exceeds the maximum 4096px')
    expect(screen.getByTestId('size-counts')).toHaveTextContent('1 error')
  })

  it('previews with anchored placement and translates backend errors', async () => {
    vi.useFakeTimers()
    vi.mocked(previewOp).mockResolvedValueOnce(previewResult(8, 8))
    const file = sizedFile(5, 6)
    session().addFiles([file])
    session().setParams({ anchor: 'topLeft', fill: 'color', color: '#ff0000ff' })
    session().setUiFlag('preview', true)
    renderTab(PotPadTab, tabId)
    await settlePreview()
    expect(previewOp).toHaveBeenCalledWith(tabId, file.path, {
      kind: 'potPad',
      params: { ...RUST_DEFAULT_PARAMS, anchor: 'topLeft', fill: 'color', color: [255, 0, 0, 255] },
    })
    const after = screen.getByTestId('compare-view').querySelectorAll('img')[1]
    expect(after.style.transform).toMatch(/scale\([^)]*\) translate\(0px, 0px\)$/)
    expect(after.style.height).toBe('8px')

    vi.mocked(previewOp).mockRejectedValueOnce({ code: 'IMG_TOO_LARGE', params: { max: 4, width: 8, height: 8 } })
    act(() => session().setParams({ maxSize: 4 }))
    await settlePreview()
    expect(screen.getByTestId('preview-error-message')).toHaveTextContent('Image size 8×8 exceeds the maximum 4px')
  })

  it('Run sends the potPad request', async () => {
    const files = [sizedFile(100, 30), sizedFile(3, 3)]
    session().addFiles(files)
    session().setParams({ target: 'squarePot', fill: 'edgeExtend' })
    renderTab(PotPadTab, tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    expect(runOp).toHaveBeenCalledWith(
      tabId,
      { kind: 'potPad', params: { ...RUST_DEFAULT_PARAMS, target: 'squarePot', fill: 'edgeExtend' } },
      files.map((f) => f.path),
      session().output,
    )
  })
})
