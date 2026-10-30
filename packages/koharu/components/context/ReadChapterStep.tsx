'use client'

import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { refreshContext, useChapterContext } from './data'
import { openContextEvent } from './events'

export function ReadChapterStep() {
  const { t } = useTranslation()
  const chapter = useChapterContext().data
  const scope = useKoharuStore((state) => state.processingScope)
  const selected = useKoharuStore((state) => state.selectedPages)
  return (
    <Button
      size='sm'
      variant='ghost'
      className='h-auto flex-wrap justify-start text-xs'
      disabled={!chapter?.supports_tasks}
      title={!chapter?.supports_tasks ? t('chapterContext.needsModel') : undefined}
      onClick={() => {
        if (!chapter?.series) {
          window.dispatchEvent(new Event(openContextEvent))
          return
        }
        void call(commands.getContextSettings)
          .then((settings) =>
            call(
              commands.readChapter,
              scope === 'selected-pages'
                ? { scope: 'pages', value: selected }
                : { scope: 'project' },
              !settings.send_images,
            ),
          )
          .then(refreshContext)
          .catch(() => undefined)
      }}
    >
      {t('chapterContext.readChapter')}
      <span className='text-[10px] text-muted-foreground'>
        {t('chapterContext.beforeTranslation')}
      </span>
    </Button>
  )
}
