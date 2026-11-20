import { cleanup, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { reservedKeys } from '@/lib/booharu/keys'
import { typingShortcut, useTypingShortcuts } from '@/lib/booharu/typingKeys'
import { useKoharuStore } from '@/lib/store'
import { commands, type CharStyle, type Typed } from '@koharu/ipc/protocol'

/** A key pressed on `target`, the page itself unless named; `keyCode` stands in for the one
 * Chromium gives, which jsdom leaves out. */
function press(init: KeyboardEventInit & { keyCode?: number }, target: Element = document.body) {
  const { keyCode, ...rest } = init
  const event = new KeyboardEvent('keydown', { ...rest, bubbles: true, cancelable: true })
  if (keyCode !== undefined) Object.defineProperty(event, 'keyCode', { value: keyCode })
  target.dispatchEvent(event)
  return event
}

const ctrlB = { key: 'b', code: 'KeyB', ctrlKey: true }
const ctrlI = { key: 'i', code: 'KeyI', ctrlKey: true }

/** The canvas typing into the text, its selected characters sharing `chars`, but for `mixed`. */
function typing(chars: CharStyle, mixed: Typed['mixed'] = []): Typed {
  return { layer: 'text', chars, mixed }
}

let styled: ReturnType<typeof vi.spyOn>
let unmarked: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  styled = vi.spyOn(commands, 'styleTyping').mockResolvedValue(undefined as never)
  unmarked = vi.spyOn(commands, 'booharuUnmarkTyping').mockResolvedValue(undefined as never)
})

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe('Booharu Ctrl+B and Ctrl+I while typing', () => {
  it('reserves both keys from Booharu key actions', () => {
    expect(reservedKeys.has('ctrl+b')).toBe(true)
    expect(reservedKeys.has('ctrl+i')).toBe(true)
  })

  it('makes the characters typed bold with Ctrl or Cmd, and takes the bold off them (S11-d)', () => {
    renderHook(() => useTypingShortcuts())
    useKoharuStore.setState({ typing: typing({ weight: 400 }) })

    expect(press(ctrlB).defaultPrevented).toBe(true)
    expect(styled).toHaveBeenLastCalledWith({ weight: 700 })

    // Off takes the weight mark off for real, so the text's own weight shows again.
    useKoharuStore.setState({ typing: typing({ weight: 700 }) })
    expect(press({ key: 'b', code: 'KeyB', metaKey: true }).defaultPrevented).toBe(true)
    expect(unmarked).toHaveBeenCalledExactlyOnceWith('bold')

    // Mixed weights go bold, every one.
    useKoharuStore.setState({ typing: typing({}, ['weight']) })
    press(ctrlB)
    expect(styled).toHaveBeenLastCalledWith({ weight: 700 })
    expect(styled).toHaveBeenCalledTimes(2)
    expect(unmarked).toHaveBeenCalledOnce()
  })

  it('makes the characters typed italic unless every one is, and takes the italic off them', () => {
    renderHook(() => useTypingShortcuts())
    useKoharuStore.setState({ typing: typing({ italic: false }) })
    expect(press(ctrlI).defaultPrevented).toBe(true)
    expect(styled).toHaveBeenLastCalledWith({ italic: true })

    useKoharuStore.setState({ typing: typing({ italic: true }) })
    expect(press(ctrlI).defaultPrevented).toBe(true)
    expect(unmarked).toHaveBeenCalledExactlyOnceWith('italic')

    useKoharuStore.setState({ typing: typing({}, ['italic']) })
    press(ctrlI)
    expect(styled).toHaveBeenLastCalledWith({ italic: true })
    expect(styled).toHaveBeenCalledTimes(2)
    expect(unmarked).toHaveBeenCalledOnce()
  })

  it('goes by the letter a layout types, or the place of B and I where it types none', () => {
    // Russian: the B and I keys type и and ш.
    expect(typingShortcut(new KeyboardEvent('keydown', { ...ctrlB, key: 'и' }))).toBe('bold')
    expect(typingShortcut(new KeyboardEvent('keydown', { ...ctrlI, key: 'ш' }))).toBe('italic')
    // Dvorak: B's place types x, and the key typing b is elsewhere.
    expect(typingShortcut(new KeyboardEvent('keydown', { ...ctrlB, key: 'x' }))).toBeNull()
    expect(
      typingShortcut(new KeyboardEvent('keydown', { key: 'b', code: 'KeyN', ctrlKey: true })),
    ).toBe('bold')
    expect(typingShortcut(new KeyboardEvent('keydown', { ...ctrlB, key: 'B' }))).toBe('bold')
    expect(typingShortcut(new KeyboardEvent('keydown', { key: 'b', code: 'KeyB' }))).toBeNull()
  })

  it('leaves the keys alone in a text field, while composing, without typing, or with Shift or Alt', () => {
    renderHook(() => useTypingShortcuts())
    useKoharuStore.setState({ typing: typing({ weight: 400 }) })
    const textarea = document.createElement('textarea')
    const input = document.createElement('input')
    document.body.append(textarea, input)

    const left = [
      press(ctrlB, textarea),
      press(ctrlI, input),
      press({ ...ctrlB, isComposing: true }),
      press({ ...ctrlI, keyCode: 229 }),
      press({ ...ctrlB, shiftKey: true }),
      press({ ...ctrlI, altKey: true }),
      press({ key: 'b', code: 'KeyB' }),
    ]
    useKoharuStore.setState({ typing: null })
    left.push(press(ctrlB), press(ctrlI))

    expect(left.map((event) => event.defaultPrevented)).toEqual(left.map(() => false))
    expect(styled).not.toHaveBeenCalled()
    expect(unmarked).not.toHaveBeenCalled()
  })

  it('leaves a key another handler took, and stops listening once unmounted', () => {
    const shortcuts = renderHook(() => useTypingShortcuts())
    useKoharuStore.setState({ typing: typing({ weight: 700 }) })
    const taken = (event: KeyboardEvent) => event.preventDefault()
    document.body.addEventListener('keydown', taken)
    press(ctrlB)
    document.body.removeEventListener('keydown', taken)
    expect(unmarked).not.toHaveBeenCalled()

    shortcuts.unmount()
    expect(press(ctrlB).defaultPrevented).toBe(false)
    expect(styled).not.toHaveBeenCalled()
    expect(unmarked).not.toHaveBeenCalled()
  })
})
