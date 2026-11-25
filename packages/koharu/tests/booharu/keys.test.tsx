import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { controlKeepsKey, keyActions, reservedKeys } from '@/lib/booharu/keys'
import { pageKey, queryClient } from '@/lib/queries'
import { shortcuts, toolKeys } from '@/lib/shortcuts'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page } from '@koharu/ipc/protocol'

/** Keys a focused control keeps: Space, the arrows, Enter, Delete, Escape, and the canvas's own
 * Ctrl, Cmd and Alt chords (undo, redo, select all, deselect, fit), as a browser names them, their
 * `code` included: Koharu's Delete, Escape, arrow and Space hotkeys match by it, so a key without
 * one would reach none of them and a test that the control kept it could never fail. */
const controlKeys: KeyboardEventInit[] = [
  { key: ' ', code: 'Space' },
  { key: 'ArrowLeft', code: 'ArrowLeft' },
  { key: 'ArrowRight', code: 'ArrowRight' },
  { key: 'ArrowUp', code: 'ArrowUp' },
  { key: 'ArrowDown', code: 'ArrowDown' },
  { key: 'Enter', code: 'Enter' },
  { key: 'Delete', code: 'Delete' },
  { key: 'Backspace', code: 'Backspace' },
  { key: 'Escape', code: 'Escape' },
  { key: 'z', code: 'KeyZ', ctrlKey: true },
  { key: 'Z', code: 'KeyZ', ctrlKey: true, shiftKey: true },
  { key: 'z', code: 'KeyZ', metaKey: true },
  { key: 'a', code: 'KeyA', ctrlKey: true },
  { key: 'd', code: 'KeyD', ctrlKey: true },
  { key: '0', code: 'Digit0', ctrlKey: true },
  { key: 'v', code: 'KeyV', altKey: true },
]

/** Keys the canvas takes from the page, as a browser names them, its `code` included, which
 * Koharu's hotkeys match by: a tool's letter, with Shift too, Show Original's `\`, Delete, Escape,
 * the arrows, and its Ctrl chords. */
const canvasKeys: KeyboardEventInit[] = [
  { key: 'v', code: 'KeyV' },
  { key: 'b', code: 'KeyB' },
  { key: 'k', code: 'KeyK' },
  { key: 'o', code: 'KeyO' },
  { key: 'B', code: 'KeyB', shiftKey: true },
  { key: '\\', code: 'Backslash' },
  { key: 'Delete', code: 'Delete' },
  { key: 'Backspace', code: 'Backspace' },
  { key: 'Escape', code: 'Escape' },
  { key: 'ArrowRight', code: 'ArrowRight' },
  { key: 'ArrowDown', code: 'ArrowDown', shiftKey: true },
  { key: 'z', code: 'KeyZ', ctrlKey: true },
  { key: 'Z', code: 'KeyZ', ctrlKey: true, shiftKey: true },
  { key: 'a', code: 'KeyA', ctrlKey: true },
  { key: 'd', code: 'KeyD', ctrlKey: true },
  { key: '0', code: 'Digit0', ctrlKey: true },
]

/** `init` pressed on `target`; the event keeps its target once dispatched. */
function press(target: Element, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { ...init, bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event
}

/** A popup of `role` holding a button, on the page. */
function popup(role: string): { popup: HTMLElement; button: HTMLButtonElement } {
  const element = document.createElement('div')
  element.setAttribute('role', role)
  const button = document.createElement('button')
  element.append(button)
  document.body.append(element)
  return { popup: element, button }
}

/** Lets the commands the workspace sent reach their spies: Koharu's `send()` runs each as a
 * mutation, which calls its command a few promises later. */
function settle(): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)))
}

function input(type: string): HTMLInputElement {
  const element = document.createElement('input')
  element.type = type
  return element
}

/** Every control that takes typed text. jsdom has no isContentEditable, which a browser gives an
 * editable element and everything inside it. */
