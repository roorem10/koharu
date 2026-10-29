'use client'

import { useQuery } from '@tanstack/react-query'
import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useProject } from '@/lib/queries'
import { commands, type EntityId } from '@koharu/bridge/protocol'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

export function BubbleShapeLine({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const tooltipId = useId()
  const revision = useProject().data?.revision
  const info = useQuery({
    queryKey: ['bubble-shape', layer, revision],
    queryFn: () => call(commands.getTextBubbleShape, layer),
  })
  const kind = info.data
    ? info.data.bubble === null
      ? 'free'
      : (info.data.shape ?? 'unknown')
    : 'unknown'
  const measures = info.data?.measures
  return (
    <Tooltip>
      <TooltipTrigger
        aria-describedby={tooltipId}
        render={<span tabIndex={0} className='block text-[10px] text-muted-foreground' />}
      >
        {t('textStyles.bubbleShape', {
          shape: info.isPending ? t('common.loading') : t(`textStyles.shapes.${kind}`),
        })}
      </TooltipTrigger>
      <TooltipContent side='left' id={tooltipId} role='tooltip'>
        {measures
          ? t('textStyles.shapeMeasures', {
              rectangularity: (measures.rectangularity * 100).toFixed(1),
              solidity: (measures.solidity * 100).toFixed(1),
              spikes: measures.spikes,
            })
          : t('textStyles.measuresUnavailable')}
      </TooltipContent>
    </Tooltip>
  )
}
