'use client'

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { TextStyleDialog } from '@/components/editor/TextStyleDialog'
import { refreshTextStyles, saveTextStyles } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import type { EntityId } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@koharu/ui/components/dropdown-menu'

export function StylesMenu({ selected }: { selected: EntityId[] }) {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  const [capture, setCapture] = useState<EntityId | null>(null)
  useEffect(() => {
    if (!styles) void refreshTextStyles().catch(() => undefined)
  }, [styles])
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={<Button variant='outline' size='sm' className='w-full' />}
          disabled={!styles}
        >
          {t('textStyles.styles')}
        </DropdownMenuTrigger>
        <DropdownMenuContent className='w-64'>
          <DropdownMenuItem disabled={!selected.length} onClick={() => setCapture(selected[0]!)}>
            {t('textStyles.setDefault')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {capture && (
        <TextStyleDialog
          layer={capture}
          title={t('textStyles.setDefault')}
          onClose={() => setCapture(null)}
          onSave={async (default_style) => {
            const current = useKoharuStore.getState().textStyles
            if (current) await saveTextStyles({ ...current, default_style })
          }}
        />
      )}
    </>
  )
}
