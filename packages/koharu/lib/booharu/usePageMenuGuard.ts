'use client'

import { useEffect } from 'react'

/** Booharu: CEF still opens Chromium's own page menu (Back, Forward, Reload, Print, View source) on
 * a right-click anywhere in the interface; Koharu's window only places it on the screen. This keeps
 * that menu for editable text alone, where it carries cut, copy and paste, as Booharu 0.0.4 did.
 * The default is prevented on the window in the capture phase, before any element hears the click;
 * Koharu's own menus still open, because Base UI's context menu trigger opens from React's
 * onContextMenu whether or not the default was prevented. */
export function usePageMenuGuard(): void {
  useEffect(() => {
    const preventPageMenu = (event: MouseEvent) => {
      const target = event.target instanceof Element ? event.target : null
      if (target?.closest('input, textarea, [contenteditable]:not([contenteditable="false"])')) {
        return
      }
      event.preventDefault()
    }
    window.addEventListener('contextmenu', preventPageMenu, { capture: true })
    return () => window.removeEventListener('contextmenu', preventPageMenu, { capture: true })
  }, [])
}
