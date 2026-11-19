/** The locales to format for in `language`: Wu's, which Intl may lack, falling back to Mandarin's,
 * as the rest of Booharu formats lists in Wu. */
function locales(language: string): string[] {
  return language === 'wuu' ? ['wuu', 'zh-CN'] : [language]
}

/** `pages`, pages counted from 1, as runs: each page once, in order, and pages in a row as one run
 * "first–last". */
export function pageRuns(pages: readonly number[]): string[] {
  const sorted = [...new Set(pages)].sort((a, b) => a - b)
  const runs: string[] = []
  for (let at = 0; at < sorted.length;) {
    let end = at
    while (end + 1 < sorted.length && sorted[end + 1] === sorted[end]! + 1) end += 1
    runs.push(end > at ? `${sorted[at]}–${sorted[end]}` : `${sorted[at]}`)
    at = end + 1
  }
  return runs
}

/** `pages` as `language` lists them, in runs, such as "1–3, 5" in English. */
export function joinPages(pages: readonly number[], language: string): string {
  return new Intl.ListFormat(locales(language), { type: 'conjunction', style: 'narrow' }).format(
    pageRuns(pages),
  )
}

/** `readAt`, an RFC 3339 time, as `language` writes a date and time, in the user's time zone; as
 * it is when it doesn't parse. */
export function readTime(readAt: string, language: string): string {
  const time = new Date(readAt)
  if (Number.isNaN(time.getTime())) return readAt
  return new Intl.DateTimeFormat(locales(language), {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(time)
}
