import { act, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm'
import i18n from '@/i18n'
import { previewOp, runOp } from '@/lib/ipc'
import { installDomMocks } from '@/lib/testing/dom'
import { defaultOutputSettings, getSession } from '@/stores/session'
import { getTool } from '@/tabs/registry'

import { openTool, previewResult, renderTab, resetTabStores, settlePreview, sizedFile, stubObjectUrls, visibleKeys } from '../_imageOps/testing'
import TrimTab from './index'
import { useTrimMeasures } from './measures'
import { buildTrimRequest, trimOutputSettings, trimPlacement } from './request'
import { trimSchema } from './schema'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  previewOp: vi.fn(),
  runOp: vi.fn(() => Promise.resolve('job-1')),
  originalImageUrl: vi.fn((f: { path: string }) => `orig://${f.path}`),
}))

/** Copied from `impl Default for TrimParams` (crates/texopt-core/src/ops/trim.rs). */
const RUST_DEFAULTS = {
  alphaThreshold: 0,
  margin: 0,
  trimLeft: true,
  trimRight: true,
  trimTop: true,
  trimBottom: true,
  snap: 'none',
  emptyBehavior: 'error',
}

const meta = (x: number, y: number, w: number, h: number, sw = 20, sh = 10) => ({ sourceSize: { w: sw, h: sh }, trimRect: { x, y, w, h } })

describe('trim schema and request', () => {
  it('defaults equal the Rust Default impl (+ UI-only writeOffsets off)', () => {
    expect(schemaDefaults(trimSchema)).toEqual({ ...RUST_DEFAULTS, writeOffsets: false })
    expect(getTool('trim').defaultParams()).toEqual({ ...RUST_DEFAULTS, writeOffsets: false })
    expect(buildTrimRequest(schemaDefaults(trimSchema))).toEqual({ kind: 'trim', params: RUST_DEFAULTS })
  })

  it('builds the exact OpRequest JSON (writeOffsets is not an op param)', () => {
    expect(
      buildTrimRequest({
        alphaThreshold: 12,
        margin: 3,
        trimLeft: false,
        trimRight: true,
        trimTop: false,
        trimBottom: true,
        snap: 'multipleOf4',
        emptyBehavior: 'onePixel',
        writeOffsets: true,
      }),
    ).toEqual({
      kind: 'trim',
      params: {
        alphaThreshold: 12,
        margin: 3,
        trimLeft: false,
        trimRight: true,
        trimTop: false,
        trimBottom: true,
        snap: 'multipleOf4',
        emptyBehavior: 'onePixel',
      },
    })
    expect(buildTrimRequest({ ...RUST_DEFAULTS, alphaThreshold: 256 })).toBeNull()
  })

  it('"Write offsets JSON" maps to the writeMeta output setting', () => {
    const output = defaultOutputSettings()
    expect(trimOutputSettings(output, { writeOffsets: true })).toEqual({ ...output, writeMeta: true })
    expect(trimOutputSettings(output, { writeOffsets: false })).toEqual({ ...output, writeMeta: false })
  })

  it('places the trimmed result at trimRect (incl. negative offsets from snap)', () => {
    expect(trimPlacement(meta(4, 2, 6, 3))).toEqual({ x: 4, y: 2, width: 6, height: 3 })
    expect(trimPlacement(meta(-1, -2, 8, 8))).toEqual({ x: -1, y: -2, width: 8, height: 8 })
  })
})

