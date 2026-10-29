'use client'

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { TextStyleDialog } from '@/components/editor/TextStyleDialog'
import { call, refreshTextStyles, saveTextStyles } from '@/lib/backend'
import { usePage, pageKey, pagesKey, projectKey, refresh } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { emptyTextStyle } from '@/lib/text-styles'
import { commands, type EntityId, type StyleScope, type TextStyle } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@koharu/ui/components/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@koharu/ui/components/dropdown-menu'
import { toast } from '@koharu/ui/components/toast'

export function StylesMenu({ selected }: { selected: EntityId[] }) {
  const { t } = useTranslation()
  const page = usePage().data
  const styles = useKoharuStore((state) => state.textStyles)
  const [capture, setCapture] = useState<EntityId | null>(null)
  const [confirmation, setConfirmation] = useState<{ scope: StyleScope; count: number } | null>(
    null,
  )
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (!styles) void refreshTextStyles().catch(() => undefined)
  }, [styles])

  const apply = async (scope: StyleScope, style: TextStyle) => {
    setBusy(true)
    try {
      const result = await call(commands.applyTextStyle, scope, style)
      await refresh(projectKey, pagesKey, pageKey)
      toast.add({
        type: 'success',
        title: t('textStyles.applied', { count: result.changed }),
        description: result.skipped
          ? t('textStyles.skipped', { count: result.skipped })
          : undefined,
      })
      setConfirmation(null)
    } finally {
      setBusy(false)
    }
  }
  const confirm = async (scope: StyleScope) => {
    const count = await call(commands.countTextStyleTargets, scope)
    setConfirmation({ scope, count })
  }
  const defaultStyle = styles?.default_style ?? emptyTextStyle
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={<Button variant='outline' size='sm' className='w-full' />}
          disabled={!styles || busy}
        >
          {t('textStyles.styles')}
        </DropdownMenuTrigger>
        <DropdownMenuContent className='w-72'>
          <DropdownMenuItem
            disabled={!selected.length}
            onClick={() => void apply({ layers: selected }, defaultStyle).catch(() => undefined)}
          >
            {t('textStyles.applyDefault')}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!page}
            onClick={() => {
              if (page) void confirm({ page: page.id }).catch(() => undefined)
            }}
          >
            {t('textStyles.applyPage')}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!page}
            onClick={() => void confirm('project').catch(() => undefined)}
          >
            {t('textStyles.applyProject')}
          </DropdownMenuItem>
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
      <Dialog
        open={confirmation !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setConfirmation(null)
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogTitle>{t('textStyles.applyDefault')}</DialogTitle>
          <DialogDescription>
            {t('textStyles.confirmApply', { count: confirmation?.count ?? 0 })}
          </DialogDescription>
          <DialogFooter>
            <Button variant='outline' disabled={busy} onClick={() => setConfirmation(null)}>
              {t('textStyles.cancel')}
            </Button>
            <Button
              disabled={busy || !confirmation?.count}
              onClick={() => {
                if (confirmation)
                  void apply(confirmation.scope, defaultStyle).catch(() => undefined)
              }}
            >
              {t('textStyles.apply')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