function textEntries(): HTMLElement[] {
  const editable = document.createElement('div')
  editable.setAttribute('contenteditable', 'true')
  Object.defineProperty(editable, 'isContentEditable', { value: true })
  return [
    ...['text', 'search', 'number', 'email', 'url', 'password', 'tel'].map(input),
    document.createElement('textarea'),
    document.createElement('select'),
    editable,
  ]
}

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe('Booharu key registry', () => {
  it("gives no Booharu action a key Koharu's canvas fixes, nor two actions one key", () => {
    const keys = Object.values(keyActions).map(({ key }) => key)
    expect(keys.filter((key) => reservedKeys.has(key))).toEqual([])
    expect(new Set(keys).size).toBe(keys.length)
  })

  it("reserves Koharu's fixed keys, the arrows and every one of its tools' letters", () => {
    const fixed = ['ctrl+z', 'ctrl+shift+z', 'ctrl+a', 'ctrl+d', 'ctrl+0']
    const arrows = ['arrowleft', 'arrowright', 'arrowup', 'arrowdown']
    const nudges = [...arrows, ...arrows.map((key) => `shift+${key}`)]
    // Koharu 0.95.0's Show Original, F7, which switches Layers and Story, and the Block tool's K.
    const koharu095 = ['\\', 'f7', 'k', 'shift+k']
    for (const key of [
      ...fixed,
      'delete',
      'backspace',
      'escape',
      'space',
      ...nudges,
      'a',
      'm',
      ...koharu095,
    ]) {
      expect(reservedKeys.has(key), key).toBe(true)
    }
    // Koharu's tool hotkey selects a tool by its letter, and with Shift too as Booharu binds it,
    // whatever tools a Koharu release adds; only Booharu's own tool keys are left to Booharu.
    for (const [tool, key] of Object.entries(toolKeys)) {
      const koharu = tool !== 'ocr_box'
      expect(reservedKeys.has(key), tool).toBe(koharu)
      expect(reservedKeys.has(`shift+${key}`), tool).toBe(koharu)
    }
  })

  it('maps O to the O tool, the key Koharu reads from toolKeys', () => {
    expect(keyActions.ocrBoxTool).toEqual({ key: 'o', tool: 'ocr_box' })
    expect(toolKeys.ocr_box).toBe(keyActions.ocrBoxTool.key)
  })

  it("keeps Booharu's [, ] and O apart from Koharu 0.95.0's K, F7 and \\", () => {
    expect(Object.values(keyActions).map(({ key }) => key)).toEqual(['o', '[', ']'])
    expect([shortcuts.original, shortcuts.layers, toolKeys.block]).toEqual(['\\', 'f7', 'k'])
  })
})

