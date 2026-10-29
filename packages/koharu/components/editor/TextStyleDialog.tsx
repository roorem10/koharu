'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
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
import { Input } from '@koharu/ui/components/input'

export function TextStyleDialog({
  layer,
  title,
  initialGroups = captureGroups,
  initialName,
  presetId,
  onSave,
  onClose,
}: {
  layer: EntityId
  title: string
  initialGroups?: StyleGroup[]
  initialName?: string
  presetId?: string
  onSave: (style: TextStyle, name: string) => Promise<void>
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [groups, setGroups] = useState(initialGroups)
  const [busy, setBusy] = useState(false)
  const [name, setName] = useState(initialName ?? '')
  const presets = useKoharuStore((state) => state.textStyles?.presets)
  const validName =
    initialName === undefined ||
    (!!name.trim() && [...name.trim()].length <= 64 && !/\p{Cc}/u.test(name))
  const duplicate =
    initialName !== undefined &&
    presets?.some(
      (preset) =>
        preset.id !== presetId &&
        preset.name.normalize('NFKC').trim().toLowerCase() ===
          name.normalize('NFKC').trim().toLowerCase(),
    )
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
        {initialName !== undefined && (
          <label className='grid gap-2 text-sm'>
            {t('textStyles.presetName')}
            <Input
              aria-label={t('textStyles.presetName')}
              value={name}
              disabled={busy}
              aria-invalid={!validName}
              onChange={(event) => setName(event.target.value)}
            />
            {!validName && (
              <span role='status' className='text-xs text-destructive'>
                {t('textStyles.nameHelp')}
              </span>
            )}
            {duplicate && (
              <span role='status' className='text-xs text-muted-foreground'>
                {t('textStyles.duplicateName')}
              </span>
            )}
          </label>
        )}
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
            disabled={busy || groups.length === 0 || !validName}
            onClick={() => {
              setBusy(true)
              void call(commands.captureTextStyle, layer, groups)
                .then((style) => onSave(style, name.trim()))
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
