import { cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { usePageMenuGuard } from '@/lib/booharu/usePageMenuGuard'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@koharu/ui/components/context-menu'

/** Whether a right-click on `target` leaves the browser free to open its own menu. */
function browserMenu(target: Element): boolean {
  const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 })
  target.dispatchEvent(event)
  return !event.defaultPrevented
}

afterEach(() => {
  cleanup()
  document.body.replaceChildren()
})

describe('Booharu page menu guard', () => {
  it('keeps the browser menu for text fields alone, until it unmounts', () => {
    const guard = renderHook(() => usePageMenuGuard())
    const panel = document.createElement('div')
    const input = document.createElement('input')
    const textarea = document.createElement('textarea')
    const editable = document.createElement('div')
    editable.setAttribute('contenteditable', 'true')
    const word = document.createElement('span')
    editable.append(word)
    const fixed = document.createElement('div')
    fixed.setAttribute('contenteditable', 'false')
    document.body.append(panel, input, textarea, editable, fixed)

    expect(browserMenu(panel)).toBe(false)
    expect(browserMenu(fixed)).toBe(false)
    expect(browserMenu(input)).toBe(true)
    expect(browserMenu(textarea)).toBe(true)
    expect(browserMenu(editable)).toBe(true)
    expect(browserMenu(word)).toBe(true)

    guard.unmount()
    expect(browserMenu(panel)).toBe(true)
  })

  it("leaves Koharu's own context menus opening", async () => {
    renderHook(() => usePageMenuGuard())
    let prevented: boolean | undefined
    render(
      <ContextMenu>
        <ContextMenuTrigger onContextMenu={(event) => (prevented = event.defaultPrevented)}>
          Page 1
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem>Delete page</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>,
    )

    fireEvent.contextMenu(screen.getByText('Page 1'), { button: 2, clientX: 20, clientY: 30 })

    // The guard had already prevented the default when the trigger heard the click.
    expect(prevented).toBe(true)
    expect(await screen.findByRole('menuitem', { name: 'Delete page' })).toBeInTheDocument()
  })
})
