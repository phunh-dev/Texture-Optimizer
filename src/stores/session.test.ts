import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import i18n from '@/i18n'
import { redoTooltip, undoTooltip } from '@/lib/history'
import { makeFiles } from '@/lib/testing/dom'

import { COALESCE_MS, createSession, deleteSession, getSession, HISTORY_LIMIT, resetSessions } from './session'

const pastCount = (tabId: string) => getSession(tabId)!.temporal.getState().pastStates.length
const futureCount = (tabId: string) => getSession(tabId)!.temporal.getState().futureStates.length
const ids = (tabId: string) => getSession(tabId)!.getState().files.map((f) => f.id)

describe('session history (undo/redo)', () => {
  beforeEach(async () => {
    resetSessions()
    await i18n.changeLanguage('en')
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('add -> remove one (badge ×) -> undo restores exact list & order -> redo removes again', () => {
    const s = createSession('t1').getState()
    s.addFiles(makeFiles(5))
    expect(ids('t1')).toEqual(['f0', 'f1', 'f2', 'f3', 'f4'])

    expect(s.removeFiles(['f2'])).toBe(1)
    expect(ids('t1')).toEqual(['f0', 'f1', 'f3', 'f4'])

    getSession('t1')!.getState().undo()
    expect(ids('t1')).toEqual(['f0', 'f1', 'f2', 'f3', 'f4'])

    getSession('t1')!.getState().redo()
    expect(ids('t1')).toEqual(['f0', 'f1', 'f3', 'f4'])
  })

  it('removing several non-adjacent files and undoing restores their original positions', () => {
    const s = createSession('t1').getState()
    s.addFiles(makeFiles(6))
    s.removeFiles(['f0', 'f3', 'f5'])
    expect(ids('t1')).toEqual(['f1', 'f2', 'f4'])
    getSession('t1')!.getState().undo()
    expect(ids('t1')).toEqual(['f0', 'f1', 'f2', 'f3', 'f4', 'f5'])
  })

  it('dedupes added files by id and does not record a step when nothing new is added', () => {
    const s = createSession('t1').getState()
    const files = makeFiles(3)
    expect(s.addFiles(files)).toBe(3)
    expect(s.addFiles([files[0], files[1]])).toBe(0)
    expect(ids('t1')).toEqual(['f0', 'f1', 'f2'])
    expect(pastCount('t1')).toBe(1)
  })

  it('clearFiles can be undone and redone', () => {
    const s = createSession('t1').getState()
    s.addFiles(makeFiles(4))
    expect(s.clearFiles()).toBe(4)
    expect(ids('t1')).toEqual([])
    getSession('t1')!.getState().undo()
    expect(ids('t1')).toEqual(['f0', 'f1', 'f2', 'f3'])
    getSession('t1')!.getState().redo()
    expect(ids('t1')).toEqual([])
  })

  it('reorderFiles is undoable', () => {
    const s = createSession('t1').getState()
    s.addFiles(makeFiles(3))
    s.reorderFiles(0, 2)
    expect(ids('t1')).toEqual(['f1', 'f2', 'f0'])
    getSession('t1')!.getState().undo()
    expect(ids('t1')).toEqual(['f0', 'f1', 'f2'])
  })

  it('30 rapid coalesced setParams produce exactly ONE history step', () => {
    vi.useFakeTimers()
    const store = createSession('t1', { params: { quality: 0 } })
    for (let i = 1; i <= 30; i++) {
      store.getState().setParams({ quality: i }, { coalesce: true })
      vi.advanceTimersByTime(20)
    }
    expect(store.getState().params.quality).toBe(30)
    expect(pastCount('t1')).toBe(1)
    vi.advanceTimersByTime(COALESCE_MS + 10) // debounce closes the group
    expect(pastCount('t1')).toBe(1)

    store.getState().undo()
    expect(store.getState().params.quality).toBe(0)
    store.getState().redo()
    expect(store.getState().params.quality).toBe(30)
  })

  it('an explicit commit (pointer-up / blur) starts a new step for the next drag', () => {
    vi.useFakeTimers()
    const store = createSession('t1', { params: { a: 0 } })
    store.getState().setParams({ a: 1 }, { coalesce: true })
    store.getState().setParams({ a: 2 }, { coalesce: true })
    store.getState().commitParams()
    store.getState().setParams({ a: 3 }, { coalesce: true })
    store.getState().setParams({ a: 4 }, { coalesce: true })
    expect(pastCount('t1')).toBe(2)
    store.getState().undo()
    expect(store.getState().params.a).toBe(2)
    store.getState().undo()
    expect(store.getState().params.a).toBe(0)
  })

  it('non-coalesced setParams creates one step per change and ignores no-op changes', () => {
    const store = createSession('t1', { params: { mode: 'a' } })
    store.getState().setParams({ mode: 'b' })
    store.getState().setParams({ mode: 'b' })
    store.getState().setParams({ mode: 'c' })
    expect(pastCount('t1')).toBe(2)
  })

  it('resetParams and applyPreset are undoable', () => {
    const store = createSession('t1', { params: { size: 1, filter: 'lanczos' } })
    store.getState().setParams({ size: 5 })
    store.getState().resetParams()
    expect(store.getState().params).toEqual({ size: 1, filter: 'lanczos' })
    store.getState().applyPreset({ filter: 'nearest' })
    expect(store.getState().params).toEqual({ size: 1, filter: 'nearest' })
    store.getState().undo()
    expect(store.getState().params).toEqual({ size: 1, filter: 'lanczos' })
    store.getState().undo()
    expect(store.getState().params).toEqual({ size: 5, filter: 'lanczos' })
  })

  it('keeps at most 100 steps', () => {
    const store = createSession('t1', { params: { n: 0 } })
    for (let i = 1; i <= HISTORY_LIMIT + 30; i++) store.getState().setParams({ n: i })
    expect(pastCount('t1')).toBe(HISTORY_LIMIT)
    for (let i = 0; i < HISTORY_LIMIT + 10; i++) store.getState().undo()
    // Oldest reachable state is 30 steps after the start.
    expect(store.getState().params.n).toBe(30)
  })

  it('selection, view size and scroll are not tracked', () => {
    const store = createSession('t1')
    store.getState().addFiles(makeFiles(3))
    const before = pastCount('t1')
    store.getState().select('f1')
    store.getState().select('f2', 'toggle')
    store.getState().setViewSize('large')
    store.getState().setScrollTop(120)
    expect(pastCount('t1')).toBe(before)
    store.getState().undo()
    expect(store.getState().viewSize).toBe('large')
  })

  it('a new action after undo clears the redo stack', () => {
    const store = createSession('t1')
    store.getState().addFiles(makeFiles(3))
    store.getState().removeFiles(['f0'])
    store.getState().undo()
    expect(futureCount('t1')).toBe(1)
    store.getState().removeFiles(['f1'])
    expect(futureCount('t1')).toBe(0)
  })

  it('prunes the selection when selected files disappear through undo/redo', () => {
    const store = createSession('t1')
    store.getState().addFiles(makeFiles(3))
    store.getState().removeFiles(['f2'])
    store.getState().undo()
    store.getState().select('f2')
    store.getState().redo()
    expect(store.getState().selectedIds).toEqual([])
  })

  it('shift-click selects a range and ctrl-click toggles', () => {
    const store = createSession('t1')
    store.getState().addFiles(makeFiles(6))
    store.getState().select('f1')
    store.getState().select('f4', 'range')
    expect(store.getState().selectedIds).toEqual(['f1', 'f2', 'f3', 'f4'])
    store.getState().select('f2', 'toggle')
    expect(store.getState().selectedIds).toEqual(['f1', 'f3', 'f4'])
  })

  it('deleting a session drops its history', () => {
    const store = createSession('t1')
    store.getState().addFiles(makeFiles(2))
    deleteSession('t1')
    expect(getSession('t1')).toBeUndefined()
    expect(createSession('t1').temporal.getState().pastStates).toHaveLength(0)
  })

  it('describes the step in Undo/Redo tooltips ("Undo: Remove 3 images")', () => {
    const store = createSession('t1')
    store.getState().addFiles(makeFiles(5))
    store.getState().removeFiles(['f0', 'f1', 'f2'])
    const t = i18n.t
    expect(undoTooltip(t, store.getState().lastAction)).toBe('Undo: Remove 3 images')
    store.getState().undo()
    const future = store.temporal.getState().futureStates.at(-1)!.lastAction ?? null
    expect(redoTooltip(t, future)).toBe('Redo: Remove 3 images')
    expect(undoTooltip(t, store.getState().lastAction)).toBe('Undo: Add 5 images')
    expect(undoTooltip(t, null)).toBe('Nothing to undo')
  })
})
