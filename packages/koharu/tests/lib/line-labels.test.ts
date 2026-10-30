import { describe, expect, it } from 'vitest'

import { LETTERING_CATEGORIES } from '@/lib/line-labels'
import translations from '@/public/locales/en-US/translation.json'

describe('lettering categories', () => {
  it('lists fifteen unique categories, each with a name', () => {
    expect(new Set(LETTERING_CATEGORIES).size).toBe(15)
    expect(Object.keys(translations.lettering.categories)).toEqual([...LETTERING_CATEGORIES])
  })
})
