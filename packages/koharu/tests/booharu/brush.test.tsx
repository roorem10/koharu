import { act, cleanup, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  brushFeel,
  restoredSlots,
  setBrushSlot,
  steppedDiameter,
  useBooharuBrush,
  useBrushSlots,
} from '@/lib/booharu/brush'
import { keyActions } from '@/lib/booharu/keys'
import { pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore, type CanvasTool } from '@/lib/store'
import { commands, type Page } from '@koharu/ipc/protocol'

/** A page whose size slider goes up to 128 px, as any page up to 2048 px wide. */
const page: Page = { id: 'page', size: { width: 1000, height: 1400 }, background: null, layers: [] }

/** A key pressed on `target`, the page itself unless named, as the desktop's page gets it: its
 * character, its key's place, and its modifiers, AltGraph among them where `altGraph` says so,
 * which jsdom can't set. */
function press(
  init: KeyboardEventInit & { altGraph?: boolean },
  target: Element = document.body,
): KeyboardEvent {
  const { altGraph = false, ...rest } = init
  const event = new KeyboardEvent('keydown', { ...rest, bubbles: true, cancelable: true })
  const modifier = event.getModifierState.bind(event)
  Object.defineProperty(event, 'getModifierState', {
    value: (key: string) => (key === 'AltGraph' ? altGraph : modifier(key)),
  })
  target.dispatchEvent(event)
  return event
}

/** The size of Koharu's brush, the one the canvas paints with. */
const diameter = () => useKoharuStore.getState().brush.diameter

/** `init` pressed once from a brush of 48 px: the size it leaves, and whether it was prevented. */
function step(init: KeyboardEventInit & { altGraph?: boolean }, target?: Element) {
  useKoharuStore.getState().setBrush({ ...useKoharuStore.getState().brush, diameter: 48 })
  const event = press(init, target)
  return { size: diameter(), prevented: event.defaultPrevented }
}

const smaller = { size: 43, prevented: true }
const larger = { size: 53, prevented: true }
const untouched = { size: 48, prevented: false }

/** A turn of the event loop, by which a command sent as a mutation has gone. */
function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

/** Picks `tool`, as the tool rail does; the hook follows it as it renders again. */
function pick(tool: CanvasTool): void {
  act(() => useKoharuStore.getState().setTool(tool))
}

/** Koharu's Read mode on or off. */
function read(reading: boolean): void {
  act(() => useKoharuStore.setState({ reading }))
}

function input(type: string): HTMLInputElement {
  const element = document.createElement('input')
  element.type = type
  document.body.append(element)
  return element
}

let setBrush: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  setBrush = vi.spyOn(commands, 'booharuSetBrush').mockResolvedValue(null)
  queryClient.setQueryData(pageKey, page)
})

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe('Booharu brush steps', () => {
  it('steps by a tenth of the size, at least 2 px', () => {
    expect(steppedDiameter(48, 1, page.size)).toBe(53)
    expect(steppedDiameter(48, -1, page.size)).toBe(43)
    expect(steppedDiameter(10, 1, page.size)).toBe(12)
    expect(steppedDiameter(10, -1, page.size)).toBe(8)
  })

  it("never goes below 1 px, nor past the page's largest slider size", () => {
    expect(steppedDiameter(2, -1, page.size)).toBe(1)
    expect(steppedDiameter(1, -1, page.size)).toBe(1)
    expect(steppedDiameter(125, 1, page.size)).toBe(128)
    expect(steppedDiameter(128, 1, page.size)).toBe(128)
    // A wider page's slider goes further.
    expect(steppedDiameter(250, 1, { width: 4096, height: 6000 })).toBe(256)
  })

  it('keeps a size past the slider, as Alt+wheel sets one, on ], and steps it down on [', () => {
    expect(steppedDiameter(300, 1, page.size)).toBe(300)
    expect(steppedDiameter(300, -1, page.size)).toBe(270)
  })

  it('names [ and ] as the two key actions', () => {
    expect(keyActions.brushSmaller).toEqual({ key: '[' })
    expect(keyActions.brushLarger).toEqual({ key: ']' })
  })
})

