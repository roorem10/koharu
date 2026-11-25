import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { RightSidebar } from '@/components/editor/RightSidebar'
import { measureTabRow, tabsFit, type TabRow, useSidebarTabsFit } from '@/lib/booharu/sidebarTabs'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { listenToEvents, qcDesktop, settle } from './qc'

/** How the row of tabs `element` is in, or is, fits now. */
const fitOf = (element: Element) =>
  (element.closest('[role="tablist"]') ?? element).getAttribute('data-booharu-tabs')

/** The right pane's row of tabs as RightSidebar lays it out: 4 px of padding each side, and 4 px
 * between tabs, 2 px while they're tight; each tab 6 px of padding, 4 px while tight, and a 1 px
 * border each side, 4 px between its parts, and a 16 px icon, the size the tabs' base class gives
 * it, its style saying so while it's hidden too. */
const listStyle = (tight: boolean) => ({
  paddingLeft: '4px',
  paddingRight: '4px',
  borderLeftWidth: '0px',
  borderRightWidth: '0px',
  columnGap: tight ? '2px' : '4px',
})
const tabStyle = (tight: boolean) => ({
  paddingLeft: tight ? '4px' : '6px',
  paddingRight: tight ? '4px' : '6px',
  borderLeftWidth: '1px',
  borderRightWidth: '1px',
  columnGap: '4px',
})
const iconStyle = { width: '16px' }

/** jsdom's own styles, which `styled` starts from however many rows a test lays out. */
const jsdomStyle = window.getComputedStyle.bind(window)

/** Gives each element's style as `style` says, the rest of it, and the others', as jsdom does. */
function styled(style: (element: Element) => Record<string, string | undefined> | undefined) {
  vi.spyOn(window, 'getComputedStyle').mockImplementation((element, pseudo) => {
    const computed = jsdomStyle(element, pseudo)
    const given = style(element)
    if (!given) return computed
    return new Proxy(computed, {
      get(target, key) {
        if (typeof key === 'string' && key in given) return given[key]
        const value: unknown = Reflect.get(target, key, target)
        return typeof value === 'function' ? value.bind(target) : value
      },
    })
  })
}

/** Whether `element`'s tab is held from shrinking, as while it's measured. */
const held = (element: Element) =>
  (element.closest('[role="tab"]') as HTMLElement | null)?.style.flexShrink === '0'

const box = (width: number) => ({ width }) as DOMRect

/** A row `width` wide of tabs whose labels are `labels` wide whole, cut to 10 px while the tabs
 * shrink and 1 px while they show their icons alone, QC's with a badge `badge` wide. */
function tabRow(width: number, labels: number[], badge?: number): HTMLElement {
  const list = document.createElement('div')
  list.setAttribute('role', 'tablist')
  list.getBoundingClientRect = () => box(width)
  labels.forEach((labelWidth, index) => {
    const tab = document.createElement('button')
    tab.setAttribute('role', 'tab')
    tab.append(document.createElementNS('http://www.w3.org/2000/svg', 'svg'))
    const label = document.createElement('span')
    label.getBoundingClientRect = () =>
      box(fitOf(label) === 'icons' ? 1 : held(label) ? labelWidth : 10)
    tab.append(label)
    if (badge !== undefined && index === labels.length - 1) {
      const count = document.createElement('span')
      count.getBoundingClientRect = () => box(badge)
      tab.append(count)
    }
    list.append(tab)
  })
  styled((element) => {
    const tight = fitOf(element) === 'tight'
    if (element === list) return listStyle(tight)
    if (element.getAttribute('role') === 'tab') return tabStyle(tight)
    if (element instanceof SVGElement) return iconStyle
  })
  return list
}

/** Lays out the rows of tabs the hook measures `width` wide: a label or a count 6 px a character
 * whole, and 1 px while its tab shrinks or shows its icon alone; each tab 6 px of padding each side,
 * 4 px tight, and 4 px between its parts; the tabs 4 px apart, 2 px tight; and 12 px icons. */
