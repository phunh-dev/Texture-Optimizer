import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { z } from 'zod'

import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { installDomMocks } from '@/lib/testing/dom'
import { usePresets } from '@/stores/presets'
import { createSession, getSession, resetSessions } from '@/stores/session'

import { ParamForm } from './ParamForm'
import { PresetBar } from './PresetBar'
import { defineFields, type FieldDescriptor } from './types'
import { schemaDefaults } from './validation'

const TAB = 'tab-form'
const TOOL = 'testtool'

const schema = z.object({
  count: z.number().int().min(1).max(10).default(4),
  strength: z.number().min(0).max(100).default(50),
  filter: z.enum(['nearest', 'lanczos']).default('lanczos'),
  mode: z.enum(['fit', 'exact']).default('fit'),
  keepRatio: z.boolean().default(true),
  fill: z.string().default('#ff000080'),
  suffix: z.string().regex(/^_/, { error: 'testtool:errors.suffix' }).default('_opt'),
  anchor: z.string().default('center'),
  size: z.object({ width: z.number().min(1), height: z.number().min(1) }).default({ width: 200, height: 100 }),
  exactOnly: z.number().default(7),
})

const fields: FieldDescriptor[] = defineFields([
  { kind: 'number', key: 'count', labelKey: 'testtool:count', unitKey: 'common:units.px', descKey: 'testtool:countDesc' },
  { kind: 'slider', key: 'strength', labelKey: 'testtool:strength', min: 0, max: 100 },
  {
    kind: 'select',
    key: 'filter',
    labelKey: 'testtool:filter',
    options: [
      { value: 'nearest', labelKey: 'testtool:nearest' },
      { value: 'lanczos', labelKey: 'testtool:lanczos' },
    ],
  },
  {
    kind: 'segmented',
    key: 'mode',
    labelKey: 'testtool:mode',
    options: [
      { value: 'fit', labelKey: 'testtool:fit' },
      { value: 'exact', labelKey: 'testtool:exact' },
    ],
  },
  { kind: 'number', key: 'exactOnly', labelKey: 'testtool:exactOnly', visibleIf: (p) => p.mode === 'exact' },
  { kind: 'switch', key: 'keepRatio', labelKey: 'testtool:keepRatio' },
  {
    kind: 'group',
    id: 'advanced',
    labelKey: 'testtool:advanced',
    fields: [
      { kind: 'color', key: 'fill', labelKey: 'testtool:fill', alpha: true },
      { kind: 'text', key: 'suffix', labelKey: 'testtool:suffix' },
      { kind: 'anchor', key: 'anchor', labelKey: 'testtool:anchor' },
      { kind: 'size', key: 'size', labelKey: 'testtool:size' },
    ],
  },
])

function renderForm() {
  return render(
    <TooltipProvider>
      <PresetBar tabId={TAB} toolId={TOOL} />
      <ParamForm tabId={TAB} fields={fields} schema={schema} />
      <Toaster />
    </TooltipProvider>,
  )
}

const params = () => getSession(TAB)!.getState().params
const steps = () => getSession(TAB)!.temporal.getState().pastStates.length

