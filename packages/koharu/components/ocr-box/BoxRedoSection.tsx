'use client'

import { useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useLineCategory } from '@/lib/line-labels'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { ocrBoxEditKey, refreshOcrBoxes, useRedoActions } from './queries'

export function BoxRedoSection({ layers }: { layers: EntityId[] }) {
  const { t } = useTranslation()
  const actions = useRedoActions(layers)
  const layer = layers.length === 1 ? layers[0] : undefined
  const category = useLineCategory(layer)
  const subtitle = useMutation({
    mutationKey: ocrBoxEditKey,
    mutationFn: async (layer: EntityId) => {
      const page = useKoharuStore.getState().canvasPage
      const added = await call(commands.addSfxSubtitle, layer)
      if (useKoharuStore.getState().canvasPage === page) {
        useKoharuStore.getState().selectLayers([added])
      }
    },
    onSettled: refreshOcrBoxes,
  })
  return (
    <>
      <div className='flex min-w-0 flex-wrap gap-1.5'>
        {(['read', 'translate', 'clean'] as const).map((action) => (
          <Button
            key={action}
            type='button'
            size='sm'
            variant='outline'
            className='text-[10px]'
            disabled={actions.disabled}
            onClick={() => actions.redo(action)}
          >
            {t(`ocrBox.${action}Again`)}
          </Button>
        ))}
        {layer && category.data?.value === 'sfx' && (
          <Button
            type='button'
            size='sm'
            variant='outline'
            className='text-[10px]'
            disabled={actions.disabled || subtitle.isPending}
            onClick={() => subtitle.mutate(layer)}
          >
            {t('ocrBox.addSubtitle')}
          </Button>
        )}
      </div>
      {actions.dialog}
    </>
  )
}
