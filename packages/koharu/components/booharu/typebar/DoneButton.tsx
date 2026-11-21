'use client'

import { useTranslation } from 'react-i18next'

import { call } from '@/lib/booharu/call'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** Done, last in the type bar while the canvas types (S11-c): it keeps what was typed, as Escape,
 * Ctrl+Enter and a click away do, as one undo step, and the text stays selected. */
export function DoneButton() {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  if (!typing) return null

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            size='xs'
            variant='secondary'
            onClick={() => void call(commands.booharuFinishTyping).catch(() => undefined)}
          />
        }
      >
        {t('booharu.typebar.done')}
      </TooltipTrigger>
      <TooltipContent>{t('booharu.typebar.doneHint')}</TooltipContent>
    </Tooltip>
  )
}
