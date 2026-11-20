'use client'

import type { SyntheticEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { effectsOf, joinNames, useTextEffects } from '@/lib/booharu/effects'
import { openFxOn } from '@/lib/booharu/fx'
import type { LayerId, PageId } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A press of the tag stays its own, so its row neither starts a drag nor selects. */
const own = (event: SyntheticEvent) => event.stopPropagation()

/** Whether `layer` of `page` carries a text effect, as its fx tag shows it. */
export function useHasEffects(page: PageId | null | undefined, layer: LayerId): boolean {
  return effectsOf(useTextEffects(page).data, layer).length > 0
}

/** A text's fx tag (S12-j), beside its ? tag on its Story row and its Layers row, while the text
 * carries a text effect beyond its lettering default, as the page's text effects tell it: a border
 * or second border unlike the default's, the outline method, a gradient fill or a skew. An italic
 * "fx", named "Text effects for" the text as its row names it, its tooltip listing the effects in
 * the interface language. A click selects the text, as showLayer does, switches to the Select tool
 * unless one of the type bar's tools is on, and opens Text effects in the type bar. */
export function FxTag({ page, layer, name }: { page: PageId; layer: LayerId; name: string }) {
  const { t, i18n } = useTranslation()
  const effects = effectsOf(useTextEffects(page).data, layer)
  if (!effects.length) return null
  const list = joinNames(
    effects.map((effect) => t(`booharu.effects.names.${effect}`)),
    i18n.language,
  )
  const label = name.trim() ? t('booharu.effects.tag', { name }) : t('booharu.effects.title')
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            type='button'
            variant='outline'
            aria-label={label}
            className='size-4 rounded-full px-0 font-serif text-[9px] leading-none text-muted-foreground italic hover:border-primary/40 hover:text-foreground'
            onPointerDown={own}
            onClick={(event) => {
              own(event)
              void openFxOn(page, [layer])
            }}
          />
        }
      >
        fx
      </TooltipTrigger>
      <TooltipContent side='left'>{t('booharu.effects.tagList', { list })}</TooltipContent>
    </Tooltip>
  )
}
