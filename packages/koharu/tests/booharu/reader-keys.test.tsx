import { QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { Reader } from '@/components/editor/Reader'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/ipc/protocol'

import { desktop, openChapter, settle } from './reader'

/** A key for each of the reader's four hotkeys, with the page it shows from the third of five:
 * Space turns forward, Page Up back, Home goes to the first page and End to the last. */
const turns: [KeyboardEventInit, string][] = [
  [{ key: ' ', code: 'Space' }, 'p4'],
  [{ key: 'PageUp', code: 'PageUp' }, 'p2'],
  [{ key: 'Home', code: 'Home' }, 'p1'],
  [{ key: 'End', code: 'End' }, 'p5'],
]

/** Where a key lands while something is open over the reader: a title-bar menu that took the
 * focus itself, and a button of a dialog, as the Run popover's and Settings' are. */
function opened(): [string, HTMLElement][] {
  const menu = document.createElement('div')
  menu.setAttribute('role', 'menu')
  menu.tabIndex = -1
  const dialog = document.createElement('div')
  dialog.setAttribute('role', 'dialog')
  const button = document.createElement('button')
  dialog.append(button)
  document.body.append(menu, dialog)
  return [
    ['menu', menu],
    ['dialog button', button],
  ]
}

beforeEach(async () => {
  desktop()
  openChapter(5, 'p3')
  render(
    <QueryClientProvider client={queryClient}>
      <Reader />
    </QueryClientProvider>,
  )
  // What it sends as it mounts arrives a few promises later.
  await settle()
  vi.mocked(commands.show).mockClear()
})

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe("Booharu's key rules in Koharu's reader", () => {
  it('turns pages with Space, Page Up, Home and End pressed on the page', async () => {
    for (const [init, page] of turns) {
      const kept = fireEvent.keyDown(document.body, init)
      await settle()
      expect(vi.mocked(commands.show).mock.calls, init.code).toEqual([[page, null]])
      // Prevented: the reader took it.
      expect(kept, init.code).toBe(false)
      vi.mocked(commands.show).mockClear()
    }
  })

  it('leaves those keys to a menu or a dialog open over it, turning no page', async () => {
    for (const [where, target] of opened()) {
      for (const [init] of turns) {
        const kept = fireEvent.keyDown(target, init)
        await settle()
        expect(vi.mocked(commands.show).mock.calls, `${where} ${init.code}`).toEqual([])
        // Not prevented either: Space presses the dialog's button, the arrows and Home and End
        // move through the menu.
        expect(kept, `${where} ${init.code}`).toBe(true)
      }
    }
  })
})
