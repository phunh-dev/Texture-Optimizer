import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { installDomMocks, makeFile } from '@/lib/testing/dom'
import { resetSessions } from '@/stores/session'

import { AtlasPreview, AtlasView, fileForFrame } from './AtlasPreview'
import type { PreviewData } from './hooks'
import { atlasPreview, type AtlasFrame, type AtlasPreviewResult } from './ipc'
import rustDefaults from './rustDefaults.json'
import { atlasDefaults, buildAtlasRequest } from './schema'

vi.mock('./ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./ipc')>()),
  atlasPreview: vi.fn(),
  atlasLoadProject: vi.fn(() => Promise.resolve(null)),
}))

const frame = (name: string, page: number, x: number, y: number, w: number, h: number, extra: Partial<AtlasFrame> = {}): AtlasFrame => ({
  name,
  aliases: [],
  page,
  frame: { x, y, w, h },
  rotated: false,
  trimmed: false,
  sourceSize: { w, h },
  spriteSourceSize: { x: 0, y: 0, w, h },
  sourcePath: `C:/textures/${name}.png`,
  status: null,
  ...extra,
})

function result(overrides: Partial<AtlasPreviewResult> = {}): AtlasPreviewResult {
  return {
    pages: [
      { width: 128, height: 64, png: new ArrayBuffer(1) },
      { width: 32, height: 32, png: new ArrayBuffer(1) },
    ],
    frames: [
      frame('hero', 0, 0, 0, 40, 30, { trimmed: true, sourceSize: { w: 48, h: 36 }, spriteSourceSize: { x: 4, y: 3, w: 40, h: 30 } }),
      frame('coin', 0, 42, 0, 16, 16, { aliases: ['coin_copy'] }),
      frame('tree', 1, 0, 0, 20, 30, { rotated: true, spriteSourceSize: { x: 0, y: 0, w: 30, h: 20 }, sourceSize: { w: 30, h: 20 } }),
    ],
    stats: {
      pages: [],
      spriteCount: 3,
      frameCount: 4,
      occupancy: 0.4375,
    },
    warnings: [{ code: 'ATLAS_FEATURE_DISABLED', params: { feature: 'rotation', exporter: 'unity' } }],
    plan: [],
    hasPrevious: false,
    params: rustDefaults.params as AtlasPreviewResult['params'],
    ...overrides,
  }
}

const data = (r: AtlasPreviewResult = result()): PreviewData => ({ ...r, urls: r.pages.map((_, i) => `blob:page${i}`) })

const files = [
  makeFile({ id: 'f-hero', name: 'hero.png', path: 'C:\\textures\\hero.png' }),
  makeFile({ id: 'f-coin', name: 'coin.png', path: 'C:/textures/coin.png' }),
]

function renderView(d: PreviewData = data(), onSelectFile = vi.fn(), selectedIds: string[] = []) {
  render(
    <TooltipProvider>
      <AtlasView data={d} files={files} selectedIds={selectedIds} onSelectFile={onSelectFile} />
    </TooltipProvider>,
  )
  return onSelectFile
}

