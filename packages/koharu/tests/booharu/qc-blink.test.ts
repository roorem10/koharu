import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

/** A rule of app/booharu.css with the cascade layers it sits in, outermost first. */
interface Placed {
  rule: CSSRule
  layers: string[]
}

/** Every rule of app/booharu.css, nested ones included, in the order the browser reads them. */
function booharuRules(): Placed[] {
  const style = document.createElement('style')
  style.textContent = readFileSync(join(import.meta.dirname, '../../app/booharu.css'), 'utf8')
  document.head.append(style)
  const walk = (rules: CSSRuleList, layers: string[]): Placed[] =>
    Array.from(rules).flatMap((rule) => {
      const inner = rule instanceof CSSLayerBlockRule ? [...layers, rule.name] : layers
      const nested = 'cssRules' in rule ? walk(rule.cssRules as CSSRuleList, inner) : []
      return [{ rule, layers }, ...nested]
    })
  const rules = walk(style.sheet!.cssRules, [])
  style.remove()
  return rules
}

/** The style rules for exactly `selector`. */
function styleRules(rules: Placed[], selector: string) {
  return rules.filter(
    (placed): placed is Placed & { rule: CSSStyleRule } =>
      placed.rule instanceof CSSStyleRule && placed.rule.selectorText === selector,
  )
}

/** The seconds a CSS time such as `1.6s` or `800ms` stands for. */
function seconds(time: string): number {
  const match = /^([\d.]+)(m?s)$/.exec(time.trim())
  if (!match) throw new Error(`Not a time: ${time}`)
  return Number(match[1]) / (match[2] === 'ms' ? 1000 : 1)
}

describe('Booharu stylesheet', () => {
  it('keeps the QC tag blinking with reduced motion, more slowly, from inside layer base', () => {
    const rules = booharuRules()
    const blinks = styleRules(rules, '.booharu-qc-blink')
    // Outside layer base a rule loses to the still interface's rules or to the blink's own.
    expect(blinks.map(({ layers }) => layers)).toEqual([['base'], ['base']])

    const [blink, slower] = blinks
    const reduced = slower!.rule.parentRule
    expect(blink!.rule.parentRule).toBeInstanceOf(CSSLayerBlockRule)
    expect(reduced).toBeInstanceOf(CSSMediaRule)
    expect((reduced as CSSMediaRule).media.mediaText).toBe('(prefers-reduced-motion: reduce)')
    // In the same layer, the slower rule wins only by coming after.
    expect(rules.findIndex(({ rule }) => rule === reduced)).toBeGreaterThan(rules.indexOf(blink!))

    const normal = blink!.rule.style
    expect(normal.getPropertyValue('animation-name')).toBe('booharu-qc-blink')
    expect(normal.getPropertyValue('animation-iteration-count')).toBe('infinite')
    expect(normal.getPropertyPriority('animation-duration')).toBe('important')
    const slow = slower!.rule.style
    expect(slow.getPropertyPriority('animation-duration')).toBe('important')
    expect(seconds(slow.getPropertyValue('animation-duration'))).toBeGreaterThan(
      seconds(normal.getPropertyValue('animation-duration')),
    )

    // It dims and comes back.
    const frames = rules.find(
      ({ rule }) => rule instanceof CSSKeyframesRule && rule.name === 'booharu-qc-blink',
    )?.rule as CSSKeyframesRule | undefined
    expect(
      Array.from(frames?.cssRules ?? [], (frame) => [
        (frame as CSSKeyframeRule).keyText,
        (frame as CSSKeyframeRule).style.getPropertyValue('opacity'),
      ]),
    ).toEqual([
      ['0%, 100%', '1'],
      ['50%', '0.35'],
    ])
  })

  it('fills the QC tag with the warning yellow over the outline button’s own colours', () => {
    const rules = booharuRules()
    const [token] = styleRules(rules, ':root')
    expect(token?.rule.style.getPropertyValue('--booharu-warning')).toBe('#dba54a')

    // The outline button's colours are utilities, in a later layer than any rule here, so only an
    // !important declaration wins over them.
    const [tag] = styleRules(rules, '.booharu-qc-tag')
    for (const property of ['background-color', 'border-color', 'color']) {
      expect(tag?.rule.style.getPropertyPriority(property), property).toBe('important')
    }
    expect(tag?.rule.style.getPropertyValue('background-color')).toBe('var(--booharu-warning)')
  })
})
