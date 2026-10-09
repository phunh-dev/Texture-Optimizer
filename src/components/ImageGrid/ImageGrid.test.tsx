import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { importFromFilePicker, importFromFolderPicker } from '@/lib/import'
import { thumbnailUrl } from '@/lib/ipc'
import { installDomMocks, makeFile, makeFiles } from '@/lib/testing/dom'
import { createSession, getSession, resetSessions } from '@/stores/session'

import { ImageGrid } from './ImageGrid'

const env = vi.hoisted(() => ({ tauri: false }))
vi.mock('@/lib/env', () => ({ inTauri: () => env.tauri }))
vi.mock('@/lib/import', () => ({
  importFromFilePicker: vi.fn(() => Promise.resolve()),
  importFromFolderPicker: vi.fn(() => Promise.resolve()),
}))
vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  thumbnailUrl: vi.fn((file: { path: string }, size: string) => `thumb://localhost/${file.path}?size=${size}`),
}))

const TAB = 'tab-grid'

function renderGrid() {
  return render(
    <TooltipProvider>
      <ImageGrid tabId={TAB} />
      <Toaster />
    </TooltipProvider>,
  )
}

const ids = () => getSession(TAB)!.getState().files.map((f) => f.id)
const cellOf = (name: string) => screen.getByRole('gridcell', { name })

