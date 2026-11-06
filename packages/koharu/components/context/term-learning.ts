export type KnownTerm = { source: string; target: string }
export type LearnedTerm = { source: string; before: string; after: string }

const word = /[\p{L}\p{M}\p{N}_]/u
const cjk = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u
const spelling = /^[\p{L}\p{M}]+(?:[ '\u2019-][\p{L}\p{M}]+)*$/u

function occurrences(text: string, term: string): number[] {
  const result: number[] = []
  for (let index = text.indexOf(term); index >= 0; index = text.indexOf(term, index + 1)) {
    const left = Array.from(text.slice(0, index)).at(-1) ?? ''
    const right = Array.from(text.slice(index + term.length))[0] ?? ''
    if (cjk.test(term) || (!word.test(left) && !word.test(right))) result.push(index)
  }
  return result
}

function distance(left: string, right: string): number {
  const a = Array.from(left.toLocaleLowerCase()),
    b = Array.from(right.toLocaleLowerCase())
  let previous = b.map((_, i) => i + 1)
  previous.unshift(0)
  for (let i = 0; i < a.length; i++) {
    const next = [i + 1]
    for (let j = 0; j < b.length; j++) {
      next.push(Math.min(next[j] + 1, previous[j + 1] + 1, previous[j] + (a[i] === b[j] ? 0 : 1)))
    }
    previous = next
  }
  return previous[b.length]
}

/** Only a single, source-grounded spelling change qualifies; sentence rewrites are ambiguous. */
export function detectLearnedTerm(
  source: string,
  before: string,
  after: string,
  known: readonly KnownTerm[],
): LearnedTerm | null {
  if (!before || before === after) return null
  const candidates = new Map<string, LearnedTerm>()
  for (const entry of known) {
    const original = entry.source.trim(),
      target = entry.target.trim()
    if (!original || Array.from(target).length < 2 || !occurrences(source, original).length)
      continue
    const positions = occurrences(before, target)
    if (positions.length !== 1) continue
    const index = positions[0]
    const prefix = before.slice(0, index),
      suffix = before.slice(index + target.length)
    if (
      !after.startsWith(prefix) ||
      !after.endsWith(suffix) ||
      after.length <= prefix.length + suffix.length
    )
      continue
    const replacement = after.slice(prefix.length, after.length - suffix.length)
    const length = Array.from(replacement).length
    if (
      replacement === target ||
      length < 2 ||
      length > 64 ||
      !spelling.test(replacement) ||
      replacement.split(' ').length !== target.split(' ').length ||
      distance(target, replacement) >
        Math.max(1, Math.floor(Math.max(Array.from(target).length, length) / 2))
    )
      continue
    const candidate = { source: original, before: target, after: replacement }
    candidates.set(JSON.stringify(candidate), candidate)
  }
  return candidates.size === 1 ? [...candidates.values()][0] : null
}
