'use client'

import { useEffect, useMemo, useRef, type ComponentProps } from 'react'
import { useTranslation } from 'react-i18next'

import { SelectionBoxActions } from '@/components/booharu/boxes/BoxActionItems'
import {
  closeBoxMenu,
  dismissBoxMenu,
  useBoxMenu,
  useBoxMenuDismissal,
  useBoxMenuEvents,
  type ShownBoxMenu,
} from '@/lib/booharu/boxMenu'
import { findLayer } from '@/lib/document'
import { usePage, useProject } from '@/lib/queries'
import type { Page } from '@koharu/ipc/protocol'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
} from '@koharu/ui/components/dropdown-menu'
import { cn } from '@koharu/ui/lib/utils'

/** The box menu (S12-l): the menu a right-click on a box on the canvas opens where the pointer
 * pressed, "Box actions", listing the box actions for the selection, which holds the box by then,
 * FX… first, as the Edit menu lists them. It leaves the page usable around it, and closes on
 * Escape, which keeps the selection, a press outside it, on the canvas too, an item run, the page
 * or the project changing, the camera moving, the window losing the focus, or its box leaving the
 * page. Its keys stay its own, as a dialog's do, so Delete deletes nothing
 * (lib/booharu/keys.ts), and go back to the canvas as it closes with nothing pressed on the page
 * (dismissBoxMenu). */
export function BoxMenu() {
  useBoxMenuEvents()
  const shown = useBoxMenu((state) => state.shown)
  const page = usePage().data ?? null
  const project = useProject().data?.name
  useBoxMenuDismissal(shown !== null)

  const there = shown !== null && page?.id === shown.page
  const gone = there && findLayer(page.layers, shown.layer) === undefined
  useEffect(() => {
    if (shown && (!there || gone)) dismissBoxMenu()
  }, [shown, there, gone])

  const last = useRef(project)
  useEffect(() => {
    if (last.current !== project) dismissBoxMenu()
    last.current = project
  }, [project])

  if (!shown || !there || gone || !page) return null
  return <ShownMenu key={shown.serial} menu={shown} page={page} />
}

/** The menu `menu` opened, over `page`, the page the canvas shows, at the point its right-click
 * pressed. */
function ShownMenu({ menu, page }: { menu: ShownBoxMenu; page: Page }) {
  const { t } = useTranslation()
  const { x, y } = menu
  // A point to open beside, as a context menu opens: its top left corner at the pointer.
  const anchor = useMemo(
    () => ({
      getBoundingClientRect: () => ({
        x,
        y,
        width: 0,
        height: 0,
        top: y,
        right: x,
        bottom: y,
        left: x,
      }),
    }),
    [x, y],
  )
  return (
    <DropdownMenu
      open
      modal={false}
      onOpenChange={(open, { reason }) => {
        if (open) return
        // Escape gives the canvas back the keys the menu took; a press on the page, an item run or
        // the focus moving on leave them on the page.
        if (reason === 'escape-key') dismissBoxMenu()
        else closeBoxMenu()
      }}
    >
      <DropdownMenuContent
        anchor={anchor}
        side='bottom'
        align='start'
        sideOffset={0}
        finalFocus={false}
        aria-label={t('booharu.boxes.menu')}
        className='w-auto min-w-44 p-1'
      >
        <SelectionBoxActions page={page} Item={BoxMenuItem} />
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** An item sized as the Edit menu's. */
function BoxMenuItem({ className, ...props }: ComponentProps<typeof DropdownMenuItem>) {
  return (
    <DropdownMenuItem
      className={cn(
        "min-h-8 gap-1.5 px-2 py-1 text-xs [&_svg:not([class*='size-'])]:size-3.5",
        className,
      )}
      {...props}
    />
  )
}
