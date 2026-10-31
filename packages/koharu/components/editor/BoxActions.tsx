'use client'

import { useTranslation } from 'react-i18next'

import { useBoxActionInfo, useBoxEdits, useBoxesBusy } from '@/components/boxes/queries'
import type { EntityId } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

export function BoxActions({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const info = useBoxActionInfo(layer).data
  const edit = useBoxEdits()
  const running = useBoxesBusy()
  if (!info) return null

  return (
    <div className='flex flex-wrap gap-1.5'>
      {info.bubble_boxes > 1 && (
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='text-[10px]'
          disabled={running || edit.isPending}
          onClick={() => edit.mutate({ action: 'join', layer })}
        >
          {t('boxes.joinWithBubble')}
        </Button>
      )}
      {info.can_split && (
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='text-[10px]'
          title={t('boxes.splitExplanation')}
          disabled={running || edit.isPending}
          onClick={() => edit.mutate({ action: 'split', layer })}
        >
          {t('boxes.split')}
        </Button>
      )}
    </div>
  )
}
