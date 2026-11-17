/** Booharu: which keys a focused control keeps from the canvas's shortcuts (koharu#1181), the rule
 * Booharu 0.0.4 shipped. Koharu leaves every key to any focused input, and a slider keeps focus
 * after a click, so the tool keys stopped working until the canvas was clicked again. */

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
