import { describe, expect, it } from 'vitest'

import {
  bindingFromEvent,
  findShortcut,
  formatBinding,
  isBinding,
  loadShortcutLayout,
  matchesBinding,
  shortcutConflicts,
} from '@/lib/shortcuts'
import { defaultShortcuts, receiveShortcuts, useKoharuStore } from '@/lib/store'

const key = (options: KeyboardEventInit) => new KeyboardEvent('keydown', options)

describe('shortcut bindings', () => {
  it('records modifiers in order, ignores modifier-only presses and keeps numpad keys distinct', () => {
    expect(
      bindingFromEvent(
        key({ key: 'Z', code: 'KeyZ', metaKey: true, altKey: true, shiftKey: true }),
      ),
    ).toBe('ctrl+alt+shift+z')
    for (const name of ['Control', 'Meta', 'Alt', 'Shift', 'Dead'])
      expect(bindingFromEvent(key({ key: name }))).toBeNull()
    expect(bindingFromEvent(key({ key: 'a', code: 'KeyQ' }))).toBe('a')
    expect(bindingFromEvent(key({ key: 'é', code: 'Digit2' }))).toBe('é')
    expect(bindingFromEvent(key({ key: '+', code: 'NumpadAdd', ctrlKey: true }))).toBe(
      'ctrl+numpadadd',
    )
    expect(bindingFromEvent(key({ key: 'Insert', code: 'Numpad0' }))).toBe('numpad0')
    expect(bindingFromEvent(key({ key: 'F12' }))).toBe('f12')
    expect(bindingFromEvent(key({ key: 'z', isComposing: true }))).toBeNull()
  })

  it('records unshifted punctuation with a separate Shift modifier', () => {
    expect(bindingFromEvent(key({ key: '+', code: 'Equal', ctrlKey: true, shiftKey: true }))).toBe(
      'ctrl+shift+=',
    )
    expect(bindingFromEvent(key({ key: '_', code: 'Minus', ctrlKey: true, shiftKey: true }))).toBe(
      'ctrl+shift+-',
    )
    expect(bindingFromEvent(key({ key: '!', code: 'Digit1', shiftKey: true }))).toBe('shift+1')
    expect(bindingFromEvent(key({ key: '}', code: 'BracketRight', shiftKey: true }))).toBe(
      'shift+]',
    )
    expect(bindingFromEvent(key({ key: '+', ctrlKey: true }))).toBe('ctrl+plus')
  })

  it('uses the same main-row equivalence for matching and conflicts', () => {
    for (const binding of ['ctrl+=', 'ctrl+plus', 'ctrl+shift+=']) {
      expect(matchesBinding(key({ key: '=', code: 'Equal', ctrlKey: true }), binding)).toBe(true)
      expect(
        matchesBinding(key({ key: '+', code: 'Equal', ctrlKey: true, shiftKey: true }), binding),
      ).toBe(true)
      expect(matchesBinding(key({ key: '+', code: 'NumpadAdd', ctrlKey: true }), binding)).toBe(
        false,
      )
    }
    for (const binding of ['ctrl+-', 'ctrl+_', 'ctrl+shift+-'])
      expect(
        matchesBinding(key({ key: '_', code: 'Minus', ctrlKey: true, shiftKey: true }), binding),
      ).toBe(true)
    expect(matchesBinding(key({ key: 'z', ctrlKey: true, shiftKey: true }), 'ctrl+z')).toBe(false)
    const conflicts = shortcutConflicts({
      ...defaultShortcuts,
      select: 'ctrl+plus',
      text_alt: 'ctrl+shift+=',
    })
    expect(conflicts.select).toEqual([{ slot: 'text_alt' }])
    expect(conflicts.text_alt).toEqual([{ slot: 'select' }])
  })

  it('checks both slots and every reserved keyboard key, but not control-scoped keys', () => {
    for (const binding of [
      'ctrl+z',
      'ctrl+shift+z',
      'ctrl+a',
      'delete',
      'backspace',
      'escape',
      'space',
      'arrowleft',
      'shift+arrowdown',
    ]) {
      expect(
        shortcutConflicts({ ...defaultShortcuts, text_alt: binding }).text_alt?.[0],
      ).toHaveProperty('builtIn')
    }
    expect(shortcutConflicts(defaultShortcuts)).toEqual({})
    expect(shortcutConflicts({ ...defaultShortcuts, text_alt: 'alt+z' })).toEqual({})
    expect(shortcutConflicts({ ...defaultShortcuts, text_alt: 't' })).toMatchObject({
      text: [{ slot: 'text_alt' }],
      text_alt: [{ slot: 'text' }],
    })
  })

  it('loads complete bindings, preserves cleared slots and resolves a hand-edited collision in page order', () => {
    receiveShortcuts({ text: 'alt+z', text_alt: 'f2', fit: '', select: 'broken', unknown: 'q' })
    expect(useKoharuStore.getState().shortcuts).toMatchObject({
      select: 'v',
      text: 'alt+z',
      text_alt: 'f2',
      fit: '',
    })
    expect(findShortcut(key({ key: 'F2' }), useKoharuStore.getState().shortcuts)).toBe('text')
    expect(
      findShortcut(key({ key: 'q' }), { ...defaultShortcuts, select_alt: 'q', text: 'q' }),
    ).toBe('select')
  })

  it('validates the storage grammar and formats readable key combinations', () => {
    for (const binding of ['', 'v', 'é', 'ctrl+alt+shift+f12', 'ctrl+plus', 'numpad9'])
      expect(isBinding(binding)).toBe(true)
    for (const binding of [
      'Ctrl+v',
      'alt+ctrl+v',
      'ctrl+ctrl+v',
      'ctrl++',
      'meta+z',
      'ctrl+',
      'f13',
      'numpad10',
      '\n',
      ' ',
    ])
      expect(isBinding(binding)).toBe(false)
    expect(formatBinding('ctrl+-')).toBe('Ctrl + −')
    expect(formatBinding('ctrl+plus')).toBe('Ctrl + +')
    expect(formatBinding('ctrl+numpadadd')).toBe('Ctrl + Num +')
    expect(formatBinding('v')).toBe('V')
  })

  it('uses the available keyboard layout for shifted characters', async () => {
    Object.defineProperty(navigator, 'keyboard', {
      configurable: true,
      value: { getLayoutMap: async () => new Map([['Digit1', '&']]) },
    })
    try {
      await loadShortcutLayout()
      expect(
        bindingFromEvent(key({ key: '1', code: 'Digit1', ctrlKey: true, shiftKey: true })),
      ).toBe('ctrl+shift+&')
    } finally {
      Reflect.deleteProperty(navigator, 'keyboard')
    }
  })
})