describe('Booharu [ and ]', () => {
  it('steps once for each bracket, typed where a US keyboard has it', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    expect(step({ key: '[', code: 'BracketLeft' })).toEqual(smaller)
    expect(step({ key: ']', code: 'BracketRight' })).toEqual(larger)
  })

  it('steps once for a bracket typed with AltGr, with AltGraph on or with Ctrl and Alt', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    // German keyboards: AltGr+8 types [ and AltGr+9 types ].
    expect(step({ key: '[', code: 'Digit8', altGraph: true })).toEqual(smaller)
    expect(step({ key: ']', code: 'Digit9', altGraph: true })).toEqual(larger)
    expect(step({ key: '[', code: 'Digit8', ctrlKey: true, altKey: true })).toEqual(smaller)
    expect(step({ key: ']', code: 'Digit9', ctrlKey: true, altKey: true })).toEqual(larger)
  })

  it('steps once by the place of a key that types no bracket there', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    // A browser reads a German keyboard's Ü as ü; the desktop reads it as ß and its + as a dead
    // key, on Windows.
    expect(step({ key: 'ü', code: 'BracketLeft' })).toEqual(smaller)
    expect(step({ key: 'ß', code: 'BracketLeft' })).toEqual(smaller)
    expect(step({ key: 'Dead', code: 'BracketRight' })).toEqual(larger)
  })

  it('refuses Shift, Ctrl alone, Alt alone and Meta', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    for (const modifier of ['shiftKey', 'ctrlKey', 'altKey', 'metaKey'] as const) {
      expect(step({ key: '[', code: 'BracketLeft', [modifier]: true }), modifier).toEqual(untouched)
      expect(step({ key: ']', code: 'BracketRight', [modifier]: true }), modifier).toEqual(
        untouched,
      )
    }
    expect(step({ key: '{', code: 'BracketLeft', shiftKey: true })).toEqual(untouched)
    expect(step({ key: 'ß', code: 'BracketLeft', ctrlKey: true })).toEqual(untouched)
  })

  it('leaves the keys to a text field, and to anything in a dialog or a popover', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    expect(step({ key: '[', code: 'BracketLeft' }, input('text'))).toEqual(untouched)
    expect(step({ key: 'ß', code: 'BracketLeft' }, input('number'))).toEqual(untouched)
    // Base UI gives a popover, the brush size popover among them, the role dialog, as a dialog.
    for (const role of ['dialog', 'alertdialog']) {
      const popup = document.createElement('div')
      popup.setAttribute('role', role)
      const slider = document.createElement('input')
      slider.type = 'range'
      popup.append(slider)
      document.body.append(popup)
      expect(step({ key: '[', code: 'BracketLeft' }, slider), role).toEqual(untouched)
      expect(step({ key: ']', code: 'BracketRight' }, popup), role).toEqual(untouched)
      popup.remove()
    }
  })

  it('lets a bare bracket through a slider outside a popup, and keeps an AltGr one there', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    const slider = input('range')
    expect(step({ key: '[', code: 'BracketLeft' }, slider)).toEqual(smaller)
    expect(step({ key: 'ü', code: 'BracketLeft' }, slider)).toEqual(smaller)
    expect(step({ key: '[', code: 'Digit8', altGraph: true }, slider)).toEqual(untouched)
    expect(step({ key: '[', code: 'Digit8', ctrlKey: true, altKey: true }, slider)).toEqual(
      untouched,
    )
  })

  it('works with no QueryClientProvider, as Providers mounts it, on the page shown', () => {
    pick('eraser')
    // renderHook gives no provider: the hook reads the page shown from Koharu's queryClient.
    renderHook(() => useBooharuBrush())
    expect(step({ key: ']', code: 'BracketRight' })).toEqual(larger)
  })

  it('leaves the keys alone, unprevented, with no page shown', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    queryClient.setQueryData(pageKey, null)
    expect(step({ key: '[', code: 'BracketLeft' })).toEqual(untouched)
    expect(step({ key: 'ß', code: 'BracketLeft' })).toEqual(untouched)
  })

  it('does nothing while reading, nor with a tool that has no brush', () => {
    pick('remove')
    renderHook(() => useBooharuBrush())
    read(true)
    expect(step({ key: '[', code: 'BracketLeft' })).toEqual(untouched)
    expect(step({ key: 'ß', code: 'BracketLeft' })).toEqual(untouched)
    read(false)
    pick('select')
    expect(step({ key: '[', code: 'BracketLeft' })).toEqual(untouched)
    pick('remove')
    expect(step({ key: '[', code: 'BracketLeft' })).toEqual(smaller)
  })

  it('does nothing while Settings shows in place of the editor', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    act(() => useKoharuStore.getState().setSettingsOpen(true))
    expect(step({ key: ']', code: 'BracketRight' })).toEqual(untouched)
    expect(step({ key: 'ß', code: 'BracketLeft' })).toEqual(untouched)
    expect(useBrushSlots.getState().draw.diameter).toBe(48)
    act(() => useKoharuStore.getState().setSettingsOpen(false))
    expect(step({ key: ']', code: 'BracketRight' })).toEqual(larger)
    expect(useBrushSlots.getState().draw.diameter).toBe(53)
  })

  it('leaves a key another handler took', () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    const taken = (event: Event) => event.preventDefault()
    document.body.addEventListener('keydown', taken)
    useKoharuStore.getState().setBrush({ ...useKoharuStore.getState().brush, diameter: 48 })
    press({ key: '[', code: 'BracketLeft' })
    expect(diameter()).toBe(48)
    document.body.removeEventListener('keydown', taken)
  })
})

