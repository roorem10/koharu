'use client'

import { useTranslation } from 'react-i18next'

import { useSetSfxHandling, useSfxHandling } from '@/components/lettering/sfx-queries'
import { type EntityId, type SfxAction } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

export function SfxHandlingLine({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const handling = useSfxHandling(layer)
  const save = useSetSfxHandling()
  if (handling.data === null) return null
  return (
    <div className='grid min-w-0 gap-1'>
      <Select
        value={handling.data?.automatic ? 'automatic' : (handling.data?.action ?? 'automatic')}
        disabled={!handling.data || save.isPending}
        onValueChange={(value) => {
          if (value !== null)
            save.mutate({ layer, action: value === 'automatic' ? null : (value as SfxAction) })
        }}
      >
        <SelectTrigger size='sm' className='w-full min-w-0' aria-label={t('lettering.sfxHandling')}>
          <SelectValue>
            {handling.data
              ? t(`lettering.sfxActions.${handling.data.action}`)
              : t('lettering.loading')}
          </SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectItem value='automatic'>{t('lettering.automatic')}</SelectItem>
          {(['skip', 'subtitle', 'translate'] as const).map((action) => (
            <SelectItem key={action} value={action}>
              {t(`lettering.sfxActions.${action}`)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <span className='text-[10px] leading-4 text-muted-foreground'>
        {handling.isError
          ? t('lettering.loadError')
          : handling.data
            ? t(`lettering.sfxReasons.${handling.data.reason}`)
            : ''}
      </span>
      {handling.data && (
        <span className='text-[10px] leading-4 text-muted-foreground'>
          {t('lettering.sfxHandlingHelp')}
        </span>
      )}
    </div>
  )
}
