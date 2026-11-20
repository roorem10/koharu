'use client'

import { useEffect } from 'react'
import { create } from 'zustand'

import { CANVAS_PRESS } from '@/lib/booharu/wordSelection'
import { useKoharuStore } from '@/lib/store'
import { events, type BooharuBoxMenu } from '@koharu/ipc/protocol'

// Booharu's box menu (S12-l): a right-click on a box on the canvas opens the box actions for the
// selection where the pointer pressed, as Booharu 0.0.4's canvas menu did. The desktop tells of the
// right-click with `booharu_box_menu` once the selection holds the box, after the canvas press
// it tells the page of first.

/** The menu a right-click opened: the box, its page, and where the pointer pressed, in the window's
 * CSS pixels; numbered, so each right-click opens a menu of its own, at the same point too. */
export interface ShownBoxMenu extends BooharuBoxMenu {
  serial: number
}

interface BoxMenuState {
  /** The menu showing, if one is. */
  shown: ShownBoxMenu | null
}

/** The box menu's state, the interface's own. */
export const useBoxMenu = create<BoxMenuState>()(() => ({ shown: null }))

/** The number the last menu opened took. */
let opened = 0

/** Opens the box menu a right-click asked for, in place of any showing. */
export function openBoxMenu(menu: BooharuBoxMenu): void {
  opened += 1
  useBoxMenu.setState({ shown: { ...menu, serial: opened } })
}

/** Closes the box menu. */
export function closeBoxMenu(): void {
  if (useBoxMenu.getState().shown) useBoxMenu.setState({ shown: null })
}

/** Opens the box menu at each right-click on a box the desktop tells of while the editor shows,
 * and closes it as the editor goes. */
export function useBoxMenuEvents(): void {
  useEffect(() => {
    let active = true
    const listening = events.booharuBoxMenu((menu) => {
      if (active) openBoxMenu(menu)
    })
    return () => {
      active = false
      closeBoxMenu()
      void listening.then((stop) => stop()).catch(() => undefined)
    }
  }, [])
}

/** While the box menu is `open`, closes it on what moves the canvas from under it, or leaves it
 * behind: a press on the canvas, which the page never sees, the desktop telling the page of it with
 * `CANVAS_PRESS`; a move of the camera, a zoom or a scroll; and the window losing the focus. */
export function useBoxMenuDismissal(open: boolean): void {
  useEffect(() => {
    if (!open) return
    window.addEventListener(CANVAS_PRESS, closeBoxMenu)
    window.addEventListener('blur', closeBoxMenu)
    const stop = useKoharuStore.subscribe((state, previous) => {
      if (state.camera !== previous.camera) closeBoxMenu()
    })
    return () => {
      window.removeEventListener(CANVAS_PRESS, closeBoxMenu)
      window.removeEventListener('blur', closeBoxMenu)
      stop()
    }
  }, [open])
}