describe('Booharu tool keys in a focused control', () => {
  it('lets a bare tool key through a slider, a checkbox or a colour input', () => {
    for (const type of ['range', 'checkbox', 'color']) {
      expect(controlKeepsKey(press(input(type), { key: 'v' })), type).toBe(false)
      expect(controlKeepsKey(press(input(type), { key: 't' })), type).toBe(false)
    }
  })

  it('leaves Space, arrows, Enter, Delete, Escape and chords to a slider', () => {
    for (const init of controlKeys) {
      expect(controlKeepsKey(press(input('range'), init)), JSON.stringify(init)).toBe(true)
    }
  })

  it('leaves every key to text entry', () => {
    for (const control of textEntries()) {
      for (const init of [{ key: 'v' }, { key: 't' }, ...controlKeys]) {
        const pressed = `${control.localName} ${JSON.stringify(init)}`
        expect(controlKeepsKey(press(control, init)), pressed).toBe(true)
      }
    }
  })

  it('passes every key from the page or a button to the canvas, as Koharu does', () => {
    for (const target of [document.body, document.createElement('button')]) {
      expect(controlKeepsKey(press(target, { key: 'v' }))).toBe(false)
      expect(controlKeepsKey(press(target, { key: 'z', ctrlKey: true }))).toBe(false)
    }
  })

  it('leaves every key to a button in a dialog, so Space presses it and Delete removes nothing', () => {
    for (const role of ['dialog', 'alertdialog']) {
      const dialog = document.createElement('div')
      dialog.setAttribute('role', role)
      const button = document.createElement('button')
      dialog.append(button)
      document.body.append(dialog)
      for (const init of [{ key: 'v' }, { key: 'o' }, ...controlKeys]) {
        expect(controlKeepsKey(press(button, init)), `${role} ${JSON.stringify(init)}`).toBe(true)
      }
      dialog.remove()
    }
  })

  it('leaves every key to an open list, so a letter finds its option', () => {
    // A list such as a select's, whose options take the focus while it's open.
    const list = document.createElement('div')
    list.setAttribute('role', 'listbox')
    const option = document.createElement('div')
    option.setAttribute('role', 'option')
    list.append(option)
    document.body.append(list)
    for (const target of [list, option]) {
      for (const init of [{ key: 'v' }, { key: 'o' }, ...controlKeys]) {
        const role = target.getAttribute('role')
        expect(controlKeepsKey(press(target, init)), `${role} ${JSON.stringify(init)}`).toBe(true)
      }
    }
  })

  it('leaves every key to an open menu, so Escape keeps the selection and Delete removes nothing', () => {
    // A menu such as the box menu a right-click on the canvas opens, which takes the focus.
    const menu = document.createElement('div')
    menu.setAttribute('role', 'menu')
    const item = document.createElement('div')
    item.setAttribute('role', 'menuitem')
    menu.append(item)
    // A menu bar's trigger is no menu: its keys reach the canvas, as a button's do.
    const bar = document.createElement('div')
    bar.setAttribute('role', 'menubar')
    const trigger = document.createElement('button')
    trigger.setAttribute('role', 'menuitem')
    bar.append(trigger)
    document.body.append(menu, bar)

    for (const target of [menu, item]) {
      for (const init of [{ key: 'v' }, { key: 'o' }, ...controlKeys]) {
        const role = target.getAttribute('role')
        expect(controlKeepsKey(press(target, init)), `${role} ${JSON.stringify(init)}`).toBe(true)
      }
    }
    expect(controlKeepsKey(press(trigger, { key: 'v' }))).toBe(false)
    expect(controlKeepsKey(press(trigger, { key: 'Delete' }))).toBe(false)
  })
})

