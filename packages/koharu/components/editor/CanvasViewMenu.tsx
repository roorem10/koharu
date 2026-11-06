'use client'

import { useTranslation } from 'react-i18next'

import { useCanvasViewStore } from '@/components/editor/viewAids'
import { formatBinding } from '@/lib/shortcuts'
import { useKoharuStore } from '@/lib/store'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import {
  MenubarCheckboxItem,
  MenubarItem,
  MenubarSeparator,
  MenubarShortcut,
} from '@koharu/ui/components/menubar'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'

export function CanvasViewMenuItems({ onGridSize }: { onGridSize: () => void }) {
  const { t } = useTranslation()
  const view = useCanvasViewStore()
  const shortcut = useKoharuStore((state) => state.shortcuts.snapping)
  return (
    <>
      <MenubarSeparator />
      <MenubarCheckboxItem
        className='min-h-8 text-xs'
        checked={view.rulers}
        onCheckedChange={view.setRulers}
      >
        {t('canvasAids.rulers')}
      </MenubarCheckboxItem>
      <MenubarCheckboxItem
        className='min-h-8 text-xs'
        checked={view.grid}
        onCheckedChange={view.setGrid}
      >
        {t('canvasAids.grid')}
      </MenubarCheckboxItem>
      <MenubarItem className='min-h-8 px-2 text-xs' onClick={onGridSize}>
        {t('canvasAids.gridSize')}
        <MenubarShortcut>{view.gridSize}</MenubarShortcut>
      </MenubarItem>
      <MenubarCheckboxItem
        className='min-h-8 text-xs'
        checked={view.snapping}
        onCheckedChange={view.setSnapping}
        title={t('canvasAids.bypassHint')}
      >
        {t('canvasAids.snapping')}
        <MenubarShortcut>{formatBinding(shortcut)}</MenubarShortcut>
      </MenubarCheckboxItem>
      <MenubarItem
        className='min-h-8 px-2 text-xs'
        disabled={!view.guides.length}
        onClick={view.clearGuides}
      >
        {t('canvasAids.clearGuides')}
      </MenubarItem>
    </>
  )
}

export function CanvasGridSettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const size = useCanvasViewStore((state) => state.gridSize)
  const setSize = useCanvasViewStore((state) => state.setGridSize)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-sm'>
        <DialogHeader>
          <DialogTitle>{t('canvasAids.gridSize')}</DialogTitle>
          <DialogDescription>{t('canvasAids.gridSizeHint')}</DialogDescription>
        </DialogHeader>
        <NumberField
          value={size}
          min={1}
          max={4096}
          step={1}
          onValueChange={(next) => {
            if (next !== null) setSize(next)
          }}
        >
          <NumberFieldGroup className='h-9'>
            <NumberFieldInput aria-label={t('canvasAids.gridSize')} />
          </NumberFieldGroup>
        </NumberField>
      </DialogContent>
    </Dialog>
  )
}
