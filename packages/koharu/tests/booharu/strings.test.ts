import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { booharuTools } from '@/lib/booharu/keys'
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

/** Whether `key` is one of Booharu's own strings, which every language has, rather than an override
 * of one of Koharu's at Koharu's path: its own, its edits' history steps, and its tools' names,
 * which Koharu's tool rail looks up by the tool's id. */
const isBooharuKey = (key: string) =>
  key.startsWith('booharu.') ||
  key.startsWith('history.steps.booharu_') ||
  [...booharuTools].some((tool) => key === `tools.${tool}`)

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

  it("gives every language each of Booharu's own keys, apart from the plural forms it needs", () => {
    const english = new Set(strings(resources['en-US'].translation).map(([key]) => key))
    const isPluralForm = (key: string) =>
      pluralForm.test(key) && english.has(key.replace(pluralForm, '_other'))
    const booharuKeys = (language: string) =>
      strings(booharuStrings[language] ?? {})
        .map(([key]) => key)
        .filter(isBooharuKey)
    const expected = booharuKeys('en-US')

    for (const language of supportedLanguages) {
      const own = booharuKeys(language)
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

  it('overrides only Koharu strings, or plural forms they lack, and copies none', () => {
    // An override left behind when Koharu renames its key, or one that copies Koharu's text, would
    // hide Koharu's next wording without anything failing.
    for (const language of supportedLanguages) {
      const koharu = new Map(strings(koharuStrings(language)))
      const isPluralFill = (key: string) =>
        pluralForm.test(key) && koharu.has(key.replace(pluralForm, '_other'))
      expect(
        strings(booharuStrings[language] ?? {})
          .filter(([key]) => !isBooharuKey(key))
          .filter(([key, value]) =>
            koharu.has(key) ? koharu.get(key) === value : !isPluralFill(key),
          )
          .map(([key]) => key),
        language,
      ).toEqual([])
    }
  })

  it('gives every count string each plural form its language needs', () => {
    // i18next shows a missing form as the language's _other one, which reads wrong where the forms
    // differ, as in Arabic's two or Russian's two to four.
    const counts = strings(resources['en-US'].translation)
      .map(([key]) => key)
      .filter((key) => key.endsWith('_other'))
      .map((key) => key.replace(pluralForm, ''))

    for (const language of supportedLanguages) {
      const own = new Set(strings(resources[language].translation).map(([key]) => key))
      const forms = new Intl.PluralRules(language).resolvedOptions().pluralCategories
      expect(
        counts
          .flatMap((count) => forms.map((form) => `${count}_${form}`))
          .filter((key) => !own.has(key)),
        language,
      ).toEqual([])
    }
  })
})
