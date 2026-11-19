import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { iconsFit, measureTabRow, useSidebarTabsFit } from '@/lib/booharu/sidebarTabs'

/** The right pane's row of tabs as RightSidebar lays it out: 4 px of padding each side and between
 * tabs, and each tab 6 px of padding and a 1 px border each side, 4 px between its parts, and a
 * 16 px icon, the size the tabs' base class gives it, its style saying so while it's hidden too. */
const listStyle = {
  paddingLeft: '4px',
  paddingRight: '4px',
  borderLeftWidth: '0px',
  borderRightWidth: '0px',
  columnGap: '4px',
}
const tabStyle = {
  paddingLeft: '6px',
  paddingRight: '6px',
  borderLeftWidth: '1px',
  borderRightWidth: '1px',
  columnGap: '4px',
}
const iconStyle = { width: '16px' }

/** Gives each element's style as `style` says, the others' as jsdom does. */
function styled(style: (element: Element) => object | undefined) {
  const real = window.getComputedStyle.bind(window)
  vi.spyOn(window, 'getComputedStyle').mockImplementation(
    (element) => (style(element) as CSSStyleDeclaration | undefined) ?? real(element),
  )
}

/** Whether `element`'s tab is held from shrinking, as while it's measured. */
const held = (element: Element) =>
  (element.closest('[role="tab"]') as HTMLElement | null)?.style.flexShrink === '0'

const box = (width: number) => ({ width }) as DOMRect

/** A row `width` wide of tabs whose labels are `labels` wide whole, and cut to 10 px while the tabs
 * shrink, QC's with a badge `badge` wide. */
function tabRow(width: number, labels: number[], badge?: number): HTMLElement {
  const list = document.createElement('div')
  list.getBoundingClientRect = () => box(width)
  labels.forEach((labelWidth, index) => {
    const tab = document.createElement('button')
    tab.setAttribute('role', 'tab')
    tab.append(document.createElementNS('http://www.w3.org/2000/svg', 'svg'))
    const label = document.createElement('span')
    label.getBoundingClientRect = () => box(held(label) ? labelWidth : 10)
    tab.append(label)
    if (badge !== undefined && index === labels.length - 1) {
      const count = document.createElement('span')
      count.getBoundingClientRect = () => box(badge)
      tab.append(count)
    }
    list.append(tab)
  })
  styled((element) => {
    if (element === list) return listStyle
    if (element.getAttribute('role') === 'tab') return tabStyle
    if (element instanceof SVGElement) return iconStyle
  })
  return list
}

describe('Booharu right pane tabs', () => {
  it('fits the icons while every whole label fits beside them, to a layout unit', () => {
    // Two tabs: 16 px icons, 4 px after each, labels of 30 and 40 px, 14 px of padding and borders
    // each, and 4 px between them: 142 px.
    const row = (available: number) => ({
      available,
      gap: 4,
      tabs: [
        { icon: 20, label: 30, fixed: 14 },
        { icon: 20, label: 40, fixed: 14 },
      ],
    })
    expect(iconsFit(row(142))).toBe(true)
    expect(iconsFit(row(142 - 1 / 64))).toBe(false)
    expect(iconsFit({ available: 0, gap: 4, tabs: [] })).toBe(true)
    // A tab without an icon takes no room for one.
    expect(
      iconsFit({ ...row(122), tabs: [row(0).tabs[0]!, { icon: 0, label: 40, fixed: 14 }] }),
    ).toBe(true)

    // English in the pane at its default 340 px once the QC tab shows, as Chrome lays it out: the
    // labels of 31.8, 26, 37.8, 28.2 and 14.1 px and QC's 22.45 px badge take 350.35 px with the
    // icons, more than the 332 px the row gives; 250.35 px without them.
    const english = {
      available: 332,
      gap: 4,
      tabs: [31.8, 26, 37.8, 28.2, 14.1].map((label, index) => ({
        icon: 20,
        label,
        fixed: index === 4 ? 14 + 4 + 22.45 : 14,
      })),
    }
    expect(iconsFit(english)).toBe(false)
    expect(iconsFit({ ...english, tabs: english.tabs.map((tab) => ({ ...tab, icon: 0 })) })).toBe(
      true,
    )
  })

  it('measures each tab’s whole label, held from shrinking, and its badge as what doesn’t shrink', () => {
    const list = tabRow(340, [37, 33, 52, 40, 18], 17)
    const row = measureTabRow(list)
    expect(row).toEqual({
      available: 332,
      gap: 4,
      tabs: [
        { icon: 20, label: 37, fixed: 14 },
        { icon: 20, label: 33, fixed: 14 },
        { icon: 20, label: 52, fixed: 14 },
        { icon: 20, label: 40, fixed: 14 },
        { icon: 20, label: 18, fixed: 35 },
      ],
    })
    // The tabs shrink again once measured.
    for (const tab of list.children) expect((tab as HTMLElement).style.flexShrink).toBe('')
    // 5 icons with the gap after each (100), the labels (180), padding, borders and the badge (91)
    // and the gaps between tabs (16): 387 px in 332.
    expect(iconsFit(row)).toBe(false)
    expect(iconsFit(measureTabRow(tabRow(395, [37, 33, 52, 40, 18], 17)))).toBe(true)
  })

  it('hides the icons once the labels no longer fit, and measures again as they change', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.getAttribute('role') === 'tablist' ? 120 : 0
    })
    // A label is 6 px a letter whole, and cut to 1 px while its tab shrinks.
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: HTMLElement,
    ) {
      if (this.getAttribute('role') === 'tablist') return box(120)
      if (this.tagName === 'SPAN') return box(held(this) ? (this.textContent?.length ?? 0) * 6 : 1)
      return box(0)
    })
    styled((element) => (element instanceof SVGElement ? { width: '12px' } : undefined))

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