describe('ImageGrid', () => {
  beforeAll(() => installDomMocks({ width: 1000, height: 800 }))
  beforeEach(async () => {
    resetSessions()
    env.tauri = false
    vi.mocked(importFromFilePicker).mockClear()
    vi.mocked(importFromFolderPicker).mockClear()
    await i18n.changeLanguage('en')
  })
  afterEach(cleanup)

  it('shows the empty drop zone with Add files / Add folder when there are no images', () => {
    createSession(TAB)
    renderGrid()
    const zone = screen.getByTestId('empty-drop-zone')
    expect(within(zone).getByText('Drop images or folders here')).toBeInTheDocument()
    fireEvent.click(within(zone).getByRole('button', { name: 'Add files' }))
    expect(importFromFilePicker).toHaveBeenCalledWith(TAB)
    fireEvent.click(within(zone).getByRole('button', { name: 'Add folder' }))
    expect(importFromFolderPicker).toHaveBeenCalledWith(TAB)
    expect(screen.queryByRole('grid')).not.toBeInTheDocument()
  })

  it('renders Small / Medium / Large with different cell sizes and column counts', () => {
    const session = createSession(TAB)
    session.getState().addFiles(makeFiles(12))
    renderGrid()
    const expected = { small: [96, 9], medium: [160, 5], large: [256, 3] } as const
    for (const size of ['small', 'medium', 'large'] as const) {
      act(() => session.getState().setViewSize(size))
      const cell = cellOf('tex_0.png')
      expect(cell).toHaveAttribute('data-size', size)
      expect(cell.style.width).toBe(`${expected[size][0]}px`)
      expect(screen.getByRole('grid')).toHaveAttribute('data-columns', String(expected[size][1]))
    }
  })

  it('has a "+" tile as the last cell that opens the file picker', () => {
    createSession(TAB).getState().addFiles(makeFiles(7))
    renderGrid()
    const cells = screen.getAllByRole('gridcell')
    expect(cells).toHaveLength(8)
    const last = cells[cells.length - 1]
    const add = within(last).getByRole('button', { name: 'Add images' })
    fireEvent.click(add)
    expect(importFromFilePicker).toHaveBeenCalledWith(TAB)
  })

  it('remove badge × removes exactly that file, and the toast Undo restores it', async () => {
    createSession(TAB).getState().addFiles(makeFiles(4))
    renderGrid()
    fireEvent.click(within(cellOf('tex_2.png')).getByRole('button', { name: 'Remove tex_2.png from the list' }))
    expect(ids()).toEqual(['f0', 'f1', 'f3'])
    expect(screen.queryByRole('gridcell', { name: 'tex_2.png' })).not.toBeInTheDocument()

    expect(await screen.findByText('Removed 1 image from the list')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }))
    expect(ids()).toEqual(['f0', 'f1', 'f2', 'f3'])
    await waitFor(() => expect(cellOf('tex_2.png')).toBeInTheDocument())
  })

  it('remove badges are always visible in Small mode and hover-only otherwise', () => {
    const session = createSession(TAB)
    session.getState().addFiles(makeFiles(2))
    renderGrid()
    const badge = () => within(cellOf('tex_0.png')).getByTestId('remove-badge')
    expect(badge().className).toContain('opacity-0')
    act(() => session.getState().setViewSize('small'))
    expect(badge().className).toContain('opacity-100')
    expect(badge().className).not.toContain('opacity-0')
  })

  it('click selects, Ctrl/Cmd+click toggles, Shift+click selects a range', () => {
    const session = createSession(TAB)
    session.getState().addFiles(makeFiles(6))
    renderGrid()
    fireEvent.click(cellOf('tex_1.png'))
    expect(session.getState().selectedIds).toEqual(['f1'])
    fireEvent.click(cellOf('tex_3.png'), { ctrlKey: true })
    expect(session.getState().selectedIds).toEqual(['f1', 'f3'])
    fireEvent.click(cellOf('tex_1.png'), { metaKey: true })
    expect(session.getState().selectedIds).toEqual(['f3'])
    // Ctrl+click moved the range anchor (Explorer behaviour); a plain click sets it again.
    fireEvent.click(cellOf('tex_3.png'))
    fireEvent.click(cellOf('tex_5.png'), { shiftKey: true })
    expect(session.getState().selectedIds).toEqual(['f3', 'f4', 'f5'])
    expect(cellOf('tex_4.png')).toHaveAttribute('aria-selected', 'true')
    expect(cellOf('tex_0.png')).toHaveAttribute('aria-selected', 'false')
  })

  it('Delete / Backspace removes the selection (undoable)', () => {
    const session = createSession(TAB)
    session.getState().addFiles(makeFiles(5))
    renderGrid()
    fireEvent.click(cellOf('tex_1.png'))
    fireEvent.click(cellOf('tex_3.png'), { ctrlKey: true })
    fireEvent.keyDown(screen.getByRole('grid'), { key: 'Delete' })
    expect(ids()).toEqual(['f0', 'f2', 'f4'])
    fireEvent.click(cellOf('tex_0.png'))
    fireEvent.keyDown(screen.getByRole('grid'), { key: 'Backspace' })
    expect(ids()).toEqual(['f2', 'f4'])
    act(() => session.getState().undo())
    act(() => session.getState().undo())
    expect(ids()).toEqual(['f0', 'f1', 'f2', 'f3', 'f4'])
  })

  it('Ctrl+A selects all and Escape clears the selection', () => {
    const session = createSession(TAB)
    session.getState().addFiles(makeFiles(3))
    renderGrid()
    fireEvent.keyDown(screen.getByRole('grid'), { key: 'a', ctrlKey: true })
    expect(session.getState().selectedIds).toEqual(['f0', 'f1', 'f2'])
    fireEvent.keyDown(screen.getByRole('grid'), { key: 'Escape' })
    expect(session.getState().selectedIds).toEqual([])
  })

  it('shows warning badges for non-POT and not-divisible-by-4 sizes', () => {
    createSession(TAB).getState().addFiles([
      makeFile({ id: 'pot', name: 'pot.png', width: 256, height: 512 }),
      makeFile({ id: 'npot4', name: 'npot4.png', width: 200, height: 100 }),
      makeFile({ id: 'odd', name: 'odd.png', width: 6, height: 6 }),
    ])
    renderGrid()
    const badges = (name: string) => ({
      npot: within(cellOf(name)).queryByTestId('badge-npot'),
      mul4: within(cellOf(name)).queryByTestId('badge-mul4'),
    })
    expect(badges('pot.png')).toEqual({ npot: null, mul4: null })
    expect(badges('npot4.png').npot).toBeInTheDocument()
    expect(badges('npot4.png').mul4).toBeNull()
    expect(badges('odd.png').npot).toBeInTheDocument()
    expect(badges('odd.png').mul4).toBeInTheDocument()
    expect(within(cellOf('npot4.png')).getByText('200 × 100')).toBeInTheDocument()
  })

  it('uses thumb:// URLs with the current size inside Tauri and a placeholder outside', () => {
    const session = createSession(TAB)
    session.getState().addFiles(makeFiles(1))
    const { unmount } = renderGrid()
    expect(within(cellOf('tex_0.png')).getByTestId('thumb-placeholder')).toBeInTheDocument()
    unmount()

    env.tauri = true
    session.getState().setViewSize('large')
    renderGrid()
    const img = cellOf('tex_0.png').querySelector('img')!
    expect(img).toHaveAttribute('loading', 'lazy')
    expect(img.getAttribute('src')).toContain('size=large')
    expect(thumbnailUrl).toHaveBeenCalledWith(expect.objectContaining({ id: 'f0' }), 'large')
  })

  it('virtualizes: renders only the visible rows of 2000 images', () => {
    createSession(TAB).getState().addFiles(makeFiles(2000))
    renderGrid()
    const rendered = screen.getAllByRole('gridcell').length
    expect(rendered).toBeGreaterThan(0)
    expect(rendered).toBeLessThan(200)
  })
})
