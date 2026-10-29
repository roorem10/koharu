'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { captureGroups, styleGroups } from '@/lib/text-styles'
import { commands, type EntityId, type StyleGroup, type TextStyle } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@koharu/ui/components/dialog'

export function TextStyleDialog({
  layer,
  title,
  initialGroups = captureGroups,
  onSave,
  onClose,
}: {
  layer: EntityId
  title: string
  initialGroups?: StyleGroup[]
  onSave: (style: TextStyle) => Promise<void>
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [groups, setGroups] = useState(initialGroups)
  const [busy, setBusy] = useState(false)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent showCloseButton={false}>
        <DialogTitle>{title}</DialogTitle>
        <DialogDescription>{t('textStyles.chooseGroups')}</DialogDescription>
        <div className='grid gap-3'>
          {styleGroups.map((group) => (
            <label key={group} className='flex items-center gap-3 text-sm'>
              <Checkbox
                checked={groups.includes(group)}
                disabled={busy}
                onCheckedChange={(checked) =>
                  setGroups((current) =>
                    checked ? [...current, group] : current.filter((value) => value !== group),
                  )
                }
              />
              {t(`textStyles.groups.${group}`)}
            </label>
          ))}
        </div>
        <DialogFooter>
          <Button variant='outline' disabled={busy} onClick={onClose}>
            {t('textStyles.cancel')}
          </Button>
          <Button
            disabled={busy || groups.length === 0}
            onClick={() => {
              setBusy(true)
              void call(commands.captureTextStyle, layer, groups)
                .then(onSave)
                .then(onClose)
                .catch(() => undefined)
                .finally(() => setBusy(false))
            }}
          >
            {t('textStyles.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
