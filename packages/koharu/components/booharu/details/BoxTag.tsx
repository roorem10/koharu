'use client'

import { type SyntheticEvent, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { qcSection, useLayerQcCount } from '@/lib/booharu/qc'
import { isBlockLayer, isTextLayer } from '@/lib/document'
import { usePage } from '@/lib/queries'
import type { Layer, LayerId, PageId } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A text's ? tag, on its Story row under its reading number, and on its Layers row through
 * `LayerBoxTag`: it opens Box details for the text, named `name` as the row names it. While the
 * quality check has something open on the text, the tag is Booharu's warning yellow and blinks, as
 * app/booharu.css draws it, says how many in its name and tooltip, and opens Box details on its QC
 * section. With `reveal`, Story's alone, it also scrolls its row into view when Show in Story asks
 * for it: the first tag to see that request clears it. */
export function BoxTag({
  page,
  layer,
  name,
  reveal = true,
}: {
  page: PageId
  layer: LayerId
  name: string
  reveal?: boolean
}) {
  const { t } = useTranslation()
  const tag = useRef<HTMLButtonElement>(null)
  const issues = useLayerQcCount(page, layer)
  const shown = useBoxDetails((state) =>
    reveal && state.reveal?.layer === layer ? state.reveal : null,
  )

  useEffect(() => {
    if (!shown || performance.now() > shown.until) return
    // A frame later, once Story has scrolled to the row's page.
    const frame = requestAnimationFrame(() => {
      const element = tag.current
      ;(element?.closest('[data-selected]') ?? element)?.scrollIntoView({ block: 'nearest' })
      useBoxDetails.setState({ reveal: null })
    })
    return () => cancelAnimationFrame(frame)
  }, [shown])

  const title = issues ? t('booharu.details.issues', { count: issues }) : t('booharu.details.title')
  let label = title
  if (name.trim()) {
    label = issues
      ? t('booharu.details.detailsIssues', { name, count: issues })
      : t('booharu.details.titleFor', { name })
  }
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            ref={tag}
            type='button'
            variant='outline'
            aria-label={label}
            className={`size-4 rounded-full px-0 text-[9px] leading-none font-semibold ${
              issues
                ? 'booharu-qc-tag booharu-qc-blink'
                : 'text-muted-foreground hover:border-primary/40 hover:text-foreground'
            }`}
            onClick={() => openBoxDetails(page, layer, issues ? qcSection : undefined)}
          />
        }
      >
        ?
      </TooltipTrigger>
      <TooltipContent side='left'>{title}</TooltipContent>
    </Tooltip>
  )
}

/** A press of the Layers tag stays its own, so the row neither starts a drag nor selects. */
const own = (event: SyntheticEvent) => event.stopPropagation()

/** A Layers row's ? tag, for a block or a text layer on the page the canvas shows, which Layers
 * lists, and only while the quality check has something open on it: Layers keeps its rows as Koharu
 * draws them otherwise. It scrolls nothing for Show in Story, which goes to Story's row. */
export function LayerBoxTag({ layer, name }: { layer: Layer; name: string }) {
  const page = usePage().data?.id
  const issues = useLayerQcCount(page, layer.id)
  if (!page || !issues || !(isBlockLayer(layer) || isTextLayer(layer))) return null
  return (
    <span className='grid size-6 shrink-0 place-items-center' onPointerDown={own} onClick={own}>
      <BoxTag page={page} layer={layer.id} name={name} reveal={false} />
    </span>
  )
}
