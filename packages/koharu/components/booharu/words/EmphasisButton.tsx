'use client'

import { Star } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { styleWords, useWordRange } from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'
import { commands, type LayerId } from '@koharu/ipc/protocol'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** The emphasis star, after the type bar's I (D14): it stresses words as Settings > Typesetting >
 * Emphasis has them look in their text's lettering category. While the canvas types into a text,
 * the selected characters take the emphasis, or at a caret the next ones typed; with words chosen
 * in the Story panel in `layer`, the text the bar shows, those words do, through the words edit.
 * Otherwise it's disabled, so it never restyles a whole box; so it is for a new text, which has no
 * layer, and so no category, until it's committed. As in 0.0.4 it has no off: Clear takes the
 * words' own style off. */
export function EmphasisButton({ layer }: { layer: LayerId | undefined }) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  const typed = useKoharuStore((state) => state.typing?.layer ?? null)
  const range = useWordRange((state) => state.range)
  const words = !typing && range && range.layer === layer ? range : null

  const press = async () => {
    if (typed !== null) {
      const patch = await call(commands.booharuEmphasisPatch, typed).catch(() => null)
      // The patch is that text's: none for another the canvas went on to type into meanwhile. Its
      // unset values are nulls, which the canvas leaves as they are.
      if (patch && useKoharuStore.getState().typing?.layer === typed) {
        await call(commands.styleTyping, patch).catch(() => undefined)
      }
    } else if (words) {
      await styleWords(words, { emphasis: true })
    }
  }

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type='button'
            aria-label={t('booharu.words.emphasis')}
            disabled={typed === null && words === null}
            className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-50'
            onClick={() => void press()}
          />
        }
      >
        <Star aria-hidden='true' className='size-3.5' />
      </TooltipTrigger>
      <TooltipContent>{t('booharu.words.emphasis')}</TooltipContent>
    </Tooltip>
  )
}
