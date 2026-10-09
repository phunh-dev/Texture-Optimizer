import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import type { ImportedFile } from '@/lib/ipc/types'
import { installDomMocks } from '@/lib/testing/dom'
import { createSession, getSession, resetSessions } from '@/stores/session'

import { MeshAtlasPreview, overlaysForPage } from './AtlasPreview'
import { meshDefaults } from './defaults'
import type { MaterialReport, ModelReport, PreviewPayload } from './ipc'
import { pageUrl } from './preview'

const file = (name: string): ImportedFile => ({ id: `id-${name}`, path: `C:\\m\\${name}`, name, ext: 'obj', width: 0, height: 0, sizeBytes: 1, mtimeMs: 1 })

function material(index: number, name: string, page: number | null, rect: MaterialReport['rect'], status: MaterialReport['status'] = 'inRange'): MaterialReport {
  return {
    materialIndex: index,
    name,
    status,
    uvChannel: 0,
    uvRange: null,
    channels: ['baseColor'],
    layoutChannel: 'baseColor',
    sourceSize: rect ? [rect.width, rect.height] : null,
    tileSize: rect ? [rect.width, rect.height] : null,
    tiles: [1, 1],
    page,
    rect,
    remap: null,
  }
}

function model(name: string, materials: MaterialReport[]): ModelReport {
  return { source: `C:/m/${name}`, name, format: 'obj', outcome: 'rewritten', output: null, outputFormat: null, sidecar: null, files: [], pages: [0], materials, warnings: [], error: null }
}

const payload: PreviewPayload = {
  channel: 'baseColor',
  images: [
    { width: 256, height: 128, png: 'AAAA' },
    { width: 64, height: 64, png: 'BBBB' },
  ],
  report: {
    version: 1,
    generator: 'texture-optimizer',
    uvOrigin: 'bottomLeft',
    scalePercent: 50,
    channels: ['baseColor', 'normal'],
    pages: [
      { index: 0, width: 256, height: 128, occupancy: 0.5, textures: [] },
      { index: 1, width: 64, height: 64, occupancy: 0.25, textures: [] },
    ],
    models: [
      model('crate.obj', [material(0, 'Wood', 0, { x: 4, y: 4, width: 128, height: 64 }), material(1, 'Tiles', null, null, 'skipped')]),
      model('barrel.obj', [material(0, 'Metal', 0, { x: 140, y: 4, width: 64, height: 64 }), material(1, 'Rim', 1, { x: 4, y: 4, width: 32, height: 32 })]),
    ],
    files: [],
    reportPath: null,
    warnings: [{ code: 'MESH_TEXTURES_DOWNSCALED', params: { percent: 50 } }],
  },
}

const files = [file('crate.obj'), file('barrel.obj')]

function renderPreview(tabId: string, state: Parameters<typeof MeshAtlasPreview>[0]['state'], selectedIds: string[] = []) {
  return render(
    <TooltipProvider>
      <MeshAtlasPreview tabId={tabId} files={files} params={meshDefaults()} selectedIds={selectedIds} state={state} />
    </TooltipProvider>,
  )
}

describe('MeshAtlasPreview', () => {
  const tabId = 'tab-preview'
  beforeAll(() => installDomMocks({ width: 600, height: 400 }))
  beforeEach(async () => {
    resetSessions()
    createSession(tabId, { files })
    await i18n.changeLanguage('en')
  })
  afterEach(cleanup)

  it('maps rects to session files by path (case and separators insensitive)', () => {
    const o = overlaysForPage(payload, 0, files)
    expect(o.map((x) => `${x.model.name}/${x.material.name}`)).toEqual(['crate.obj/Wood', 'barrel.obj/Metal'])
    expect(o.every((x) => x.file !== null)).toBe(true)
    expect(overlaysForPage(payload, 1, files)).toHaveLength(1)
  })

  it('draws rect overlays at their atlas position with hover info; click selects the model', () => {
    const urls = payload.images.map((i) => pageUrl(i.png))
    renderPreview(tabId, { data: payload, urls, error: null, loading: false })
    expect(screen.getByTestId('page-size')).toHaveTextContent('256×128 (POT)')
    expect(screen.getByTestId('occupancy')).toHaveTextContent('50% used')
    expect(screen.getByText('Scale 50%')).toBeInTheDocument()
    expect(screen.getByRole('img', { name: 'Page 1' })).toHaveAttribute('src', 'data:image/png;base64,AAAA')
    const rects = screen.getAllByTestId('atlas-rect')
    expect(rects).toHaveLength(2)
    expect(rects[0].style.left).toBe(`${(4 / 256) * 100}%`)
    expect(rects[0].style.width).toBe('50%')
    expect(rects[0].style.height).toBe('50%')
    expect(rects[1]).toHaveAttribute('title', 'barrel.obj › Metal — 64×64')
    fireEvent.mouseEnter(rects[0])
    expect(screen.getByTestId('rect-hover')).toHaveTextContent('crate.obj › Wood — 128×64')
    fireEvent.click(rects[1])
    expect(getSession(tabId)!.getState().selectedIds).toEqual(['id-barrel.obj'])

    // Per-material UV status list + warnings.
    const statuses = screen.getAllByTestId('preview-material').map((li) => li.textContent)
    expect(statuses).toEqual(['WoodPage 1In range', 'TilesSkipped', 'MetalPage 1In range', 'RimPage 2In range'])
    expect(screen.getByTestId('preview-warnings')).toHaveTextContent('Textures were scaled to 50% to fit one atlas page')

    // Second page.
    fireEvent.click(screen.getByRole('radio', { name: 'Page 2' }))
    expect(screen.getAllByTestId('atlas-rect')).toHaveLength(1)
    expect(screen.getByTestId('page-size')).toHaveTextContent('64×64 (POT)')
  })

  it('highlights the rects of selected models', () => {
    renderPreview(tabId, { data: payload, urls: ['a', 'b'], error: null, loading: false }, ['id-crate.obj'])
    const rects = screen.getAllByTestId('atlas-rect')
    expect(rects[0].dataset.selected).toBe('true')
    expect(rects[1].dataset.selected).toBe('false')
  })

  it('shows loading and translated errors before a layout exists', () => {
    renderPreview(tabId, { data: null, urls: [], error: null, loading: true })
    expect(screen.getByTestId('preview-status')).toHaveTextContent('Packing…')
    cleanup()
    renderPreview(tabId, { data: null, urls: [], error: { code: 'ATLAS_DOES_NOT_FIT', params: { name: 'crate.obj / Wood', width: 9000, height: 10, maxWidth: 2048, maxHeight: 2048 } }, loading: false })
    expect(screen.getByTestId('preview-status')).toHaveTextContent('Preview failed')
    expect(screen.getByTestId('preview-status')).toHaveTextContent('crate.obj / Wood')
  })
})
