import type { ShortcutAction, Shortcuts, ShortcutSlot } from './store'

export const shortcutActions: readonly ShortcutAction[] = [
  'select',
  'text',
  'draw',
  'eraser',
  'color_picker',
  'remove',
  'pan',
  'fit',
]

export const shortcutLabels: Record<ShortcutAction, string> = {
  select: 'tools.select',
  text: 'tools.text',
  draw: 'tools.draw',
  eraser: 'tools.eraser',
  color_picker: 'tools.color_picker',
  remove: 'tools.remove',
  pan: 'tools.pan',
  fit: 'canvas.fit',
}

const modifiers = ['ctrl', 'alt', 'shift'] as const
const namedKey = /^(numpad(add|subtract|[0-9])|f([1-9]|1[0-2])|plus)$/
const reservedKey = /^(delete|backspace|escape|space|arrow(left|right|up|down))$/

function parts(binding: string): string[] | null {
  if (!binding || [...binding].length > 32 || binding !== binding.toLowerCase()) return null
  const result = binding.split('+')
  const key = result.at(-1)!
  let previous = -1
  for (const modifier of result.slice(0, -1)) {
    const index = modifiers.indexOf(modifier as (typeof modifiers)[number])
    if (index <= previous) return null
    previous = index
  }
  return key ? result : null
}

export function isBinding(binding: string): boolean {
  if (binding === '') return true
  const parsed = parts(binding)
  const key = parsed?.at(-1)
  return !!key && (namedKey.test(key) || ([...key].length === 1 && !/[\s\p{Cc}\p{Cf}]/u.test(key)))
}

/** The two main-row symbol pairs intentionally match with or without Shift. */
function identity(binding: string): string | null {
  const parsed = parts(binding)
  if (!parsed) return null
  let key = parsed.pop()!
  if (!isBinding(binding) && !reservedKey.test(key)) return null
  if (key === 'plus' || key === '=') key = '='
  if (key === '_' || key === '-') key = '-'
  return [
    ...parsed.filter((modifier) => modifier !== 'shift' || (key !== '=' && key !== '-')),
    key,
  ].join('+')
}

const unshiftedCodes: Record<string, string> = {
  Equal: '=',
  Minus: '-',
  BracketLeft: '[',
  BracketRight: ']',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Backslash: '\\',
  Comma: ',',
  Period: '.',
  Slash: '/',
}
const unshiftedSymbols: Record<string, string> = {
  '+': '=',
  _: '-',
  '!': '1',
  '@': '2',
  '#': '3',
  $: '4',
  '%': '5',
  '^': '6',
  '&': '7',
  '*': '8',
  '(': '9',
  ')': '0',
  '{': '[',
  '}': ']',
  ':': ';',
  '"': "'",
  '~': '`',
  '|': '\\',
  '<': ',',
  '>': '.',
  '?': '/',
}

let keyboardLayout: ReadonlyMap<string, string> | undefined
let layoutRequest: Promise<void> | undefined

export function loadShortcutLayout(): Promise<void> {
  const keyboard = (
    navigator as Navigator & {
      keyboard?: { getLayoutMap: () => Promise<ReadonlyMap<string, string>> }
    }
  ).keyboard
  if (!layoutRequest && keyboard?.getLayoutMap) {
    layoutRequest = keyboard
      .getLayoutMap()
      .then((layout) => {
        keyboardLayout = layout
      })
      .catch(() => undefined)
  }
  return layoutRequest ?? Promise.resolve()
}

type KeyEvent = Pick<
  KeyboardEvent,
  'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'isComposing'
>

export function bindingFromEvent(event: KeyEvent): string | null {
  if (
    event.isComposing ||
    ['Control', 'Meta', 'Alt', 'AltGraph', 'Shift', 'Dead', 'Unidentified'].includes(event.key)
  )
    return null
  let key: string
  if (/^Numpad(Add|Subtract|[0-9])$/.test(event.code)) key = event.code.toLowerCase()
  else if (/^F([1-9]|1[0-2])$/.test(event.key)) key = event.key.toLowerCase()
  else if (event.code === 'Space' || event.key === ' ') key = 'space'
  else {
    key = event.key.toLowerCase()
    if (event.shiftKey && [...key].length === 1) {
      // Letters use the layout's key value. Punctuation falls back to the unshifted
      // main-row code when a shifted event no longer exposes that character.
      key =
        keyboardLayout?.get(event.code)?.toLowerCase() ??
        unshiftedCodes[event.code] ??
        (/^Digit[0-9]$/.test(event.code) ? event.code.slice(-1) : (unshiftedSymbols[key] ?? key))
    }
    if (key === '+') key = 'plus'
  }
  const binding = [
    event.ctrlKey || event.metaKey ? 'ctrl' : '',
    event.altKey ? 'alt' : '',
    event.shiftKey ? 'shift' : '',
    key,
  ]
    .filter(Boolean)
    .join('+')
  return isBinding(binding) ||
    builtInShortcuts.some(
      (entry) =>
        entry.checked &&
        entry.bindings.some(
          (reserved) => identity(reserved) === identity(binding) && identity(binding) !== null,
        ),
    )
    ? binding
    : null
}

export function matchesBinding(event: KeyEvent, binding: string): boolean {
  const recorded = bindingFromEvent(event)
  const expected = identity(binding)
  return expected !== null && recorded !== null && identity(recorded) === expected
}

export function hasModifier(binding: string): boolean {
  return binding.includes('+')
}

