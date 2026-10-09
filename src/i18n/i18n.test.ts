import { describe, expect, it } from 'vitest'

import i18n, { detectLanguage } from '.'

describe('i18n', () => {
  it('detects Vietnamese locales and falls back to English', () => {
    expect(detectLanguage('vi-VN')).toBe('vi')
    expect(detectLanguage('vi')).toBe('vi')
    expect(detectLanguage('en-US')).toBe('en')
    expect(detectLanguage('fr-FR')).toBe('en')
    expect(detectLanguage(undefined)).toBe('en')
  })

  it('resolves keys by id in both languages and switches at runtime', async () => {
    await i18n.changeLanguage('en')
    expect(i18n.t('common:actions.undo')).toBe('Undo')
    await i18n.changeLanguage('vi')
    expect(i18n.t('common:actions.undo')).toBe('Hoàn tác')
  })

  it('interpolates error parameters', async () => {
    await i18n.changeLanguage('en')
    expect(i18n.t('errors:IMG_TOO_LARGE', { max: 4096, width: 5000, height: 100 })).toBe('Image size 5000×100 exceeds the maximum 4096px')
  })
})
