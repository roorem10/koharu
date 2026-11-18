/** Booharu's keys: its key actions with their default keys, the keys Koharu's canvas fixes, which
 * no Booharu key may take, and which keys a focused control keeps from the canvas's shortcuts. */

import { toolKeys, type CanvasTool } from '@/lib/store'

/** A key as the registry names it: the modifiers it needs, in the order `ctrl`, `alt`, `shift`,
 * then the key, `event.key` in lower case or `space` for the space bar, joined by `+`, as in
 * `ctrl+shift+z`. `ctrl` also stands for Cmd, as Koharu's canvas reads either. */
export type KeyName = string

/** One of Booharu's key actions: its default key, and the tool it selects when it's a tool's key.
 * A tool's key reaches the canvas through Koharu's `toolKeys`, which holds the same letter, so
 * Koharu's key handler selects the tool as it selects its own. */
interface KeyAction {
  key: KeyName
  tool?: CanvasTool
}

/** Every Booharu key action, by its id, with its default key. */
export const keyActions = {
  /** The O tool: a box drawn with it becomes a text block that is read. */
  ocrBoxTool: { key: 'o', tool: 'ocr_box' },
} as const satisfies Record<string, KeyAction>

/** The tools Booharu adds to Koharu's canvas, whose keys are Booharu's. */
export const booharuTools: ReadonlySet<CanvasTool> = new Set(
  Object.values<KeyAction>(keyActions).flatMap(({ tool }) => (tool ? [tool] : [])),
)

/** The keys Koharu's canvas takes in CanvasWorkspace's down(), which no Booharu key may take:
 * undo, redo, select all, deselect, fit, delete, clear the selection, the hand tool held on Space,
 * and the letter of each of Koharu's tools, which selects it with or without Shift. */
export const reservedKeys: ReadonlySet<KeyName> = new Set([
  'ctrl+z',
  'ctrl+shift+z',
  'ctrl+a',
  'ctrl+d',
  'ctrl+0',
  'delete',
  'backspace',
  'escape',
  'space',
  ...(Object.entries(toolKeys) as [CanvasTool, string][])
    .filter(([tool]) => !booharuTools.has(tool))
    .flatMap(([, key]) => [key, `shift+${key}`]),
])

// Which keys a focused control keeps from the canvas's shortcuts (koharu#1181), the rule Booharu
// 0.0.4 shipped. Koharu leaves every key to any focused input, and a slider keeps focus after a
// click, so the tool keys stopped working until the canvas was clicked again.

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

/** Whether the canvas's shortcuts leave `event` to the control it reached. Text entry keeps every
 * key. Any other input, such as a slider, a checkbox or a colour input, keeps its own keys too:
 * Space, the arrows, Enter, Delete, Escape, and every Ctrl, Cmd or Alt chord. Only a bare printable
 * character other than Space, which means nothing to such an input, goes on to switch tools. */
export function controlKeepsKey(event: KeyboardEvent): boolean {
  if (isTextEntry(event.target)) return true
  if (!(event.target instanceof HTMLInputElement)) return false
  const toolKey = event.key.length === 1 && event.key !== ' '
  return !toolKey || event.ctrlKey || event.metaKey || event.altKey
}