describe('TrimTab', () => {
  let tabId = ''
  const session = () => getSession(tabId)!.getState()
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetTabStores()
    useTrimMeasures.setState({ byTab: {}, running: {} })
    vi.mocked(previewOp).mockReset()
    vi.mocked(runOp).mockClear()
    stubObjectUrls()
    await i18n.changeLanguage('en')
    tabId = openTool('trim').tabId
  })
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('exposes every trim param plus the offsets JSON option', () => {
    renderTab(TrimTab, tabId)
    expect(visibleKeys(tabId)).toEqual([
      'alphaThreshold',
      'margin',
      'emptyBehavior',
      'trimLeft',
      'trimRight',
      'trimTop',
      'trimBottom',
      'snap',
      'writeOffsets',
    ])
    // The sides group can be collapsed.
    fireEvent.click(screen.getByRole('button', { name: 'Sides to trim' }))
    expect(visibleKeys(tabId)).toEqual(['alphaThreshold', 'margin', 'emptyBehavior', 'snap', 'writeOffsets'])
  })

  it('previews with trimRect placement, shows the offset and records the measured size', async () => {
    vi.useFakeTimers()
    vi.mocked(previewOp).mockResolvedValue(previewResult(8, 8, meta(-1, 3, 8, 8)))
    const file = sizedFile(20, 10)
    session().addFiles([file, sizedFile(30, 30)])
    session().setParams({ snap: 'pot' })
    session().setUiFlag('preview', true)
    renderTab(TrimTab, tabId)
    expect(within(screen.getByTestId(`size-row-${file.id}`)).getByTestId('size-out')).toHaveTextContent('?')
    await settlePreview()
    expect(previewOp).toHaveBeenCalledWith(tabId, file.path, { kind: 'trim', params: { ...RUST_DEFAULTS, snap: 'pot' } })
    const after = screen.getByTestId('compare-view').querySelectorAll('img')[1]
    expect(after.style.transform).toMatch(/scale\([^)]*\) translate\(-1px, 3px\)$/)
    expect(after.style.width).toBe('8px')
    expect(screen.getByTestId('trim-offset')).toHaveTextContent('Offset -1, 3')
    expect(within(screen.getByTestId(`size-row-${file.id}`)).getByTestId('size-out')).toHaveTextContent('8×8')
    expect(screen.getByTestId('size-counts')).toHaveTextContent('1 not measured')

    // Changing params invalidates the measurement.
    act(() => session().setParams({ margin: 2 }))
    expect(within(screen.getByTestId(`size-row-${file.id}`)).getByTestId('size-out')).toHaveTextContent('?')
  })

  it('measures every file on demand; fully transparent images show the translated error', async () => {
    const a = sizedFile(20, 10)
    const empty = sizedFile(8, 8)
    vi.mocked(previewOp).mockImplementation((_tab, path) =>
      path === empty.path ? Promise.reject({ code: 'TRIM_EMPTY', params: { width: 8, height: 8 } }) : Promise.resolve(previewResult(6, 3, meta(4, 2, 6, 3))),
    )
    session().addFiles([a, empty])
    renderTab(TrimTab, tabId)
    const button = screen.getByTestId('measure-sizes')
    expect(button).toHaveTextContent('Measure 2 images')
    fireEvent.click(button)
    await waitFor(() => expect(within(screen.getByTestId(`size-row-${a.id}`)).getByTestId('size-out')).toHaveTextContent('6×3'))
    await waitFor(() => expect(screen.getByTestId(`size-row-${empty.id}`)).toHaveAttribute('data-state', 'error'))
    expect(previewOp).toHaveBeenCalledTimes(2)
    expect(within(screen.getByTestId(`size-row-${empty.id}`)).getByTestId('size-error')).toHaveTextContent(
      'Nothing to trim: every pixel of the 8×8 image is at or below the alpha threshold',
    )
    expect(screen.getByTestId('measure-sizes')).toBeDisabled()
  })

  it('Run writes offsets JSON only when enabled', async () => {
    const files = [sizedFile(20, 10)]
    session().addFiles(files)
    session().setParams({ alphaThreshold: 8, writeOffsets: true })
    renderTab(TrimTab, tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    expect(runOp).toHaveBeenCalledWith(
      tabId,
      { kind: 'trim', params: { ...RUST_DEFAULTS, alphaThreshold: 8 } },
      [files[0].path],
      { ...defaultOutputSettings(), writeMeta: true },
    )
  })

  it('Run without offsets keeps writeMeta off', async () => {
    session().addFiles([sizedFile(20, 10)])
    renderTab(TrimTab, tabId)
    fireEvent.click(screen.getByTestId('run-button'))
    await waitFor(() => expect(runOp).toHaveBeenCalled())
    expect(vi.mocked(runOp).mock.calls[0][3]).toEqual({ ...defaultOutputSettings(), writeMeta: false })
  })
})