describe('Booharu tool keys on the canvas', () => {
  /** The workspace, with one layer selected; `sent` lists the commands it sends from then on. */
  async function renderWorkspace() {
    const sent: string[] = []
    const table = commands as unknown as Record<string, () => Promise<unknown>>
    for (const name of Object.keys(table)) {
      vi.spyOn(table, name).mockImplementation(async () => {
        sent.push(name)
        return null
      })
    }
    // jsdom has no 2D canvas; the workspace then sends black for its theme colours.
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
    const view = render(
      <QueryClientProvider client={queryClient}>
        <CanvasWorkspace />
      </QueryClientProvider>,
    )
    act(() => useKoharuStore.setState({ selectedLayers: ['layer'] }))
    // What it sends as it mounts arrives a few promises later.
    await settle()
    sent.length = 0
    return { view, sent }
  }

  it('switches tools from the zoom slider and leaves the slider its own keys', async () => {
    const { view, sent } = await renderWorkspace()
    const slider = view.container.querySelector<HTMLInputElement>('footer input[type="range"]')
    expect(slider).not.toBeNull()
    act(() => slider!.focus())

    const arrow = ({ key }: KeyboardEventInit) => key?.startsWith('Arrow') ?? false
    for (const init of controlKeys.filter((key) => !arrow(key))) {
      // Not prevented: the canvas took none of them, Space, Delete and Escape included.
      expect(fireEvent.keyDown(slider!, init), JSON.stringify(init)).toBe(true)
    }
    for (const init of controlKeys.filter(arrow)) fireEvent.keyDown(slider!, init)
    await settle()
    const state = useKoharuStore.getState()
    expect(state.tool).toBe('select')
    expect(state.selectedLayers).toEqual(['layer'])
    // The arrows zoomed, as the slider does; the canvas undid, deleted and fitted nothing.
    expect(new Set(sent)).toEqual(new Set(['setZoom']))

    fireEvent.keyDown(slider!, { key: 'b' })
    expect(useKoharuStore.getState().tool).toBe('draw')
    fireEvent.keyDown(slider!, { key: 'v' })
    expect(useKoharuStore.getState().tool).toBe('select')
  })

  it('selects the O tool with O, from the page and from a focused slider', async () => {
    const { view } = await renderWorkspace()
    fireEvent.keyDown(document.body, { key: 'o' })
    expect(useKoharuStore.getState().tool).toBe('ocr_box')
    fireEvent.keyDown(document.body, { key: 'v' })
    expect(useKoharuStore.getState().tool).toBe('select')

    const slider = view.container.querySelector<HTMLInputElement>('footer input[type="range"]')
    act(() => slider!.focus())
    fireEvent.keyDown(slider!, { key: 'o' })
    expect(useKoharuStore.getState().tool).toBe('ocr_box')
    // A chord stays the slider's: Alt+V leaves the O tool selected.
    fireEvent.keyDown(slider!, { key: 'v', altKey: true })
    expect(useKoharuStore.getState().tool).toBe('ocr_box')
  })

  it("selects Koharu's Block tool with K, with Shift too and from a focused slider, beside O", async () => {
    const { view } = await renderWorkspace()
    fireEvent.keyDown(document.body, { key: 'k', code: 'KeyK' })
    expect(useKoharuStore.getState().tool).toBe('block')
    fireEvent.keyDown(document.body, { key: 'o', code: 'KeyO' })
    expect(useKoharuStore.getState().tool).toBe('ocr_box')
    fireEvent.keyDown(document.body, { key: 'K', code: 'KeyK', shiftKey: true })
    expect(useKoharuStore.getState().tool).toBe('block')

    const slider = view.container.querySelector<HTMLInputElement>('footer input[type="range"]')
    act(() => slider!.focus())
    fireEvent.keyDown(slider!, { key: 'v', code: 'KeyV' })
    fireEvent.keyDown(slider!, { key: 'k', code: 'KeyK' })
    expect(useKoharuStore.getState().tool).toBe('block')
  })

  it("toggles Koharu's Show Original with \\ from the page, the key going with the canvas, as Read mode takes its place", async () => {
    const { view, sent } = await renderWorkspace()
    // Prevented: the canvas took it.
    expect(fireEvent.keyDown(document.body, { key: '\\', code: 'Backslash' })).toBe(false)
    expect(useKoharuStore.getState().original).toBe(true)
    await settle()
    expect(commands.setOriginal).toHaveBeenLastCalledWith(true)
    fireEvent.keyDown(document.body, { key: '\\', code: 'Backslash' })
    expect(useKoharuStore.getState().original).toBe(false)
    // On again as Read mode comes.
    fireEvent.keyDown(document.body, { key: '\\', code: 'Backslash' })
    await settle()

    // Read mode shows the reader in the canvas's place: the scan is shown no more, and the key
    // toggles nothing there.
    view.unmount()
    await settle()
    expect(commands.setOriginal).toHaveBeenLastCalledWith(false)
    sent.length = 0
    expect(fireEvent.keyDown(document.body, { key: '\\', code: 'Backslash' })).toBe(true)
    expect(useKoharuStore.getState().original).toBe(true)
    await settle()
    expect(sent).toEqual([])
  })

  it('leaves every key to a text field, a select and editable text', async () => {
    const { sent } = await renderWorkspace()
    for (const control of textEntries()) {
      document.body.append(control)
      for (const init of [{ key: 'b' }, ...controlKeys, ...canvasKeys]) {
        fireEvent.keyDown(control, init)
      }
      control.remove()
    }
    await settle()
    const state = useKoharuStore.getState()
    expect(state.tool).toBe('select')
    expect(state.selectedLayers).toEqual(['layer'])
    expect(sent).toEqual([])
  })

  it('selects a tool with Shift and its letter too', async () => {
    await renderWorkspace()
    fireEvent.keyDown(document.body, { key: 'B', code: 'KeyB', shiftKey: true })
    expect(useKoharuStore.getState().tool).toBe('draw')
    fireEvent.keyDown(document.body, { key: 'O', code: 'KeyO', shiftKey: true })
    expect(useKoharuStore.getState().tool).toBe('ocr_box')
    fireEvent.keyDown(document.body, { key: 'V', code: 'KeyV', shiftKey: true })
    expect(useKoharuStore.getState().tool).toBe('select')
  })

  it("takes Koharu's keys from the page: undo, the arrows' nudge, Delete and Escape", async () => {
    // Delete takes the selected layers it finds on the shown page (Koharu's #1529).
    queryClient.setQueryData(pageKey, {
      id: 'page',
      size: { width: 100, height: 100 },
      background: null,
      layers: [
        {
          id: 'layer',
          name: 'layer',
          kind: 'text',
          content: [{ insert: 'Hi' }],
          visible: true,
          locked: false,
          opacity: 1,
          blend: 'normal',
          attributes: {},
        },
      ],
    } satisfies Page)
    const { sent } = await renderWorkspace()
    fireEvent.keyDown(document.body, { key: 'z', code: 'KeyZ', ctrlKey: true })
    expect(fireEvent.keyDown(document.body, { key: 'ArrowRight', code: 'ArrowRight' })).toBe(false)
    fireEvent.keyDown(document.body, { key: 'ArrowUp', code: 'ArrowUp', shiftKey: true })
    fireEvent.keyDown(document.body, { key: 'Delete', code: 'Delete' })
    await settle()
    expect(sent).toEqual(['undo', 'nudge', 'nudge', 'edit'])
    expect(commands.nudge).toHaveBeenNthCalledWith(1, 1, 0)
    expect(commands.nudge).toHaveBeenNthCalledWith(2, 0, -10)
    fireEvent.keyDown(document.body, { key: 'Escape', code: 'Escape' })
    expect(useKoharuStore.getState().selectedLayers).toEqual([])
    queryClient.removeQueries({ queryKey: pageKey })
  })

  it('leaves every key in a dialog, a menu or an open list to it', async () => {
    const { sent } = await renderWorkspace()
    for (const role of ['dialog', 'alertdialog', 'menu', 'listbox']) {
      const { popup: element, button } = popup(role)
      for (const init of canvasKeys) {
        // Not prevented either: the dialog's button, the menu's item or the list's option has it.
        expect(fireEvent.keyDown(button, init), `${role} ${JSON.stringify(init)}`).toBe(true)
      }
      element.remove()
    }
    await settle()
    const state = useKoharuStore.getState()
    expect(state.tool).toBe('select')
    expect(state.selectedLayers).toEqual(['layer'])
    expect(sent).toEqual([])
  })

  it("leaves Space to a dialog's button, which it presses, and holds no Hand tool", async () => {
    await renderWorkspace()
    const { button } = popup('dialog')
    expect(fireEvent.keyDown(button, { key: ' ', code: 'Space' })).toBe(true)
    expect(useKoharuStore.getState().tool).toBe('select')
    // The key let go isn't prevented either, which would stop the button's click.
    expect(fireEvent.keyUp(button, { key: ' ', code: 'Space' })).toBe(true)
    expect(useKoharuStore.getState().tool).toBe('select')
  })

  it('leaves alone Space let go that ends no hold, so a control that kept it pressed is pressed', async () => {
    await renderWorkspace()
    const button = document.createElement('button')
    document.body.append(button)
    // Space pressed never reached the canvas, as when a control stops it, so no hold began: the
    // key let go isn't prevented, which would stop the browser pressing the focused button.
    expect(fireEvent.keyUp(button, { key: ' ', code: 'Space' })).toBe(true)
    expect(useKoharuStore.getState().tool).toBe('select')
    // A hold begun on the page still takes the key let go on a button, and ends.
    fireEvent.keyDown(document.body, { key: ' ', code: 'Space' })
    expect(useKoharuStore.getState().tool).toBe('pan')
    expect(fireEvent.keyUp(button, { key: ' ', code: 'Space' })).toBe(false)
    expect(useKoharuStore.getState().tool).toBe('select')
  })

  it('ends a Space hold begun on the page when Space is let go in a dialog', async () => {
    await renderWorkspace()
    const { button } = popup('dialog')
    fireEvent.keyDown(document.body, { key: ' ', code: 'Space' })
    expect(useKoharuStore.getState().tool).toBe('pan')
    fireEvent.keyUp(button, { key: ' ', code: 'Space' })
    expect(useKoharuStore.getState().tool).toBe('select')
  })
})
