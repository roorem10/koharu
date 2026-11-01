import { describe, expect, it } from 'vitest'

import { resources } from '@/lib/i18n'
import { orderedLanguageChoices } from '@/lib/translation'

function keys(value: object, prefix = ''): string[] {
  return Object.entries(value).flatMap(([key, item]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return item && typeof item === 'object' ? keys(item, path) : [path]
  })
}

function strings(value: object, prefix = ''): Array<[string, string]> {
  return Object.entries(value).flatMap(([key, item]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return typeof item === 'string' ? [[path, item]] : strings(item, path)
  })
}

function placeholders(value: string): string[] {
  return Array.from(value.matchAll(/{{\s*([^},\s]+).*?}}/g), ([, name]) => name).sort()
}

const pluralForm = /_(zero|one|two|few|many|other)$/

/** The English key a locale key mirrors: languages may add plural forms English lacks. */
function englishKey(key: string, english: Set<string>): string | undefined {
  if (english.has(key)) return key
  const other = key.replace(pluralForm, '_other')
  return other !== key && english.has(other) ? other : undefined
}

describe('editor localization', () => {
  it('defines the same flattened translation schema in every locale', () => {
    const expected = keys(resources['en-US'].translation).sort()
    const english = new Set(expected)
    for (const [locale, resource] of Object.entries(resources)) {
      expect('native' in resource.translation, locale).toBe(false)
      const own = keys(resource.translation)
      // Every English key exists, and extra keys are only plural forms of English plurals.
      expect(
        expected.filter((key) => !own.includes(key)),
        locale,
      ).toEqual([])
      expect(
        own.filter((key) => !englishKey(key, english)),
        locale,
      ).toEqual([])
    }
  })

  it('preserves interpolation variables and valid text in every locale', () => {
    const english = new Map(strings(resources['en-US'].translation))
    const englishKeys = new Set(english.keys())

    for (const [locale, resource] of Object.entries(resources)) {
      for (const [key, value] of strings(resource.translation)) {
        expect(value.trim(), `${locale}: ${key}`).not.toBe('')
        expect(placeholders(value), `${locale}: ${key}`).toEqual(
          placeholders(english.get(englishKey(key, englishKeys) ?? key) ?? ''),
        )
        expect(value, `${locale}: ${key}`).not.toMatch(/[ÃƒÃ‚]|Ã¢(?:â‚¬|â„¢|Å“)|Ã¯Â¿Â½|ï¿½/)
      }
    }
  })

  it('describes the available update and emphasizes its version in every locale', () => {
    for (const [locale, resource] of Object.entries(resources)) {
      expect(resource.translation.updater.available.description, locale).toContain(
        '<strong>{{version}}</strong>',
      )
    }
  })

  it('distinguishes a font origin from source text', () => {
    for (const [locale, resource] of Object.entries(resources)) {
      expect(resource.translation.fontPicker.source, locale).not.toBe(
        resource.translation.inspector.source,
      )
    }
  })

  it('orders language choices by their displayed name without mutating the source', () => {
    const languages = [
      { tag: 'ja-JP', name: 'Japanese' },
      { tag: 'zh-CN', name: 'Simplified Chinese' },
      { tag: 'en-US', name: 'English' },
      { tag: 'de-DE', name: 'German' },
    ]

    expect(orderedLanguageChoices(languages).map((language) => language.name)).toEqual([
      'English',
      'German',
      'Japanese',
      'Simplified Chinese',
    ])
    expect(languages.map((language) => language.name)).toEqual([
      'Japanese',
      'Simplified Chinese',
      'English',
      'German',
    ])
  })
})
