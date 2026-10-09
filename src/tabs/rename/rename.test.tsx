import { invoke } from '@tauri-apps/api/core'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import type { ImportedFile } from '@/lib/ipc/types'
import { installDomMocks, makeFile } from '@/lib/testing/dom'
import { getSession, resetSessions } from '@/stores/session'
import { useTabs } from '@/stores/tabs'
import { getTool } from '@/tabs/registry'

import RenameTab from '.'
import type { ExecuteOutcome, RenamePlanItem, RevertOutcome } from './api'
import { defaultRenameParams, renameSchema, toExecuteMode, toRenameParams } from './schema'

vi.mock('@tauri-apps/api/core', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/core')>()),
  invoke: vi.fn(),
}))

/** Literal copy of `impl Default for RenameParams` (crates/texopt-core/src/rename/mod.rs). */
const RUST_DEFAULTS = {
  template: '{name}',
  prefix: '',
  suffix: '',
  startNumber: 1,
  step: 1,
  zeroPad: 0,
  case: 'keep',
  findReplace: [],
  smart: { enabled: false, preset: 'unreal', customMap: {} },
  sortBy: 'none',
  sortDesc: false,
  keepExtension: true,
  extensionCase: 'keep',
  dateFormat: '%Y%m%d',
  sanitize: false,
  invalidCharReplacement: '_',
}

const DIR = 'C:/tex'
const fileAt = (name: string, id = name): ImportedFile => makeFile({ id, name, path: `${DIR}/${name}` })

type Handler = (args: Record<string, unknown>) => unknown
let handlers: Record<string, Handler> = {}
const calls = (cmd: string) => vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd).map(([, args]) => args as Record<string, unknown>)

let tabId = ''
const session = () => getSession(tabId)!.getState()

function renderTab() {
  return render(
    <TooltipProvider>
      <RenameTab tabId={tabId} />
      <Toaster />
    </TooltipProvider>,
  )
}

/** Plan with one rename, one unchanged file and one conflict. */
const MIXED_PLAN: RenamePlanItem[] = [
  { from: `${DIR}/a.png`, to: `${DIR}/T_a.png`, conflict: null },
  { from: `${DIR}/b.png`, to: `${DIR}/b.png`, conflict: null },
  { from: `${DIR}/c.png`, to: `${DIR}/taken.png`, conflict: 'existsOnDisk' },
]