describe("Booharu's brush for each tool", () => {
  it('brings each tool its own size and keeps the size set while it is picked', () => {
    renderHook(() => useBooharuBrush())
    pick('draw')
    useKoharuStore.getState().setBrush({ ...useKoharuStore.getState().brush, diameter: 20 })
    pick('eraser')
    expect(diameter()).toBe(48)
    useKoharuStore.getState().setBrush({ ...useKoharuStore.getState().brush, diameter: 90 })
    pick('remove')
    expect(diameter()).toBe(48)
    press({ key: ']', code: 'BracketRight' })
    expect(diameter()).toBe(53)
    // A tool without a brush leaves Koharu's size, and every brush, as they were.
    pick('select')
    expect(diameter()).toBe(53)
    pick('draw')
    expect(diameter()).toBe(20)
    pick('eraser')
    expect(diameter()).toBe(90)
    expect(useBrushSlots.getState()).toMatchObject({
      draw: { diameter: 20 },
      eraser: { diameter: 90 },
      remove: { diameter: 53 },
    })
  })

  it('keeps the colour shared by every tool', () => {
    renderHook(() => useBooharuBrush())
    pick('draw')
    useKoharuStore.getState().setBrush({ diameter: 20, color: '#FF0000' })
    pick('eraser')
    expect(useKoharuStore.getState().brush).toEqual({ diameter: 48, color: '#FF0000' })
  })

  it('takes a value only in its range, and an opacity only for Draw', () => {
    setBrushSlot('draw', { hardness: 0.25, opacity: 0.5 })
    setBrushSlot('draw', { hardness: 1.5, opacity: 0, diameter: 0 })
    setBrushSlot('eraser', { hardness: 0, opacity: 0.5 })
    expect(useBrushSlots.getState()).toEqual({
      draw: { diameter: 48, hardness: 0.25, opacity: 0.5 },
      eraser: { diameter: 48, hardness: 0 },
      remove: { diameter: 48, hardness: 1 },
    })
  })

  it('sends the feel as restored, then on each change, each once', async () => {
    setBrushSlot('remove', { hardness: 0.5 })
    renderHook(() => useBooharuBrush())
    await settled()
    expect(setBrush).toHaveBeenCalledTimes(1)
    expect(setBrush).toHaveBeenLastCalledWith({
      draw_hardness: 1,
      eraser_hardness: 1,
      remove_hardness: 0.5,
      draw_opacity: 1,
    })

    setBrushSlot('draw', { hardness: 0.2, opacity: 0.4 })
    setBrushSlot('eraser', { hardness: 0.7 })
    await settled()
    expect(setBrush).toHaveBeenCalledTimes(3)
    expect(setBrush).toHaveBeenLastCalledWith({
      draw_hardness: 0.2,
      eraser_hardness: 0.7,
      remove_hardness: 0.5,
      draw_opacity: 0.4,
    })

    // A size is no feel: the canvas has it through Koharu's brush.
    pick('draw')
    useKoharuStore.getState().setBrush({ ...useKoharuStore.getState().brush, diameter: 70 })
    await settled()
    expect(setBrush).toHaveBeenCalledTimes(3)
    expect(brushFeel(useBrushSlots.getState())).toEqual(setBrush.mock.lastCall?.[0])
  })
})

