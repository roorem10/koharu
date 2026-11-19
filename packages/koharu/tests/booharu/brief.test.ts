import { describe, expect, it } from 'vitest'

import { joinPages, pageRuns, readTime } from '@/lib/booharu/brief'

describe('Booharu chapter brief formatting', () => {
  it('lists a character’s pages once each, in order, with pages in a row as one run', () => {
    expect(pageRuns([9, 1, 2, 3, 5, 3, 10])).toEqual(['1–3', '5', '9–10'])
    expect(pageRuns([])).toEqual([])
    expect(joinPages([4, 1, 2], 'en-US')).toBe('1–2, 4')
    expect(joinPages([4, 1, 2], 'ja-JP')).toBe('1–2、4')
    // Wu formats as Mandarin, which Intl has.
    expect(joinPages([4, 1, 2], 'wuu')).toBe(joinPages([4, 1, 2], 'zh-CN'))
  })

  it('writes when the chapter was read as the language does, and an unreadable time as it is', () => {
    const at = '2026-11-19T09:30:00Z'
    const english = new Intl.DateTimeFormat('en-US', { dateStyle: 'medium', timeStyle: 'short' })
    expect(readTime(at, 'en-US')).toBe(english.format(new Date(at)))
    expect(readTime('yesterday', 'en-US')).toBe('yesterday')
  })
})
