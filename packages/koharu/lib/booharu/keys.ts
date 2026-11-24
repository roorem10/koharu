/** Booharu's keys: its key actions with their default keys, the keys Koharu's canvas fixes, which
 * no Booharu key may take, and which keys a focused control keeps from the canvas's shortcuts. */

import { toolKeys } from '@/lib/shortcuts'
import type { CanvasTool } from '@/lib/store'

/** A key as the registry names it: the modifiers it needs, in the order `ctrl`, `alt`, `shift`,
 * then the key, `event.key` in lower case or `space` for the space bar, joined by `+`, as in
 * `ctrl+shift+z`. `ctrl` also stands for Cmd, as Koharu's canvas reads either. */
export type KeyName = string

/** One of Booharu's key actions: its default key, and the tool it selects when it's a tool's key.
 * A tool's key reaches the canvas through Koharu's `toolKeys` (`lib/shortcuts.ts`), which holds the
 * same letter, so Koharu's tool hotkey in CanvasWorkspace selects the tool as it selects its own. */
interface KeyAction {
  key: KeyName
  tool?: CanvasTool
}

/** Every Booharu key action, by its id, with its default key. */
export const keyActions = {
  /** The O tool: a box drawn with it becomes a text block that is read. */
  ocrBoxTool: { key: 'o', tool: 'ocr_box' },
  /** A smaller brush, with Brush, the Eraser or Remove: by a tenth of its size, at least 2 px, as
   * Photoshop's [ steps it (`lib/booharu/brush.ts`). */
  brushSmaller: { key: '[' },
  /** A larger brush, with Brush, the Eraser or Remove, as Photoshop's ]. */
  brushLarger: { key: ']' },
} as const satisfies Record<string, KeyAction>

/** The tools Booharu adds to Koharu's canvas, whose keys are Booharu's. */
export const booharuTools: ReadonlySet<CanvasTool> = new Set(
  Object.values<KeyAction>(keyActions).flatMap(({ tool }) => (tool ? [tool] : [])),
)

/** The keys no Booharu key action may take: those Koharu's hotkeys in CanvasWorkspace take (its
 * `shortcuts` and `toolKeys` in `lib/shortcuts.ts`), undo, redo, select all, deselect, fit, delete,
 * clear the selection, the hand tool held on Space, the arrows, which move the selection a pixel or
 * ten with Shift, and the letter of each of Koharu's tools, which selects it with or without Shift;
 * and bold and italic while the canvas types, which useTypingShortcuts takes. */
export const reservedKeys: ReadonlySet<KeyName> = new Set([
  'ctrl+z',
  'ctrl+shift+z',
  'ctrl+a',
  'ctrl+d',
  'ctrl+0',
  'ctrl+b',
  'ctrl+i',
  'delete',
  'backspace',
  'escape',
  'space',
  ...['arrowleft', 'arrowright', 'arrowup', 'arrowdown'].flatMap((key) => [key, `shift+${key}`]),
  ...(Object.entries(toolKeys) as [CanvasTool, string][])
    .filter(([tool]) => !booharuTools.has(tool))
    .flatMap(([, key]) => [key, `shift+${key}`]),
])

// Which keys a focused control keeps from the canvas's shortcuts (koharu#1181), the rule Booharu
// 0.0.4 shipped. Koharu's hotkeys leave every key to any focused input or form control
// (react-hotkeys-hook's form tags and roles), and a slider keeps focus after a click, so the tool
// keys stopped working until the canvas was clicked again.

/** The input types that take typed text. */
const textEntryTypes = new Set(['text', 'search', 'number', 'email', 'url', 'password', 'tel'])

/** Whether `target` takes typed text, and so keeps every key. */
export function isTextEntry(target: EventTarget | null): boolean {
  return (
    (target instanceof HTMLInputElement && textEntryTypes.has(target.type)) ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  )
}

/** Whether `target` is inside an open dialog, such as Box details or Settings (Base UI gives its
 * popup the role dialog or alertdialog), an open menu, such as the box menu a right-click on the
 * canvas opens, or a Story row's (the role menu), or an open list of options, such as a select's,
 * whose options take the focus and whose typeahead takes the letters (the role listbox). */
export function inDialog(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest('[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]') !==
      null
  )
}

/** Whether the canvas's shortcuts leave `event` to the control it reached. Text entry keeps every
 * key, and so does everything inside a dialog, a menu or an open list: Space presses its focused
 * button, Escape closes a menu and keeps the selection, a letter finds a menu's or a list's item,
 * and Delete never removes the layers behind it, as Koharu's canvas did from a dialog's buttons. Any
 * other input, such as a slider, a checkbox or a colour input, keeps its own keys too: Space, the
 * arrows, Enter, Delete, Escape, and every Ctrl, Cmd or Alt chord. Only a bare printable character
 * other than Space, which means nothing to such an input, goes on to switch tools. Koharu's tool
 * hotkey in CanvasWorkspace takes this rule as its `ignoreEventWhen`, and its other hotkeys skip a
 * key inside a dialog, a menu or an open list. `chord` counts the key as one typed with a chord
 * whatever its modifiers say, as the brush's `[` and `]` count a character typed with AltGraph,
 * which Windows reports as Ctrl+Alt and Chromium's key map may report alone. */
export function controlKeepsKey(event: KeyboardEvent, chord = false): boolean {
  if (isTextEntry(event.target) || inDialog(event.target)) return true
  if (!(event.target instanceof HTMLInputElement)) return false
  const toolKey = event.key.length === 1 && event.key !== ' '
  return !toolKey || chord || event.ctrlKey || event.metaKey || event.altKey
}