describe("Booharu's brushes between runs", () => {
  it('come back from what was kept', async () => {
    setBrushSlot('draw', { diameter: 30, hardness: 0.3, opacity: 0.6 })
    setBrushSlot('remove', { diameter: 70, hardness: 0.1 })
    const kept = localStorage.getItem('booharu.brushes')
    expect(kept).not.toBeNull()

    useBrushSlots.setState(useBrushSlots.getInitialState())
    localStorage.setItem('booharu.brushes', kept ?? '')
    await useBrushSlots.persist.rehydrate()
    expect(useBrushSlots.getState()).toEqual({
      draw: { diameter: 30, hardness: 0.3, opacity: 0.6 },
      eraser: { diameter: 48, hardness: 1 },
      remove: { diameter: 70, hardness: 0.1 },
    })
  })

  it("start at Koharu's size on a first load, and take nothing out of range", async () => {
    localStorage.removeItem('booharu.brushes')
    useKoharuStore.getState().setBrush({ diameter: 64, color: '#000000' })
    await useBrushSlots.persist.rehydrate()
    expect(useBrushSlots.getState()).toEqual({
      draw: { diameter: 64, hardness: 1, opacity: 1 },
      eraser: { diameter: 64, hardness: 1 },
      remove: { diameter: 64, hardness: 1 },
    })

    const stored = {
      draw: { diameter: 0, hardness: 2, opacity: 0 },
      eraser: { diameter: 'big', hardness: -1 },
      remove: { diameter: 12, hardness: 0.5 },
    }
    localStorage.setItem('booharu.brushes', JSON.stringify({ state: stored, version: 0 }))
    await useBrushSlots.persist.rehydrate()
    expect(useBrushSlots.getState()).toEqual({
      draw: { diameter: 64, hardness: 1, opacity: 1 },
      eraser: { diameter: 64, hardness: 1 },
      remove: { diameter: 12, hardness: 0.5 },
    })
    expect(restoredSlots('nothing kept')).toEqual(restoredSlots(undefined))
  })

  it("keep only the brushes, and Koharu's brush only its size and colour", () => {
    pick('draw')
    renderHook(() => useBooharuBrush())
    setBrushSlot('draw', { hardness: 0.5, opacity: 0.5 })
    useKoharuStore.getState().setBrush({ ...useKoharuStore.getState().brush, diameter: 33 })

    const booharu = JSON.parse(localStorage.getItem('booharu.brushes') ?? '{}')
    expect(Object.keys(booharu.state).sort()).toEqual(['draw', 'eraser', 'remove'])
    const koharu = JSON.parse(localStorage.getItem('koharu.workspace') ?? '{}')
    expect(koharu.state.brush).toEqual({ diameter: 33, color: '#FFFFFF' })
  })
})