describe('AtlasView', () => {
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    await i18n.changeLanguage('en')
  })
  afterEach(cleanup)

  it('renders the page with one overlay rect per frame, and page tabs', () => {
    renderView()
    const img = screen.getByRole('img', { name: 'Atlas page preview' }).querySelector('img')!
    expect(img.getAttribute('src')).toBe('blob:page0')
    let rects = screen.getAllByTestId('atlas-frame')
    expect(rects.map((r) => r.getAttribute('data-name'))).toEqual(['hero', 'coin'])
    expect(rects[1].getAttribute('x')).toBe('42')
    expect(rects[1].getAttribute('width')).toBe('16')
    const tabs = screen.getAllByRole('tab')
    expect(tabs.map((t) => t.textContent)).toEqual(['Page 1', 'Page 2'])
    fireEvent.click(tabs[1])
    rects = screen.getAllByTestId('atlas-frame')
    expect(rects.map((r) => r.getAttribute('data-name'))).toEqual(['tree'])
    expect(screen.getByRole('img', { name: 'Atlas page preview' }).querySelector('img')!.getAttribute('src')).toBe('blob:page1')
  })

  it('hovering a frame shows a tooltip with name and size; click selects the source file', () => {
    const onSelect = renderView()
    const [hero, coin] = screen.getAllByTestId('atlas-frame')
    fireEvent.pointerEnter(hero)
    const tip = screen.getByRole('tooltip')
    expect(tip).toHaveTextContent('hero')
    expect(tip).toHaveTextContent('40×30 px')
    expect(tip).toHaveTextContent('Source 48×36 px')
    expect(tip).toHaveTextContent('Trimmed')
    expect(tip).toHaveTextContent('Click to select its image')
    fireEvent.pointerLeave(hero)
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument()
    fireEvent.pointerEnter(coin)
    expect(screen.getByRole('tooltip')).toHaveTextContent('Also: coin_copy')

    // Source path matching ignores separator style / case.
    fireEvent.click(hero)
    expect(onSelect).toHaveBeenCalledWith('f-hero')
    fireEvent.click(coin)
    expect(onSelect).toHaveBeenLastCalledWith('f-coin')
  })

  it('a sprite kept from the existing atlas is not selectable and says so', () => {
    const onSelect = renderView(
      data(
        result({
          frames: [frame('old', 0, 0, 0, 10, 10, { sourcePath: 'D:/gone/old.png', status: 'kept' })],
          hasPrevious: true,
          plan: [
            { name: 'old', status: 'kept', sourcePath: 'D:/gone/old.png' },
            { name: 'hero', status: 'new', sourcePath: 'C:/textures/hero.png' },
            { name: 'x', status: 'removed', sourcePath: null },
          ],
        }),
      ),
    )
    const rect = screen.getByTestId('atlas-frame')
    expect(rect.getAttribute('data-status')).toBe('kept')
    fireEvent.pointerEnter(rect)
    expect(screen.getByRole('tooltip')).toHaveTextContent('Not in the current list (kept from the existing atlas)')
    expect(within(screen.getByRole('tooltip')).getByText('Kept')).toBeInTheDocument()
    fireEvent.click(rect)
    expect(onSelect).not.toHaveBeenCalled()
    const plan = screen.getByTestId('atlas-plan')
    expect(plan).toHaveTextContent('Merged with the existing atlas')
    expect(plan).toHaveTextContent('1 kept')
    expect(plan).toHaveTextContent('1 new')
    expect(plan).toHaveTextContent('1 removed')
    expect(plan).not.toHaveTextContent('replaced')
  })

  it('shows stats and translated warnings; highlights selected sprites', () => {
    renderView(data(), vi.fn(), ['f-coin'])
    const stats = screen.getByTestId('atlas-stats')
    expect(stats).toHaveTextContent('Size: 128×64')
    expect(stats).toHaveTextContent('Pages: 2')
    expect(stats).toHaveTextContent('Used: 43.8%')
    expect(stats).toHaveTextContent('Sprites: 3')
    expect(stats).toHaveTextContent('4 names')
    expect(screen.getByTestId('atlas-warnings')).toHaveTextContent('Rotation was turned off because Unity does not support it')
    const [hero, coin] = screen.getAllByTestId('atlas-frame')
    expect(coin.getAttribute('data-selected')).toBe('true')
    expect(hero.getAttribute('data-selected')).toBeNull()
  })

  it('zoom buttons change the zoom level', () => {
    renderView()
    expect(screen.getByText('100%')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }))
    expect(screen.getByText('125%')).toBeInTheDocument()
  })

  it('fileForFrame falls back to the file stem', () => {
    const f = frame('coin', 0, 0, 0, 1, 1, { sourcePath: null })
    expect(fileForFrame(f, files)?.id).toBe('f-coin')
    expect(fileForFrame(frame('nope', 0, 0, 0, 1, 1, { sourcePath: null }), files)).toBeNull()
  })
})

describe('AtlasPreview (live)', () => {
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    vi.mocked(atlasPreview).mockReset()
    await i18n.changeLanguage('en')
    URL.createObjectURL = vi.fn(() => 'blob:x')
    URL.revokeObjectURL = vi.fn()
  })
  afterEach(cleanup)

  it('builds the preview (debounced) from files + params and refreshes on change', async () => {
    vi.mocked(atlasPreview).mockResolvedValue(result())
    const params = { ...atlasDefaults(), outputDir: 'D:/out' }
    const { rerender } = render(
      <TooltipProvider>
        <AtlasPreview tabId="t1" files={files} params={params} selectedIds={[]} />
      </TooltipProvider>,
    )
    expect(screen.getByText('Packing…')).toBeInTheDocument()
    await waitFor(() => expect(screen.getAllByTestId('atlas-frame')).toHaveLength(2))
    expect(atlasPreview).toHaveBeenCalledTimes(1)
    expect(atlasPreview).toHaveBeenCalledWith('t1', ['C:\\textures\\hero.png', 'C:/textures/coin.png'], buildAtlasRequest(params))

    const next = { ...params, padding: 8 }
    vi.mocked(atlasPreview).mockResolvedValue(result({ frames: [frame('hero', 0, 0, 0, 40, 30)] }))
    rerender(
      <TooltipProvider>
        <AtlasPreview tabId="t1" files={files} params={next} selectedIds={[]} />
      </TooltipProvider>,
    )
    await waitFor(() => expect(screen.getAllByTestId('atlas-frame')).toHaveLength(1))
    expect(vi.mocked(atlasPreview).mock.calls[1][2].params.padding).toBe(8)
    // The first result's page blobs were released when replaced.
    await act(async () => undefined)
    expect(URL.revokeObjectURL).toHaveBeenCalled()
  })

  it('shows a translated error and an empty state', async () => {
    vi.mocked(atlasPreview).mockRejectedValue({ code: 'ATLAS_DOES_NOT_FIT', params: { name: 'big', width: 5000, height: 10, maxWidth: 2048, maxHeight: 2048 } })
    render(
      <TooltipProvider>
        <AtlasPreview tabId="t2" files={files} params={atlasDefaults()} selectedIds={[]} />
      </TooltipProvider>,
    )
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Sprite "big" (5000×10) does not fit in an atlas of at most 2048×2048',
    )
    cleanup()
    render(
      <TooltipProvider>
        <AtlasPreview tabId="t3" files={[]} params={atlasDefaults()} selectedIds={[]} />
      </TooltipProvider>,
    )
    expect(screen.getByText('Add images to see the atlas.')).toBeInTheDocument()
  })
})
