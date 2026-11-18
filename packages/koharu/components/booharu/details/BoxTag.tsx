'use client'

import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import type { LayerId, PageId } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A Story row's ? tag, under its reading number: it opens Box details for the row's text, named
 * `name` as the row names it, and scrolls the row into view when Show in Story asks for it. */
export function BoxTag({ page, layer, name }: { page: PageId; layer: LayerId; name: string }) {
  const { t } = useTranslation()
  const tag = useRef<HTMLButtonElement>(null)
  const reveal = useBoxDetails((state) => (state.reveal?.layer === layer ? state.reveal : null))

  useEffect(() => {
    if (!reveal || performance.now() > reveal.until) return
    // A frame later, once Story has scrolled to the row's page.
    const frame = requestAnimationFrame(() => {
      const element = tag.current
      ;(element?.closest('[data-selected]') ?? element)?.scrollIntoView({ block: 'nearest' })
      useBoxDetails.setState({ reveal: null })
    })
    return () => cancelAnimationFrame(frame)
  }, [reveal])

  const title = t('booharu.details.title')
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            ref={tag}
            type='button'
            variant='outline'
            aria-label={name.trim() ? t('booharu.details.titleFor', { name }) : title}
            className='size-4 rounded-full px-0 text-[9px] leading-none font-semibold text-muted-foreground hover:border-primary/40 hover:text-foreground'
            onClick={() => openBoxDetails(page, layer)}
          />
        }
      >
        ?
      </TooltipTrigger>
      <TooltipContent side='left'>{title}</TooltipContent>
    </Tooltip>
  )
}