function laidOut(width: number) {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute('role') === 'tablist' ? width : 0
  })
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: HTMLElement,
  ) {
    if (this.getAttribute('role') === 'tablist') return box(width)
    if (this.tagName === 'SPAN' && fitOf(this) !== 'icons' && held(this))
      return box((this.textContent?.length ?? 0) * 6)
    return box(this.tagName === 'SPAN' ? 1 : 0)
  })
  styled((element) => {
    const tight = fitOf(element) === 'tight'
    if (element.getAttribute('role') === 'tablist') return { columnGap: tight ? '2px' : '4px' }
    if (element.getAttribute('role') === 'tab')
      return {
        paddingLeft: tight ? '4px' : '6px',
        paddingRight: tight ? '4px' : '6px',
        columnGap: '4px',
      }
    if (element instanceof SVGElement) return { width: '12px' }
  })
}

/** How the row of tabs on screen fits now. */
const shownFit = () => screen.getByRole('tablist').getAttribute('data-booharu-tabs')

/** The right pane, with no project open, its row of tabs `width` wide, laid out as `laidOut` lays
 * rows out, and its tooltips showing at once, as the app shows them. */
function pane(width: number) {
  listenToEvents()
  qcDesktop(null)
  laidOut(width)
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={0}>
        <RightSidebar />
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

/** The tooltip showing, if one is. */
const tooltip = () => document.querySelector('[data-slot="tooltip-content"]')

/** A row of tabs `available` wide whose labels are `labels` wide whole, laid out as RightSidebar
 * lays them out, the last QC's with its count at 12 on a 22.45 px badge. */
function row(available: number, labels: number[]): TabRow {
  return {
    available,
    gap: 4,
    tightGap: 2,
    tabs: labels.map((label, index) => {
      const badge = index === labels.length - 1 ? 4 + 22.45 : 0
      return { icon: 20, label, fixed: 14 + badge, tightFixed: 10 + badge }
    }),
  }
}

describe('Booharu right pane tabs', () => {
  it('fits the tabs whole, then without icons, then tight, then as icons alone, to a layout unit', () => {
    // Two tabs: 16 px icons, 4 px after each, labels of 30 and 40 px, 14 px of padding and borders
    // each, 10 px tight, and 4 px between them, 2 px tight: 142 px whole, 102 px without the icons
    // and 92 px tight.
    const two = (available: number): TabRow => ({
      available,
      gap: 4,
      tightGap: 2,
      tabs: [
        { icon: 20, label: 30, fixed: 14, tightFixed: 10 },
        { icon: 20, label: 40, fixed: 14, tightFixed: 10 },
      ],
    })
    expect(tabsFit(two(142))).toBe('whole')
    expect(tabsFit(two(142 - 1 / 64))).toBe('labels')
    expect(tabsFit(two(102))).toBe('labels')
    expect(tabsFit(two(101))).toBe('tight')
    expect(tabsFit(two(92))).toBe('tight')
    expect(tabsFit(two(91))).toBe('icons')
    expect(tabsFit({ available: 0, gap: 4, tightGap: 2, tabs: [] })).toBe('whole')
    // A tab without an icon takes no room for one.
    expect(
      tabsFit({
        ...two(122),
        tabs: [two(0).tabs[0]!, { icon: 0, label: 40, fixed: 14, tightFixed: 10 }],
      }),
    ).toBe('whole')
  })

  it('keeps every whole label readable at 340 and 280 px, as Chrome lays the row out', () => {
    // Each label's whole width as Chrome lays it out with the app's fonts, Layers, Story, Context,
    // Agent and QC. The pane's border leaves the row 331 px at 340 px and 271 px at 280 px.
    const labels = {
      'zh-CN': [20, 20, 30, 30, 14.14],
      'en-US': [31.8, 25.97, 37.75, 28.16, 14.14],
      'ja-JP': [39.7, 50, 59.7, 59.81, 14.14],
      'pt-BR': [44.78, 38.22, 43.77, 33.7, 14.14],
      'ru-RU': [25.45, 50.41, 46.44, 28.27, 14.14],
      'fr-FR': [38.67, 38, 43.3, 28.16, 14.14],
    }
    const fits = (available: number) =>
      Object.fromEntries(
        Object.entries(labels).map(([language, widths]) => [
          language,
          tabsFit(row(available, widths)),
        ]),
      )
    // English takes 350.27 px with the icons and 250.27 px without them; Japanese 335.8 px without
    // them and 307.8 px tight, more than 280 px leaves, where only the icons fit.
    expect(fits(331)).toEqual({
      'zh-CN': 'whole',
      'en-US': 'labels',
      'ja-JP': 'tight',
      'pt-BR': 'labels',
      'ru-RU': 'labels',
      'fr-FR': 'labels',
    })
    expect(fits(271)).toEqual({
      'zh-CN': 'labels',
      'en-US': 'labels',
      'ja-JP': 'icons',
      'pt-BR': 'tight',
      'ru-RU': 'tight',
      'fr-FR': 'tight',
    })
  })

  it('measures each tab whole and tight, held from shrinking, however the tabs fit now', () => {
    const list = tabRow(340, [37, 33, 52, 40, 18], 17)
    // Shown as icons alone, the labels screen readers' only: they're measured whole all the same.
    list.setAttribute('data-booharu-tabs', 'icons')
    const measured = measureTabRow(list)
    expect(measured).toEqual({
      available: 332,
      gap: 4,
      tightGap: 2,
      tabs: [
        { icon: 20, label: 37, fixed: 14, tightFixed: 10 },
        { icon: 20, label: 33, fixed: 14, tightFixed: 10 },
        { icon: 20, label: 52, fixed: 14, tightFixed: 10 },
        { icon: 20, label: 40, fixed: 14, tightFixed: 10 },
        { icon: 20, label: 18, fixed: 35, tightFixed: 31 },
      ],
    })
    // The tabs fit as they did and shrink again once measured.
    expect(list).toHaveAttribute('data-booharu-tabs', 'icons')
    for (const tab of list.children) expect((tab as HTMLElement).style.flexShrink).toBe('')
    // The labels (180), padding, borders and the badge (91) and the gaps between tabs (16): 287 px
    // in 332, with the icons (100) 387 px.
    expect(tabsFit(measured)).toBe('labels')
    const whole = tabRow(395, [37, 33, 52, 40, 18], 17)
    expect(tabsFit(measureTabRow(whole))).toBe('whole')
    expect(whole).not.toHaveAttribute('data-booharu-tabs')
  })

  it('tells how the tabs fit, measuring again as they change', async () => {
    laidOut(120)
    // Each tab keyed by its label, so a label changed is a tab made again.
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

    // Two labels of 10 letters (120): 138 px tight in 120, so the icons alone.
    const view = render(<Row labels={['Layersssss', 'Storyyyyyy']} />)
    expect(shownFit()).toBe('icons')
    // Of 8 letters (96), as in another language: 124 px without the icons, 114 px tight.
    view.rerender(<Row labels={['Layersss', 'Storyyyy']} />)
    await waitFor(() => expect(shownFit()).toBe('tight'))
    // Of 6 letters (72): 100 px without the icons, 132 px with them.
    view.rerender(<Row labels={['Layers', 'Storyy']} />)
    await waitFor(() => expect(shownFit()).toBe('labels'))
    // Of 4 letters (48): 108 px with the icons.
    view.rerender(<Row labels={['Laye', 'Stor']} />)
    await waitFor(() => expect(shownFit()).toBeNull())
  })

  it('measures again as a label or the count changes in place, as React changes them', async () => {
    laidOut(120)
    // The tabs keep their keys, as RightSidebar's do, so React changes their text in place: a
    // language switch or a new count reaches the hook as a change of text alone.
    function Row({ labels, count }: { labels: string[]; count: number }) {
      const fit = useSidebarTabsFit()
      return (
        <div role='tablist' {...fit}>
          {labels.map((label, index) => (
            <button key={index} role='tab'>
              <svg />
              <span>{label}</span>
              {index === labels.length - 1 && <span>{count}</span>}
            </button>
          ))}
        </div>
      )
    }

    // The labels and the count, 6 px a character, 9 of them in all: 118 px with the icons.
    const view = render(<Row labels={['Layers', 'QC']} count={9} />)
    expect(shownFit()).toBeNull()
    // Layers in another language, 14 in all: 116 px without the icons.
    view.rerender(<Row labels={['Layerssssss', 'QC']} count={9} />)
    await waitFor(() => expect(shownFit()).toBe('labels'))
    // The count gains a digit: 122 px without the icons, 112 px tight.
    view.rerender(<Row labels={['Layerssssss', 'QC']} count={10} />)
    await waitFor(() => expect(shownFit()).toBe('tight'))
    // Layers back in the first language: 92 px without the icons.
    view.rerender(<Row labels={['Layers', 'QC']} count={10} />)
    await waitFor(() => expect(shownFit()).toBe('labels'))
  })

  it('names each tab in a tooltip while the icons show alone, as the desktop draws no title', async () => {
    const user = userEvent.setup()
    // Layers, Story, Context and Agent, 23 letters: 176 px tight in 170.
    pane(170)
    expect(shownFit()).toBe('icons')
    const agent = screen.getByRole('tab', { name: 'Agent' })
    expect(agent).not.toHaveAttribute('title')
    await user.hover(agent)
    await waitFor(() => expect(tooltip()).toHaveTextContent(/^Agent$/))
    await user.click(agent)
    expect(agent).toHaveAttribute('aria-selected', 'true')
  })

  it('names no tab in a tooltip while its label shows, but the one F7 shows', async () => {
    const user = userEvent.setup()
    // 262 px with the icons in 270.
    pane(270)
    expect(shownFit()).toBeNull()
    await user.hover(screen.getByRole('tab', { name: 'Agent' }))
    await settle()
    expect(tooltip()).toBeNull()
  })

  it('names F7 beside the tab it shows, Story in front of Layers, then Layers', async () => {
    const user = userEvent.setup()
    pane(270)
    await user.hover(screen.getByRole('tab', { name: 'Story' }))
    await waitFor(() => expect(tooltip()).toHaveTextContent(/^StoryF7$/))
    await user.unhover(screen.getByRole('tab', { name: 'Story' }))
    await user.keyboard('{F7}')
    expect(screen.getByRole('tab', { name: 'Story' })).toHaveAttribute('aria-selected', 'true')
    await user.hover(screen.getByRole('tab', { name: 'Layers' }))
    await waitFor(() => expect(tooltip()).toHaveTextContent(/^LayersF7$/))
  })

  it('shows Layers with F7 from Context, Agent or QC, as Koharu shows it from Story', () => {
    pane(270)
    for (const from of ['context', 'agent', 'qc', 'story'] as const) {
      act(() => useKoharuStore.getState().showPanel(from))
      fireEvent.keyDown(document.body, { key: 'F7', code: 'F7' })
      expect(useKoharuStore.getState().panel, from).toBe('layers')
    }
  })

  it('switches with F7 from a text field, typing nothing there, as Koharu chose', () => {
    pane(270)
    const field = document.createElement('textarea')
    document.body.append(field)
    field.focus()
    // Prevented: the field takes no key.
    expect(fireEvent.keyDown(field, { key: 'F7', code: 'F7' })).toBe(false)
    expect(useKoharuStore.getState().panel).toBe('story')
    field.remove()
  })

  it('leaves F7 to a dialog, a menu or an open list, switching no tab behind it', () => {
    pane(270)
    for (const role of ['dialog', 'alertdialog', 'menu', 'listbox']) {
      const popup = document.createElement('div')
      popup.setAttribute('role', role)
      const inside = document.createElement(role === 'dialog' ? 'textarea' : 'button')
      popup.append(inside)
      document.body.append(popup)
      inside.focus()
      expect(fireEvent.keyDown(inside, { key: 'F7', code: 'F7' }), role).toBe(true)
      expect(useKoharuStore.getState().panel, role).toBe('layers')
      popup.remove()
    }
  })

  it('fits the tabs whole while the row isn’t laid out', () => {
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
    expect(screen.getByRole('tablist')).not.toHaveAttribute('data-booharu-tabs')
  })
})
