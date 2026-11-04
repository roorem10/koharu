'use client'

import { Menu } from '@base-ui/react/menu'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { useBoxActionInfo, useBoxEdits, useBoxesBusy } from '@/components/boxes/queries'
import { originalVisible, useCompareStore } from '@/components/qc/compare'
import { useRedesignStore } from '@/components/redesign/state'
import { expandLayerSelection, isTextLayer } from '@/lib/document'
import { hitTestLayers } from '@/lib/geometry'
import { usePage, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { EntityId, Point } from '@koharu/bridge/protocol'
import { DropdownMenu, DropdownMenuItem } from '@koharu/ui/components/dropdown-menu'

type MenuTarget = { x: number; y: number; layer: EntityId; targets: EntityId[]; page: EntityId }

export function useCanvasContextMenu(): {
  open: (event: MouseEvent, point: Point) => void
  menu: ReactNode
} {
  const page = usePage().data
  const project = useProject().data?.name
  const compare = useCompareStore((state) => state.pinned || state.held)
  const [target, setTarget] = useState<MenuTarget | null>(null)
  const activePointers = useRef(new Set<number>())
  useEffect(() => {
    const start = (event: PointerEvent) => {
      if (
        event.button <= 1 &&
        event.target instanceof Element &&
        event.target.closest('[aria-busy]')
      )
        activePointers.current.add(event.pointerId)
    }
    const end = (event: PointerEvent) => activePointers.current.delete(event.pointerId)
    const blur = () => {
      activePointers.current.clear()
      setTarget(null)
    }
    document.addEventListener('pointerdown', start, true)
    document.addEventListener('pointerup', end, true)
    document.addEventListener('pointercancel', end, true)
    window.addEventListener('blur', blur)
    return () => {
      document.removeEventListener('pointerdown', start, true)
      document.removeEventListener('pointerup', end, true)
      document.removeEventListener('pointercancel', end, true)
      window.removeEventListener('blur', blur)
    }
  }, [])
  useEffect(() => {
    setTarget(null)
  }, [page?.id, project, compare])
  useEffect(() => {
    if (!target) return
    const close = () => setTarget(null)
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close()
    }
    window.addEventListener('scroll', close, true)
    window.addEventListener('wheel', close, true)
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('scroll', close, true)
      window.removeEventListener('wheel', close, true)
      window.removeEventListener('keydown', escape)
    }
  }, [target])

  return {
    open: (event, point) => {
      event.preventDefault()
      setTarget(null)
      if (
        event.button !== 2 ||
        (event.buttons & 5) !== 0 ||
        activePointers.current.size ||
        originalVisible() ||
        !page
      )
        return
      const state = useKoharuStore.getState()
      const layer = hitTestLayers(page.layers, point, state.layerFrames)
      if (!layer || !isTextLayer(layer)) return
      const expanded = expandLayerSelection(page.layers, state.selectedLayers)
      if (!expanded.includes(layer.id)) state.selectLayers([layer.id])
      const selection = expanded.includes(layer.id) ? expanded : [layer.id]
      const targets = page.layers
        .filter(isTextLayer)
        .filter((layer) => selection.includes(layer.id))
        .map((layer) => layer.id)
      setTarget({ x: event.clientX, y: event.clientY, layer: layer.id, targets, page: page.id })
    },
    menu:
      target && target.page === page?.id && !compare ? (
        <CanvasMenu target={target} close={() => setTarget(null)} />
      ) : null,
  }
}

function CanvasMenu({ target, close }: { target: MenuTarget; close: () => void }) {
  const { t } = useTranslation()
  const info = useBoxActionInfo(target.layer).data
  const edit = useBoxEdits()
  const running = useBoxesBusy()
  const openFx = useRedesignStore((state) => state.openFx)
  const openDetails = useRedesignStore((state) => state.openDetails)
  const action = (run: () => void) => {
    close()
    run()
  }
  return (
    <DropdownMenu
      open
      onOpenChange={(open) => {
        if (!open) close()
      }}
      modal={false}
    >
      <Menu.Portal>
        <Menu.Positioner
          anchor={{ getBoundingClientRect: () => new DOMRect(target.x, target.y, 0, 0) }}
          align='start'
          className='z-50 outline-none'
        >
          <Menu.Popup
            className='min-w-48 rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10 outline-none'
            aria-label={t('redesign.boxMenu')}
          >
            <DropdownMenuItem onClick={() => action(() => openFx(target.targets))}>
              {t('redesign.fxMenu')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => action(() => openDetails(target.layer))}>
              {t('redesign.boxDetails')}
            </DropdownMenuItem>
            {info?.can_split && (
              <DropdownMenuItem
                disabled={running || edit.isPending}
                onClick={() => action(() => edit.mutate({ action: 'split', layer: target.layer }))}
              >
                {t('boxes.split')}
              </DropdownMenuItem>
            )}
            {(info?.bubble_boxes ?? 0) > 1 && (
              <DropdownMenuItem
                disabled={running || edit.isPending}
                onClick={() => action(() => edit.mutate({ action: 'join', layer: target.layer }))}
              >
                {t('boxes.joinWithBubble')}
              </DropdownMenuItem>
            )}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </DropdownMenu>
  )
}
