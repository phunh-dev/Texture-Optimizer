import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import i18n from '@/i18n'
import { installDomMocks } from '@/lib/testing/dom'
import { createSession, getSession, resetSessions } from '@/stores/session'

import { ParamForm } from './ParamForm'
import { defineFields } from './types'

const TAB = 'tab-multi'
const fields = defineFields([
  {
    kind: 'multiSelect',
    key: 'channels',
    labelKey: 'mesh:params.channels.label',
    options: [
      { value: 'baseColor', labelKey: 'mesh:channels.baseColor' },
      { value: 'normal', labelKey: 'mesh:channels.normal' },
      { value: 'emissive', labelKey: 'mesh:channels.emissive' },
    ],
  },
])

describe('ParamForm multiSelect', () => {
  beforeAll(() => installDomMocks())
  beforeEach(async () => {
    resetSessions()
    createSession(TAB, { params: { channels: ['baseColor'] } })
    await i18n.changeLanguage('en')
  })
  afterEach(cleanup)

  it('toggles values, keeps option order and is undoable', () => {
    render(
      <TooltipProvider>
        <ParamForm tabId={TAB} fields={fields} />
      </TooltipProvider>,
    )
    expect(screen.getByRole('button', { name: 'Base color' })).toHaveAttribute('data-state', 'on')
    expect(screen.getByRole('button', { name: 'Emissive' })).toHaveAttribute('data-state', 'off')
    fireEvent.click(screen.getByRole('button', { name: 'Emissive' }))
    fireEvent.click(screen.getByRole('button', { name: 'Normal' }))
    expect(getSession(TAB)!.getState().params.channels).toEqual(['baseColor', 'normal', 'emissive'])
    fireEvent.click(screen.getByRole('button', { name: 'Base color' }))
    expect(getSession(TAB)!.getState().params.channels).toEqual(['normal', 'emissive'])
    getSession(TAB)!.getState().undo()
    expect(getSession(TAB)!.getState().params.channels).toEqual(['baseColor', 'normal', 'emissive'])
  })
})
