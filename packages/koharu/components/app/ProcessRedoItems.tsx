'use client'

import { useTranslation } from 'react-i18next'

import { useRedoActions } from '@/components/ocr-box/queries'
import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { MenubarItem, MenubarSeparator } from '@koharu/ui/components/menubar'

export function ProcessRedoItems() {
  const { t } = useTranslation()
  const selected = useKoharuStore((state) => state.selectedLayers)
  const page = usePage().data
  const layers = selected.filter((id) =>
    page?.layers.some((layer) => layer.id === id && layer.type === 'text'),
  )
  const actions = useRedoActions(layers)
  return (
    <>
      <MenubarSeparator />
      {(['read', 'translate', 'clean'] as const).map((action) => (
        <MenubarItem
          key={action}
          className='min-h-8 px-2 py-1 text-xs'
          closeOnClick={false}
          disabled={actions.disabled}
          onClick={() => actions.redo(action)}
        >
          {t(`ocrBox.${action}Again`)}
        </MenubarItem>
      ))}
      {actions.dialog}
    </>
  )
}
