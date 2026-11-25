import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { typedByHand } from '@/lib/booharu/attrs'
import type { Layer, Value } from '@koharu/ipc/protocol'

/** The hand-edit records booharu-core's attrs.rs tests hold `Auto::of` to as well. */
const records = JSON.parse(
  readFileSync(join(import.meta.dirname, 'auto-records.json'), 'utf8'),
) as {
  cases: { case: string; auto: Value; typed: boolean }[]
  stricter: { case: string; auto: Value }[]
}

/** A block with no translation, holding `auto` as its hand-edit record, or none. */
function holding(auto?: Value): Layer {
  return {
    id: 'block',
    name: 'block',
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: auto === undefined ? {} : { booharu: { auto } },
    kind: 'block',
    source: { content: [{ insert: 'やあ' }] },
  }
}

describe('the marker of a translation typed by hand', () => {
  it.each(records.cases)('reads $case as booharu-core reads it', ({ auto, typed }) => {
    expect(typedByHand(holding(auto))).toBe(typed)
  })

  it('reads none from a block with no record, or a record in another namespace', () => {
    expect(typedByHand(holding())).toBe(false)
    expect(typedByHand({ ...holding(), attributes: { c2pa: { auto: { typed: true } } } })).toBe(
      false,
    )
  })

  it.each(records.stricter)(
    'reads none from $case, which booharu-core reads and Booharu never writes',
    ({ auto }) => {
      expect(typedByHand(holding(auto))).toBe(false)
    },
  )
})
