'use client'

import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { ModelPicker } from '@/components/controls/ModelPicker'
import { call } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { commands, type ModelSelection } from '@koharu/bridge/protocol'

import type { Serialized } from './types'

export function ReadingModelPicker({
  value,
  onSelect,
  onBack,
}: {
  value: ModelSelection | null
  onSelect: (model: Serialized<ModelSelection>) => void
  onBack: () => void
}) {
  const { t } = useTranslation()
  const providers = useKoharuStore((state) => state.preferences?.providers.entries) ?? []
  const models = useQuery({
    queryKey: ['reading-models', providers],
    queryFn: () => call(commands.getReadingModels),
  })
  return (
    <div className='grid gap-2'>
      <ModelPicker
        value={value}
        models={models.data ?? []}
        providers={providers}
        loading={models.isPending}
        onBack={onBack}
        onSelect={(model) =>
          onSelect({
            provider: model.provider,
            model: model.model ?? null,
            quantization: model.quantization ?? null,
            vision: model.vision ?? false,
            reasoning: model.reasoning ?? false,
          })
        }
      />
      <p className='text-[11px] leading-relaxed text-muted-foreground'>
        {t('chapterContext.readerDownload')}
      </p>
    </div>
  )
}