describe('Pattern Renamer tab', () => {
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    vi.mocked(invoke).mockReset()
    handlers = {
      rename_plan: ({ paths }) => (paths as string[]).map((p) => ({ from: p, to: p.replace(/\/([^/]+)$/, '/T_$1'), conflict: null })),
    }
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const h = handlers[cmd]
      if (!h) throw new Error(`unexpected command ${cmd}`)
      return h((args ?? {}) as Record<string, unknown>)
    })
    await i18n.changeLanguage('en')
    tabId = useTabs.getState().openTab('rename')
  })
  afterEach(cleanup)

  it('schema / registry defaults equal the Rust defaults plus the UI-only destination', () => {
    const withUi = { ...RUST_DEFAULTS, mode: 'inPlace', copyDir: '' }
    expect(schemaDefaults(renameSchema)).toEqual(withUi)
    expect(defaultRenameParams()).toEqual(withUi)
    expect(getTool('rename').defaultParams()).toEqual(withUi)
    expect(session().params).toEqual(withUi)
    expect(toRenameParams(session().params)).toEqual(RUST_DEFAULTS)
  })

  it('sends exact params / mode JSON to rename_plan (UI keys and empty custom suffixes dropped)', async () => {
    session().addFiles([fileAt('a.png')])
    session().setParams({
      prefix: 'P_',
      smart: { enabled: true, preset: 'custom', customMap: { normal: 'NRM', roughness: '' } },
      mode: 'copyTo',
      copyDir: 'D:/out',
    })
    expect(JSON.stringify(toRenameParams(session().params))).toBe(
      '{"template":"{name}","prefix":"P_","suffix":"","startNumber":1,"step":1,"zeroPad":0,"case":"keep","findReplace":[],' +
        '"smart":{"enabled":true,"preset":"custom","customMap":{"normal":"NRM"}},"sortBy":"none","sortDesc":false,"keepExtension":true,' +
        '"extensionCase":"keep","dateFormat":"%Y%m%d","sanitize":false,"invalidCharReplacement":"_"}',
    )
    expect(toExecuteMode(session().params)).toEqual({ kind: 'copyTo', dir: 'D:/out' })
    renderTab()
    await waitFor(() => expect(calls('rename_plan')).toHaveLength(1))
    expect(calls('rename_plan')[0]).toEqual({
      paths: [`${DIR}/a.png`],
      params: toRenameParams(session().params),
      mode: { kind: 'copyTo', dir: 'D:/out' },
    })
  })

  it('renders the plan table: conflicts highlighted with a translated reason, unchanged rows dimmed, counts', async () => {
    handlers.rename_plan = () => MIXED_PLAN
    session().addFiles(['a.png', 'b.png', 'c.png'].map((n) => fileAt(n)))
    renderTab()
    const table = await screen.findByRole('table', { name: 'Rename preview' })
    const bodyRows = () => within(table).queryAllByRole('row').filter((r) => r.hasAttribute('aria-rowindex'))
    await waitFor(() => expect(bodyRows()).toHaveLength(3))
    const body = bodyRows()
    expect(body[0]).toHaveTextContent('a.png')
    expect(body[0]).toHaveTextContent('T_a.png')
    expect(body[0]).toHaveTextContent('OK')
    expect(body[1]).toHaveAttribute('data-unchanged', 'true')
    expect(body[1].className).toContain('opacity-50')
    expect(body[1]).toHaveTextContent('Unchanged')
    expect(body[2]).toHaveAttribute('data-conflict', 'existsOnDisk')
    expect(body[2].className).toContain('bg-destructive/10')
    expect(body[2]).toHaveTextContent('A file with this name already exists')
    const summary = screen.getByTestId('rename-summary')
    expect(summary).toHaveTextContent('3 files')
    expect(summary).toHaveTextContent('2 to rename')
    expect(summary).toHaveTextContent('1 unchanged')
    expect(summary).toHaveTextContent('1 conflict')
    expect(screen.getByTestId('rename-button')).toBeDisabled()
    expect(screen.getByText('Resolve 1 conflict first')).toBeInTheDocument()

    await act(() => i18n.changeLanguage('vi'))
    expect(body[2]).toHaveTextContent('Đã có tệp trùng tên')
    await act(() => i18n.changeLanguage('en'))
  })

  it('virtualizes long plans (only visible rows are mounted)', async () => {
    const files = Array.from({ length: 500 }, (_, i) => fileAt(`t${i}.png`))
    session().addFiles(files)
    renderTab()
    await waitFor(() => expect(screen.getByTestId('rename-summary')).toHaveTextContent('500 files'))
    const mounted = within(screen.getByRole('table')).queryAllByRole('row').filter((r) => r.hasAttribute('aria-rowindex'))
    expect(mounted.length).toBeGreaterThan(10)
    expect(mounted.length).toBeLessThan(100)
    expect(mounted[0]).toHaveTextContent('T_t0.png')
  })

  it('template token chips insert at the caret; unknown tokens are flagged live', async () => {
    session().setParams({ template: 'tex_{index}' })
    renderTab()
    const input = screen.getByRole('textbox', { name: 'Template' })
    ;(input as HTMLInputElement).setSelectionRange(4, 4)
    fireEvent.click(screen.getByRole('button', { name: 'Insert {name}' }))
    expect(session().params.template).toBe('tex_{name}{index}')
    expect(session().lastAction).toEqual({ key: 'changeParams' })

    fireEvent.change(input, { target: { value: '{name}_{foo}' } })
    expect(await screen.findByText('Unknown token {foo}')).toBeInTheDocument()
    expect(input).toHaveAttribute('aria-invalid', 'true')
  })

  it('find/replace rules: add, edit, toggle, reorder, remove; invalid regex shown inline', async () => {
    renderTab()
    fireEvent.click(screen.getByRole('button', { name: 'Add rule' }))
    fireEvent.click(screen.getByRole('button', { name: 'Add rule' }))
    expect(session().params.findReplace).toEqual([
      { find: '', replace: '', regex: false, caseSensitive: true },
      { find: '', replace: '', regex: false, caseSensitive: true },
    ])
    fireEvent.change(screen.getByRole('textbox', { name: 'Find (rule 1)' }), { target: { value: 'diffuse' } })
    fireEvent.change(screen.getByRole('textbox', { name: 'Replace with (rule 1)' }), { target: { value: 'albedo' } })
    fireEvent.change(screen.getByRole('textbox', { name: 'Find (rule 2)' }), { target: { value: '(\\d+)' } })
    fireEvent.click(within(screen.getByTestId('rule-1')).getByRole('button', { name: 'Regular expression ($1 inserts a group)' }))
    fireEvent.click(within(screen.getByTestId('rule-0')).getByRole('button', { name: 'Match case' }))
    expect(session().params.findReplace).toEqual([
      { find: 'diffuse', replace: 'albedo', regex: false, caseSensitive: false },
      { find: '(\\d+)', replace: '', regex: true, caseSensitive: true },
    ])

    fireEvent.click(screen.getByRole('button', { name: 'Move rule 2 up' }))
    expect((session().params.findReplace as { find: string }[]).map((r) => r.find)).toEqual(['(\\d+)', 'diffuse'])
    expect(screen.getByRole('button', { name: 'Move rule 1 up' })).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: 'Move rule 1 down' }))
    expect((session().params.findReplace as { find: string }[]).map((r) => r.find)).toEqual(['diffuse', '(\\d+)'])

    // The backend rejects rule 2's pattern: the error appears under that rule.
    handlers.rename_plan = () => {
      throw { code: 'RENAME_INVALID_REGEX', params: { pattern: '(', index: 1, detail: 'unclosed group' } }
    }
    session().addFiles([fileAt('a.png')])
    const alert = await within(screen.getByTestId('rule-1')).findByRole('alert')
    expect(alert).toHaveTextContent('Invalid regular expression: unclosed group')
    expect(screen.getByRole('textbox', { name: 'Find (rule 2)' })).toHaveAttribute('aria-invalid', 'true')
    expect(screen.getByRole('textbox', { name: 'Find (rule 1)' })).toHaveAttribute('aria-invalid', 'false')

    fireEvent.click(screen.getByRole('button', { name: 'Remove rule 1' }))
    expect(session().params.findReplace).toEqual([{ find: '(\\d+)', replace: '', regex: true, caseSensitive: true }])
  })

  it('execute: confirm → rename_execute → session paths updated (no history step) → Revert toast reverts', async () => {
    const a = fileAt('a.png', 'id-a')
    const b = fileAt('b.png', 'id-b')
    session().addFiles([a, b])
    const historyBefore = getSession(tabId)!.temporal.getState().pastStates.length
    const renamedA = { ...a, id: 'id-ta', name: 'T_a.png', path: `${DIR}/T_a.png` }
    const renamedB = { ...b, id: 'id-tb', name: 'T_b.png', path: `${DIR}/T_b.png` }
    const log = {
      id: '0001700000000000-0000',
      entries: [
        { from: a.path, to: renamedA.path },
        { from: b.path, to: renamedB.path },
      ],
      mode: { kind: 'inPlace' as const },
      timestamp: 1_700_000_000_000,
    }
    handlers.rename_execute = (): ExecuteOutcome => ({
      log,
      updates: [
        { from: a.path, file: renamedA },
        { from: b.path, file: renamedB },
      ],
    })
    handlers.rename_revert = (): RevertOutcome => ({
      log,
      updates: [
        { from: renamedA.path, file: a },
        { from: renamedB.path, file: b },
      ],
      removed: [],
    })
    renderTab()

    const button = screen.getByTestId('rename-button')
    await waitFor(() => expect(button).toBeEnabled())
    expect(button).toHaveTextContent('Rename 2 files')
    fireEvent.click(button)
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('Rename files?')).toBeInTheDocument()
    expect(within(dialog).getByTestId('rename-confirm-counts')).toHaveTextContent('2 files will be renamed')
    expect(calls('rename_execute')).toHaveLength(0)

    fireEvent.click(within(dialog).getByTestId('rename-confirm'))
    await waitFor(() => expect(calls('rename_execute')).toHaveLength(1))
    expect(calls('rename_execute')[0]).toEqual({ paths: [a.path, b.path], params: RUST_DEFAULTS, mode: { kind: 'inPlace' } })
    await waitFor(() => expect(session().files.map((f) => f.path)).toEqual([renamedA.path, renamedB.path]))
    expect(getSession(tabId)!.temporal.getState().pastStates.length).toBe(historyBefore)
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())

    expect(await screen.findByText('Renamed 2 files')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Revert' }))
    await waitFor(() => expect(calls('rename_revert')).toEqual([{ logId: log.id }]))
    await waitFor(() => expect(session().files.map((f) => f.path)).toEqual([a.path, b.path]))
    expect(await screen.findByText('Restored the original names of 2 files')).toBeInTheDocument()
  })

  it('execute errors are shown as translated toasts and leave the files alone', async () => {
    session().addFiles([fileAt('a.png')])
    handlers.rename_execute = () => {
      throw { code: 'RENAME_HAS_CONFLICTS', params: { count: 1 } }
    }
    renderTab()
    const button = screen.getByTestId('rename-button')
    await waitFor(() => expect(button).toBeEnabled())
    fireEvent.click(button)
    fireEvent.click(within(await screen.findByRole('dialog')).getByTestId('rename-confirm'))
    expect(await screen.findByText('Cannot rename: 1 conflict(s) must be resolved first')).toBeInTheDocument()
    expect(session().files.map((f) => f.path)).toEqual([`${DIR}/a.png`])
  })

  it('"Revert last rename" calls rename_revert_last and moves the files back', async () => {
    const renamed = fileAt('T_a.png', 'id-ta')
    const original = fileAt('a.png', 'id-a')
    session().addFiles([renamed])
    handlers.rename_revert_last = (): RevertOutcome => ({
      log: { id: '1-0', entries: [{ from: original.path, to: renamed.path }], mode: { kind: 'inPlace' }, timestamp: 1 },
      updates: [{ from: renamed.path, file: original }],
      removed: [],
    })
    renderTab()
    fireEvent.click(screen.getByRole('button', { name: 'Revert last rename' }))
    await waitFor(() => expect(calls('rename_revert_last')).toHaveLength(1))
    await waitFor(() => expect(session().files).toEqual([original]))
    expect(await screen.findByText('Restored the original name of 1 file')).toBeInTheDocument()

    handlers.rename_revert_last = () => {
      throw { code: 'RENAME_NO_LOG', params: {} }
    }
    fireEvent.click(screen.getByRole('button', { name: 'Revert last rename' }))
    expect(await screen.findByText('There is no rename left to revert')).toBeInTheDocument()
  })

  it('copy mode needs a folder; then plans and executes with copyTo', async () => {
    session().addFiles([fileAt('a.png')])
    session().setParams({ mode: 'copyTo' })
    renderTab()
    expect(await screen.findByText('Choose a destination folder first')).toBeInTheDocument()
    expect(screen.getByText('No folder selected')).toBeInTheDocument()
    await waitFor(() => expect(calls('rename_plan').at(-1)?.mode).toEqual({ kind: 'inPlace' }))

    act(() => session().setParams({ copyDir: 'D:/out' }))
    await waitFor(() => expect(calls('rename_plan').at(-1)?.mode).toEqual({ kind: 'copyTo', dir: 'D:/out' }))
    const button = screen.getByTestId('rename-button')
    await waitFor(() => expect(button).toBeEnabled())
    expect(button).toHaveTextContent('Copy 1 file')
  })
})
