import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxMenu } from '@/components/booharu/boxes/BoxMenu'
import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { closeBoxMenu, useBoxMenu } from '@/lib/booharu/boxMenu'
import { useFx } from '@/lib/booharu/fx'
import { CANVAS_PRESS } from '@/lib/booharu/wordSelection'
import { pageKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'

import { boxesPage, desktop, openProject } from './boxes'
import { listenToEvents, tell } from './qc'

/** The desktop answering every command and telling the events `tell` gives, with the boxes page
 * open on the canvas. */
function open() {
  const calls = desktop()
  listenToEvents()
  openProject(boxesPage())
  return calls
}

function mount(ui: ReactNode = <BoxMenu />) {
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** The box menu over the canvas, whose keys it takes from the page. jsdom has no 2D canvas; the
 * workspace then sends black for its theme colours. */
function mountOverCanvas() {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
  mount(
    <>
      <CanvasWorkspace />
      <BoxMenu />
    </>,
  )
}

/** A right-click on `layer` at `x`, `y` as the desktop tells of it, the selection holding
 * `selected` by then; resolves to the menu it opens. */
async function rightClick(layer: string, x = 120, y = 80, selected: string[] = [layer]) {
  act(() => useKoharuStore.setState({ selectedLayers: selected }))
  tell('booharuBoxMenu', { page: 'page', layer, x, y })
  return screen.findByRole('menu', { name: 'Box actions' })
}

/** The menu's items, by name, each with whether it's enabled. */
function items(menu: HTMLElement): [string, boolean][] {
  return within(menu)
    .getAllByRole('menuitem')
    .map((item) => [item.textContent ?? '', item.getAttribute('aria-disabled') !== 'true'])
}

/** Where the menu's positioner placed it. */
const placed = (menu: HTMLElement) => menu.parentElement?.style.transform

async function closed() {
  await waitFor(() => expect(screen.queryByRole('menu')).toBeNull())
  expect(useBoxMenu.getState().shown).toBeNull()
}

beforeEach(() => closeBoxMenu())

describe('Booharu box menu (S12-l)', () => {
  it('opens where the right-click pressed, listing the box actions, FX… first, and takes the focus', async () => {
    open()
    mount()
    expect(screen.queryByRole('menu')).toBeNull()

    const menu = await rightClick('second', 120, 80)

    expect(items(menu)).toEqual([
      ['FX…', true],
      ['Read again', true],
      ['Translate again', true],
      ['Clean again', true],
      ['Join with balloon', true],
      ['Split', true],
      ['Add subtitle', false],
      ['Change category…', true],
      ['Reset to lettering default', false],
      ['Explain this line', true],
      ['Box details', true],
    ])
    await waitFor(() => expect(placed(menu)).toBe('translate(120px, 80px)'))
    await waitFor(() => expect(menu.contains(document.activeElement)).toBe(true))
  })

  it('runs an item on the selection, then closes', async () => {
    const { run } = open()
    mount()
    const menu = await rightClick('second')

    fireEvent.click(within(menu).getByRole('menuitem', { name: 'Clean again' }))

    await waitFor(() => expect(run).toHaveBeenCalledWith({ blocks: ['second'] }, ['inpaint']))
    await closed()
    expect(useKoharuStore.getState().selectedLayers).toEqual(['second'])
    // The item pressed on the page, the keys stay with it.
    expect(commands.booharuCanvasKeys).not.toHaveBeenCalled()
  })

  it('opens Text effects on the selection with FX…, switching to the Select tool', async () => {
    open()
    act(() => useKoharuStore.setState({ tool: 'draw' }))
    mount()
    const menu = await rightClick('second', 120, 80, ['first', 'second'])

    fireEvent.click(within(menu).getByRole('menuitem', { name: 'FX…' }))

    await waitFor(() => expect(useFx.getState().open).toBe(true))
    expect(useKoharuStore.getState().tool).toBe('select')
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first', 'second'])
    await closed()
  })

  it('keeps its keys from the canvas: Delete deletes nothing, and Escape closes it, keeps the selection and gives the canvas its keys back', async () => {
    open()
    mountOverCanvas()
    const menu = await rightClick('second')
    await waitFor(() => expect(menu.contains(document.activeElement)).toBe(true))

    // Each key as a browser names it, its code included, which Koharu's Delete hotkey matches by.
    for (const [key, code] of [
      ['Delete', 'Delete'],
      ['Backspace', 'Backspace'],
      ['b', 'KeyB'],
    ])
      fireEvent.keyDown(document.activeElement!, { key, code })

    expect(commands.edit).not.toHaveBeenCalled()
    expect(useKoharuStore.getState().tool).toBe('select')
    expect(screen.getByRole('menu', { name: 'Box actions' })).toBe(menu)
    expect(commands.booharuCanvasKeys).not.toHaveBeenCalled()

    fireEvent.keyDown(document.activeElement!, { key: 'Escape', code: 'Escape' })

    await closed()
    expect(useKoharuStore.getState().selectedLayers).toEqual(['second'])
    // The desktop gives the canvas the keys again, so Delete takes the anchors Direct Select picked.
    expect(commands.booharuCanvasKeys).toHaveBeenCalledTimes(1)
    // Closed, it leaves the keys to the canvas again: Delete deletes the selected box.
    fireEvent.keyDown(document.body, { key: 'Delete', code: 'Delete' })
    await waitFor(() =>
      expect(commands.edit).toHaveBeenCalledWith({
        label: 'delete_layers',
        ops: [{ op: 'remove_layer', layer: 'second' }],
      }),
    )
  })

  const page = boxesPage()
  // Each closer, and whether closing gives the canvas back the keys the menu took: all but a press
  // on the canvas, which gave it them already.
  const closers: [string, () => void, boolean][] = [
    ['a press on the canvas', () => window.dispatchEvent(new Event(CANVAS_PRESS)), false],
    [
      'the camera moving',
      () => useKoharuStore.setState({ camera: { zoom: 1.5, x: 0, y: 0 } }),
      true,
    ],
    ['the window losing the focus', () => window.dispatchEvent(new Event('blur')), true],
    [
      'another page showing',
      () => queryClient.setQueryData(pageKey, { ...page, id: 'other' }),
      true,
    ],
    [
      'its box leaving the page',
      () =>
        queryClient.setQueryData(pageKey, {
          ...page,
          layers: page.layers.filter(({ id }) => id !== 'free'),
        }),
      true,
    ],
    [
      'another project opening',
      () =>
        queryClient.setQueryData<ProjectView>(
          projectKey,
          (project) => project && { ...project, name: 'Other' },
        ),
      true,
    ],
  ]

  it.each(closers)('closes on %s', async (_, close, keys) => {
    open()
    mount()
    await rightClick('free')

    act(close)

    await closed()
    expect(commands.booharuCanvasKeys).toHaveBeenCalledTimes(keys ? 1 : 0)
  })

  it('closes on a press outside it', async () => {
    open()
    mount(
      <>
        <button type='button'>Outside</button>
        <BoxMenu />
      </>,
    )
    await rightClick('free')

    await userEvent.setup().click(screen.getByRole('button', { name: 'Outside' }))

    await closed()
    // The press landed on the page, which keeps the keys.
    expect(commands.booharuCanvasKeys).not.toHaveBeenCalled()
  })

  it('stays open while its box stays on the page the canvas shows', async () => {
    open()
    mount()
    const menu = await rightClick('free')

    act(() => {
      useKoharuStore.setState({ selectionBounds: { x: 0, y: 0, width: 40, height: 80 } })
      // Another box deleted, its page read again.
      queryClient.setQueryData(pageKey, {
        ...page,
        layers: page.layers.filter(({ id }) => id !== 'locked'),
      })
    })

    expect(screen.getByRole('menu', { name: 'Box actions' })).toBe(menu)
    expect(useBoxMenu.getState().shown).not.toBeNull()
  })

  it('listens for a press on the canvas only while it shows', async () => {
    open()
    const added = vi.spyOn(window, 'addEventListener')
    const removed = vi.spyOn(window, 'removeEventListener')
    const presses = (spy: typeof added) =>
      spy.mock.calls.filter(([type]) => type === CANVAS_PRESS).length
    mount()
    expect(presses(added)).toBe(0)

    await rightClick('free')
    expect(presses(added)).toBe(1)

    act(() => window.dispatchEvent(new Event(CANVAS_PRESS)))
    await closed()
    expect(presses(removed)).toBe(1)
  })

  it('opens afresh at each right-click, at its point', async () => {
    open()
    mount()
    const first = await rightClick('second', 120, 80)

    const again = await rightClick('second', 120, 80)
    expect(first.isConnected).toBe(false)
    const elsewhere = await rightClick('free', 300, 200)

    expect(again.isConnected).toBe(false)
    expect(screen.getAllByRole('menu')).toEqual([elsewhere])
    await waitFor(() => expect(placed(elsewhere)).toBe('translate(300px, 200px)'))
    expect(items(elsewhere)[0]).toEqual(['FX…', true])
  })

  it('opens nothing for a page the canvas no longer shows', async () => {
    open()
    mount()
    act(() => useKoharuStore.setState({ selectedLayers: ['free'] }))

    tell('booharuBoxMenu', { page: 'other', layer: 'free', x: 120, y: 80 })

    await closed()
    // The right-click took the keys for a menu that never shows, so the canvas gets them back.
    expect(commands.booharuCanvasKeys).toHaveBeenCalledTimes(1)
  })
})