describe('ParamForm', () => {
  beforeAll(() => {
    installDomMocks()
    for (const lng of ['en', 'vi']) {
      i18n.addResourceBundle(lng, TOOL, {
        count: lng === 'en' ? 'Count' : 'Số lượng',
        countDesc: 'How many',
        strength: 'Strength',
        filter: 'Filter',
        nearest: 'Nearest',
        lanczos: 'Lanczos',
        mode: 'Mode',
        fit: 'Fit',
        exact: 'Exact',
        exactOnly: 'Exact value',
        keepRatio: 'Keep ratio',
        advanced: 'Advanced',
        fill: 'Fill color',
        suffix: 'Suffix',
        anchor: 'Anchor',
        size: 'Size',
        errors: { suffix: 'Suffix must start with _' },
      })
    }
  })
  beforeEach(async () => {
    resetSessions()
    localStorage.clear()
    usePresets.setState({ byTool: {}, loaded: {} })
    await i18n.changeLanguage('en')
    createSession(TAB, { params: schemaDefaults(schema) })
  })
  afterEach(cleanup)

  it('derives defaults from the zod schema', () => {
    expect(params()).toMatchObject({ count: 4, strength: 50, filter: 'lanczos', size: { width: 200, height: 100 } })
  })

  it('renders every descriptor kind from descriptors', () => {
    renderForm()
    expect(screen.getByRole('spinbutton', { name: 'Count' })).toHaveValue('4')
    expect(screen.getByText('px')).toBeInTheDocument()
    expect(screen.getByText('How many')).toBeInTheDocument()
    expect(screen.getByRole('slider', { name: 'Strength' })).toHaveAttribute('aria-valuenow', '50')
    expect(screen.getByRole('combobox', { name: 'Filter' })).toHaveTextContent('Lanczos')
    expect(screen.getByRole('radio', { name: 'Fit' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('switch', { name: 'Keep ratio' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('button', { name: 'Advanced' })).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByRole('textbox', { name: 'Hex color' })).toHaveValue('#ff000080')
    expect(screen.getByRole('spinbutton', { name: 'Opacity' })).toHaveValue('50')
    expect(screen.getByRole('textbox', { name: 'Suffix' })).toHaveValue('_opt')
    const anchors = within(screen.getByRole('radiogroup', { name: 'Anchor' })).getAllByRole('radio')
    expect(anchors).toHaveLength(9)
    expect(screen.getByRole('radio', { name: 'Center' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('spinbutton', { name: 'Width' })).toHaveValue('200')
    expect(screen.getByRole('spinbutton', { name: 'Height' })).toHaveValue('100')
    expect(screen.getByRole('button', { name: 'Keep width and height linked' })).toHaveAttribute('aria-pressed', 'true')
  })

  it('collapses groups', () => {
    renderForm()
    fireEvent.click(screen.getByRole('button', { name: 'Advanced' }))
    expect(screen.queryByRole('textbox', { name: 'Suffix' })).not.toBeInTheDocument()
  })

  it('honours visibleIf', () => {
    renderForm()
    expect(screen.queryByRole('spinbutton', { name: 'Exact value' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('radio', { name: 'Exact' }))
    expect(params().mode).toBe('exact')
    expect(screen.getByRole('spinbutton', { name: 'Exact value' })).toBeInTheDocument()
  })

  it('shows validation errors inline (built-in and custom i18n keys)', async () => {
    const user = userEvent.setup()
    renderForm()
    const count = screen.getByRole('spinbutton', { name: 'Count' })
    await user.clear(count)
    await user.type(count, '0')
    expect(params().count).toBe(0)
    expect(await screen.findByText('Must be at least 1')).toBeInTheDocument()
    expect(count).toHaveAttribute('aria-invalid', 'true')

    const suffix = screen.getByRole('textbox', { name: 'Suffix' })
    await user.clear(suffix)
    await user.type(suffix, 'x')
    expect(screen.getByText('Suffix must start with _')).toBeInTheDocument()
  })

  it('updates the session store; typing coalesces into one undo step', async () => {
    const user = userEvent.setup()
    renderForm()
    fireEvent.click(screen.getByRole('switch', { name: 'Keep ratio' }))
    expect(params().keepRatio).toBe(false)
    fireEvent.click(screen.getByRole('radio', { name: 'Bottom right' }))
    expect(params().anchor).toBe('bottomRight')
    expect(steps()).toBe(2)

    const suffix = screen.getByRole('textbox', { name: 'Suffix' })
    await user.type(suffix, '_lowres')
    expect(params().suffix).toBe('_opt_lowres')
    expect(steps()).toBe(3) // 7 keystrokes -> 1 step
    act(() => getSession(TAB)!.getState().undo())
    expect(params().suffix).toBe('_opt')
  })

  it('size field keeps the ratio while linked', async () => {
    const user = userEvent.setup()
    renderForm()
    const width = screen.getByRole('spinbutton', { name: 'Width' })
    await user.clear(width)
    await user.type(width, '400')
    expect(params().size).toEqual({ width: 400, height: 200 })
    fireEvent.click(screen.getByRole('button', { name: 'Keep width and height linked' }))
    await user.clear(width)
    await user.type(width, '50')
    expect(params().size).toEqual({ width: 50, height: 200 })
  })

  it('slider value changes coalesce into one undo step', async () => {
    const user = userEvent.setup()
    renderForm()
    const slider = screen.getByRole('slider', { name: 'Strength' })
    const numberBox = slider.closest('div.flex')!.querySelector('input')!
    await user.clear(numberBox)
    await user.type(numberBox, '73')
    expect(params().strength).toBe(73)
    expect(steps()).toBe(1)
    fireEvent.blur(numberBox)
    act(() => getSession(TAB)!.getState().undo())
    expect(params().strength).toBe(50)
  })

  it('saves, loads and deletes presets; Reset restores defaults', async () => {
    const user = userEvent.setup()
    renderForm()
    act(() => getSession(TAB)!.getState().setParams({ count: 9, filter: 'nearest' }))

    fireEvent.click(screen.getByRole('button', { name: 'Save as preset…' }))
    await user.type(await screen.findByLabelText('Preset name'), 'Pixel art')
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(usePresets.getState().byTool[TOOL]?.map((p) => p.name)).toEqual(['Pixel art']))
    expect(JSON.parse(localStorage.getItem(`presets.json:${TOOL}`)!)[0].params).toMatchObject({ count: 9, filter: 'nearest' })

    fireEvent.click(screen.getByRole('button', { name: 'Reset to defaults' }))
    expect(params()).toMatchObject({ count: 4, filter: 'lanczos' })
    expect(screen.getByRole('spinbutton', { name: 'Count' })).toHaveValue('4')

    fireEvent.click(screen.getByRole('button', { name: 'Presets' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Pixel art' }))
    expect(params()).toMatchObject({ count: 9, filter: 'nearest' })
    expect(getSession(TAB)!.getState().lastAction).toEqual({ key: 'applyPreset' })

    // Applying a preset is undoable.
    act(() => getSession(TAB)!.getState().undo())
    expect(params()).toMatchObject({ count: 4, filter: 'lanczos' })

    fireEvent.click(screen.getByRole('button', { name: 'Presets' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Delete preset Pixel art' }))
    await waitFor(() => expect(usePresets.getState().byTool[TOOL]).toEqual([]))
    expect(JSON.parse(localStorage.getItem(`presets.json:${TOOL}`)!)).toEqual([])
  })

  it('re-renders labels when the language changes', async () => {
    renderForm()
    expect(screen.getByRole('spinbutton', { name: 'Count' })).toBeInTheDocument()
    await act(() => i18n.changeLanguage('vi'))
    expect(screen.getByRole('spinbutton', { name: 'Số lượng' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Khôi phục mặc định' })).toBeInTheDocument()
  })
})