export function formatBinding(binding: string): string {
  const names: Record<string, string> = {
    ctrl: 'Ctrl',
    alt: 'Alt',
    shift: 'Shift',
    plus: '+',
    '-': '−',
    numpadadd: 'Num +',
    numpadsubtract: 'Num −',
    space: 'Space',
    escape: 'Esc',
    delete: 'Delete',
    backspace: 'Backspace',
    arrowleft: '←',
    arrowright: '→',
    arrowup: '↑',
    arrowdown: '↓',
  }
  return binding
    .split('+')
    .map(
      (key) =>
        names[key] ?? (/^numpad[0-9]$/.test(key) ? `Num ${key.slice(-1)}` : key.toUpperCase()),
    )
    .join(' + ')
}

export function shortcutSlots(action: ShortcutAction): readonly [ShortcutSlot, ShortcutSlot] {
  return [action, `${action}_alt`]
}

export function findShortcut(event: KeyEvent, shortcuts: Shortcuts): ShortcutAction | undefined {
  return shortcutActions.find((action) =>
    shortcutSlots(action).some((slot) => matchesBinding(event, shortcuts[slot])),
  )
}

interface BuiltInShortcut {
  id: string
  group: 'edit' | 'canvas' | 'mouse' | 'fonts' | 'agent' | 'import'
  labelKey: string
  keys: string[]
  checked: boolean
  bindings: string[]
}

export const builtInShortcuts: readonly BuiltInShortcut[] = [
  {
    id: 'undo',
    group: 'edit',
    labelKey: 'menu.undo',
    keys: ['Ctrl + Z'],
    checked: true,
    bindings: ['ctrl+z'],
  },
  {
    id: 'redo',
    group: 'edit',
    labelKey: 'menu.redo',
    keys: ['Ctrl + Shift + Z'],
    checked: true,
    bindings: ['ctrl+shift+z'],
  },
  {
    id: 'selectAll',
    group: 'edit',
    labelKey: 'settings.shortcuts.fixed.selectAll',
    keys: ['Ctrl + A'],
    checked: true,
    bindings: ['ctrl+a'],
  },
  {
    id: 'delete',
    group: 'edit',
    labelKey: 'settings.shortcuts.fixed.delete',
    keys: ['Delete', 'Backspace'],
    checked: true,
    bindings: ['delete', 'backspace'],
  },
  {
    id: 'cancel',
    group: 'edit',
    labelKey: 'settings.shortcuts.fixed.cancel',
    keys: ['Esc'],
    checked: true,
    bindings: ['escape'],
  },
  {
    id: 'pan',
    group: 'canvas',
    labelKey: 'settings.shortcuts.fixed.pan',
    keys: ['Space'],
    checked: true,
    bindings: ['space'],
  },
  {
    id: 'nudge',
    group: 'canvas',
    labelKey: 'settings.shortcuts.fixed.nudge',
    keys: ['← ↑ ↓ →'],
    checked: true,
    bindings: ['arrowleft', 'arrowright', 'arrowup', 'arrowdown'],
  },
  {
    id: 'nudgeFast',
    group: 'canvas',
    labelKey: 'settings.shortcuts.fixed.nudgeFast',
    keys: ['Shift + ← ↑ ↓ →'],
    checked: true,
    bindings: ['shift+arrowleft', 'shift+arrowright', 'shift+arrowup', 'shift+arrowdown'],
  },
  {
    id: 'addSelection',
    group: 'mouse',
    labelKey: 'settings.shortcuts.fixed.addSelection',
    keys: ['settings.shortcuts.gestures.select'],
    checked: false,
    bindings: [],
  },
  {
    id: 'rangeSelection',
    group: 'mouse',
    labelKey: 'settings.shortcuts.fixed.rangeSelection',
    keys: ['settings.shortcuts.gestures.range'],
    checked: false,
    bindings: [],
  },
  {
    id: 'middlePan',
    group: 'mouse',
    labelKey: 'settings.shortcuts.fixed.middlePan',
    keys: ['settings.shortcuts.gestures.middle'],
    checked: false,
    bindings: [],
  },
  {
    id: 'brush',
    group: 'mouse',
    labelKey: 'settings.shortcuts.fixed.brush',
    keys: ['settings.shortcuts.gestures.brush'],
    checked: false,
    bindings: [],
  },
  {
    id: 'horizontal',
    group: 'mouse',
    labelKey: 'settings.shortcuts.fixed.horizontal',
    keys: ['settings.shortcuts.gestures.horizontal'],
    checked: false,
    bindings: [],
  },
  {
    id: 'font',
    group: 'fonts',
    labelKey: 'settings.shortcuts.fixed.font',
    keys: ['↑ / ↓', 'Enter'],
    checked: false,
    bindings: [],
  },
  {
    id: 'send',
    group: 'agent',
    labelKey: 'settings.shortcuts.fixed.send',
    keys: ['Enter', 'Shift + Enter'],
    checked: false,
    bindings: [],
  },
  {
    id: 'drop',
    group: 'import',
    labelKey: 'settings.shortcuts.fixed.drop',
    keys: ['Esc'],
    checked: false,
    bindings: [],
  },
]

export type ShortcutConflict = { slot: ShortcutSlot } | { builtIn: BuiltInShortcut }

export function shortcutConflicts(
  shortcuts: Shortcuts,
): Partial<Record<ShortcutSlot, ShortcutConflict[]>> {
  const conflicts: Partial<Record<ShortcutSlot, ShortcutConflict[]>> = {}
  const slots = shortcutActions.flatMap(shortcutSlots)
  for (const slot of slots) {
    const key = identity(shortcuts[slot])
    if (key === null) continue
    const found: ShortcutConflict[] = slots
      .filter((other) => other !== slot && identity(shortcuts[other]) === key)
      .map((slot) => ({ slot }))
    for (const entry of builtInShortcuts) {
      if (entry.checked && entry.bindings.some((binding) => identity(binding) === key))
        found.push({ builtIn: entry })
    }
    if (found.length) conflicts[slot] = found
  }
  return conflicts
}
