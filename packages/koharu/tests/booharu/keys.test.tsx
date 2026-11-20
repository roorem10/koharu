import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { CanvasWorkspace } from '@/components/editor/CanvasWorkspace'
import { controlKeepsKey, keyActions, reservedKeys } from '@/lib/booharu/keys'
import { queryClient } from '@/lib/queries'
import { toolKeys, useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'

/** Keys a focused control keeps: Space, the arrows, Enter, Delete, Escape, and the canvas's own
 * Ctrl, Cmd and Alt chords (undo, redo, select all, deselect, fit). */
const controlKeys: KeyboardEventInit[] = [
  { key: ' ', code: 'Space' },
  { key: 'ArrowLeft' },
  { key: 'ArrowRight' },
  { key: 'ArrowUp' },
  { key: 'ArrowDown' },
  { key: 'Enter' },
  { key: 'Delete' },
  { key: 'Backspace' },
  { key: 'Escape' },
  { key: 'z', ctrlKey: true },
  { key: 'Z', ctrlKey: true, shiftKey: true },
  { key: 'z', metaKey: true },
  { key: 'a', ctrlKey: true },
  { key: 'd', ctrlKey: true },
  { key: '0', ctrlKey: true },
  { key: 'v', altKey: true },
]

/** `init` pressed on `target`; the event keeps its target once dispatched. */
function press(target: Element, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { ...init, bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event
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

  it("reserves Koharu's fixed keys and every one of its tools' letters", () => {
    const fixed = ['ctrl+z', 'ctrl+shift+z', 'ctrl+a', 'ctrl+d', 'ctrl+0']
    for (const key of [...fixed, 'delete', 'backspace', 'escape', 'space', 'a', 'm', 'l']) {
      expect(reservedKeys.has(key), key).toBe(true)
    }
    // Koharu's handler selects a tool by its letter with or without Shift, whatever tools a Koharu
    // release adds; only Booharu's own tool keys are left to Booharu.
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
  function renderWorkspace() {
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
    sent.length = 0
    return { view, sent }
  }

  it('switches tools from the zoom slider and leaves the slider its own keys', () => {
    const { view, sent } = renderWorkspace()
    const slider = view.container.querySelector<HTMLInputElement>('footer input[type="range"]')
    expect(slider).not.toBeNull()
    act(() => slider!.focus())

    for (const init of controlKeys.filter(({ key }) => !key?.startsWith('Arrow'))) {
      // Not prevented: the canvas took none of them, Space included.
      expect(fireEvent.keyDown(slider!, init), JSON.stringify(init)).toBe(true)
    }
    for (const key of ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown']) {
      fireEvent.keyDown(slider!, { key })
    }
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

  it('selects the O tool with O, from the page and from a focused slider', () => {
    const { view } = renderWorkspace()
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

  it('leaves every key to a text field, a select and editable text', () => {
    const { sent } = renderWorkspace()
    for (const control of textEntries()) {
      document.body.append(control)
      for (const init of [{ key: 'b' }, ...controlKeys]) {
        fireEvent.keyDown(control, init)
      }
      control.remove()
    }
    const state = useKoharuStore.getState()
    expect(state.tool).toBe('select')
    expect(state.selectedLayers).toEqual(['layer'])
    expect(sent).toEqual([])
  })
})
