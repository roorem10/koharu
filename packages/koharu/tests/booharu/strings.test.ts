import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { booharuStrings, mergeStrings } from '@/lib/booharu/strings'
import { resources, supportedLanguages } from '@/lib/i18n'

/** Every string under `value`, by its dotted key. */
function strings(value: object, prefix = ''): Array<[string, string]> {
  return Object.entries(value).flatMap(([key, item]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return typeof item === 'string' ? [[path, item]] : strings(item, path)
  })
}

/** Koharu's own strings for a language, as upstream ships them. */
function koharuStrings(language: string): object {
  const file = join(import.meta.dirname, '../../public/locales', language, 'translation.json')
  return JSON.parse(readFileSync(file, 'utf8'))
}

const pluralForm = /_(zero|one|two|few|many|other)$/

describe('Booharu strings overlay', () => {
  it('merges objects key by key, lets Booharu values win, and changes neither input', () => {
    const koharu = { menu: { about: 'About', help: 'Help' }, title: 'Koharu' }
    const booharu = {
      menu: { about: 'About Booharu', github: 'Booharu on GitHub' },
      title: 'Booharu',
    }

    expect(mergeStrings(koharu, booharu)).toEqual({
      menu: { about: 'About Booharu', help: 'Help', github: 'Booharu on GitHub' },
      title: 'Booharu',
    })
    expect(koharu).toEqual({ menu: { about: 'About', help: 'Help' }, title: 'Koharu' })
  })

  it("keeps every Koharu string in each language and lets Booharu's win", () => {
    for (const language of supportedLanguages) {
      const expected = new Map([
        ...strings(koharuStrings(language)),
        ...strings(booharuStrings[language] ?? {}),
      ])
      expect(new Map(strings(resources[language].translation)), language).toEqual(expected)
    }
  })

  it("gives every language English's Booharu keys, apart from the plural forms it needs", () => {
    const english = new Set(strings(resources['en-US'].translation).map(([key]) => key))
    const isPluralForm = (key: string) =>
      pluralForm.test(key) && english.has(key.replace(pluralForm, '_other'))
    const expected = strings(booharuStrings['en-US']).map(([key]) => key)

    for (const language of supportedLanguages) {
      const own = strings(booharuStrings[language] ?? {}).map(([key]) => key)
      expect(
        expected.filter((key) => !own.includes(key)),
        language,
      ).toEqual([])
      expect(
        own.filter((key) => !expected.includes(key) && !isPluralForm(key)),
        language,
      ).toEqual([])
    }
  })
})
