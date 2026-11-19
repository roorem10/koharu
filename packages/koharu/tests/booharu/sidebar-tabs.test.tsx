import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { iconsFit, measureTabRow, useSidebarTabsFit } from '@/lib/booharu/sidebarTabs'

/** The right pane's row of tabs as RightSidebar lays it out: 4 px of padding each side and between
 * tabs, and each tab 6 px of padding and a 1 px border each side, 4 px between its parts. */
const listStyle = { paddingLeft: '4px', paddingRight: '4px', columnGap: '4px' }
const tabStyle = {
  paddingLeft: '6px',
  paddingRight: '6px',
  borderLeftWidth: '1px',
  borderRightWidth: '1px',
  columnGap: '4px',
}

/** A row `width` wide of tabs whose labels are `labels` wide, QC's with a badge `badge` wide. */
function tabRow(width: number, labels: number[], badge?: number): HTMLElement {
  const list = document.createElement('div')
  Object.defineProperty(list, 'clientWidth', { value: width })
  labels.forEach((labelWidth, index) => {
    const tab = document.createElement('button')
    tab.setAttribute('role', 'tab')
    tab.append(document.createElementNS('http://www.w3.org/2000/svg', 'svg'))
    const label = document.createElement('span')
    // Cut short, the label's scroll width is still its whole width.
    Object.defineProperty(label, 'scrollWidth', { value: labelWidth })
    tab.append(label)
    if (badge !== undefined && index === labels.length - 1) {
      const count = document.createElement('span')
      count.getBoundingClientRect = () => ({ width: badge }) as DOMRect
      tab.append(count)
    }
    list.append(tab)
  })
  const real = window.getComputedStyle.bind(window)
  vi.spyOn(window, 'getComputedStyle').mockImplementation((element) => {
    if (element === list) return listStyle as CSSStyleDeclaration
    if (element.getAttribute('role') === 'tab') return tabStyle as CSSStyleDeclaration
    return real(element)
  })
  return list
}

describe('Booharu right pane tabs', () => {
  it('fits the icons while every whole label fits beside them, half a pixel of rounding allowed', () => {
    // Two tabs: 12 px icons, 4 px after each, labels of 30 and 40 px, 14 px of padding and borders
    // each, and 4 px between them: 134 px.
    const row = (available: number) => ({
      available,
      gap: 4,
      icon: 16,
      tabs: [
        { label: 30, fixed: 14 },
        { label: 40, fixed: 14 },
      ],
    })
    expect(iconsFit(row(134))).toBe(true)
    expect(iconsFit(row(133.5))).toBe(true)
    expect(iconsFit(row(133.4))).toBe(false)
    expect(iconsFit({ available: 0, gap: 4, icon: 16, tabs: [] })).toBe(true)
  })

  it('measures each tab’s whole label, and its badge as part of what doesn’t shrink', () => {
    const row = measureTabRow(tabRow(340, [37, 33, 52, 40, 18], 17))
    expect(row).toEqual({
      available: 332,
      gap: 4,
      icon: 16,
      tabs: [
        { label: 37, fixed: 14 },
        { label: 33, fixed: 14 },
        { label: 52, fixed: 14 },
        { label: 40, fixed: 14 },
        { label: 18, fixed: 35 },
      ],
    })
    // 5 icons with the gap after each (80), the labels (180), padding, borders and the badge (91)
    // and the gaps between tabs (16): 367 px in 332.
    expect(iconsFit(row)).toBe(false)
    expect(iconsFit(measureTabRow(tabRow(375, [37, 33, 52, 40, 18], 17)))).toBe(true)
  })

  it('hides the icons once the labels no longer fit, and measures again as they change', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.getAttribute('role') === 'tablist' ? 120 : 0
    })
    vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return (this.textContent?.length ?? 0) * 6
    })

    function Row({ labels }: { labels: string[] }) {
      const fit = useSidebarTabsFit()
      return (
        <div role='tablist' {...fit}>
          {labels.map((label) => (
            <button key={label} role='tab'>
              <svg />
              <span>{label}</span>
            </button>
          ))}
        </div>
      )
    }

    // With no padding or gaps here: 2 icons (24) and 2 labels of 10 letters at 6 px (120), 144 px
    // in 120.
    const view = render(<Row labels={['Layersssss', 'Storyyyyyy']} />)
    expect(screen.getByRole('tablist')).toHaveAttribute('data-booharu-icons', 'hidden')
    // Shorter labels, as in another language: 2 icons and 2 labels of 4 letters, 72 px.
    view.rerender(<Row labels={['Laye', 'Stor']} />)
    await waitFor(() =>
      expect(screen.getByRole('tablist')).not.toHaveAttribute('data-booharu-icons'),
    )
  })

  it('keeps the icons while the row isn’t laid out', () => {
    function Row() {
      const fit = useSidebarTabsFit()
      return (
        <div role='tablist' {...fit}>
          <button role='tab'>
            <svg />
            <span>A label far too long for a row with no width at all</span>
          </button>
        </div>
      )
    }
    render(<Row />)
    expect(screen.getByRole('tablist')).not.toHaveAttribute('data-booharu-icons')
  })
})
