import { describe, expect, it } from 'vitest'

import type { OutputSettings } from '@/lib/ipc/types'

import { outputProblem } from './OutputSettingsPanel'

const base: OutputSettings = {
  mode: { kind: 'inPlace' },
  format: 'keep',
  pngCompression: 'default',
  jpgQuality: 90,
  optimizePng: false,
  conflict: 'autoRename',
}

describe('outputProblem', () => {
  it('accepts in-place, a chosen folder and a normal suffix', () => {
    expect(outputProblem(base)).toBeNull()
    expect(outputProblem({ ...base, mode: { kind: 'folder', path: 'C:/out' } })).toBeNull()
    expect(outputProblem({ ...base, mode: { kind: 'suffix', suffix: '_opt' } })).toBeNull()
  })

  it('requires a folder in folder mode', () => {
    expect(outputProblem({ ...base, mode: { kind: 'folder', path: '' } })).toBe('output.noFolder')
  })

  it('rejects an empty or blank suffix (it would overwrite the source)', () => {
    expect(outputProblem({ ...base, mode: { kind: 'suffix', suffix: '' } })).toBe('output.noSuffix')
    expect(outputProblem({ ...base, mode: { kind: 'suffix', suffix: '  ' } })).toBe('output.noSuffix')
  })

  it('rejects a suffix containing a path separator', () => {
    expect(outputProblem({ ...base, mode: { kind: 'suffix', suffix: 'a/b' } })).toBe('output.badSuffix')
    expect(outputProblem({ ...base, mode: { kind: 'suffix', suffix: 'a\\b' } })).toBe('output.badSuffix')
  })
})
