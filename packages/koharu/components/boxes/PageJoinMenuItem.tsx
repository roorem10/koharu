'use client'

import { useTranslation } from 'react-i18next'

import type { EntityId } from '@koharu/bridge/protocol'
import { MenubarItem } from '@koharu/ui/components/menubar'

import { useBoxEdits, useBoxesBusy } from './queries'

export function PageJoinMenuItem({ page }: { page: EntityId | null }) {
  const { t } = useTranslation()
  const edit = useBoxEdits()
  const running = useBoxesBusy()
  return (
    <MenubarItem
      className='min-h-8 px-2 py-1 text-xs'
      disabled={!page || running || edit.isPending}
      onClick={() => {
        if (page) edit.mutate({ action: 'page', page })
      }}
    >
      {t('boxes.joinPage')}
    </MenubarItem>
  )
}
