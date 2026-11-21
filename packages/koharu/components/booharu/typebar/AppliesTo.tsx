'use client'

import { useTranslation } from 'react-i18next'

import { call } from '@/lib/booharu/call'
import { wordsQuote } from '@/lib/booharu/typebar'
import { setWordRange, type WordRange } from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A small text button of the type bar's, as the Story panel's word toolbar draws its buttons. */
const BUTTON =
  'grid h-6 min-w-6 shrink-0 place-items-center rounded-md border border-input bg-background px-1.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground'

/** What the type bar's character settings apply to, first in the bar (S11-f): while the canvas
 * types with characters selected, those characters, as the canvas quotes them (`Typed.selected`);
 * with `words`, the words chosen in the Story panel, quoted the same way. The quote is cut to the
 * bar's room, its whole shown in a tooltip, which the page draws, as the desktop never shows a
 * `title`. Whole box goes back to the whole box: while typing it keeps what was typed, as Done
 * does, and the text stays selected; for words chosen in Story it lets them go. Nothing at a caret,
 * where the settings are the next characters', or with nothing chosen. */
export function AppliesTo({ words }: { words: WordRange | null }) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  const selected = useKoharuStore((state) => state.typing?.selected || null)
  const quote = typing ? selected : words && wordsQuote(words)
  if (!quote) return null

  const label = t('booharu.typebar.appliesTo', { text: quote })
  const wholeBox = () =>
    typing ? void call(commands.booharuFinishTyping).catch(() => undefined) : setWordRange(null)

  return (
    <div className='flex max-w-48 min-w-0 items-center gap-1'>
      <Tooltip>
        <TooltipTrigger
          render={<span className='min-w-0 truncate text-[11px] text-muted-foreground' />}
        >
          {label}
        </TooltipTrigger>
        <TooltipContent className='break-words'>{label}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger render={<button type='button' className={BUTTON} onClick={wholeBox} />}>
          {t('booharu.typebar.wholeBox')}
        </TooltipTrigger>
        <TooltipContent>{t('booharu.typebar.wholeBoxHint')}</TooltipContent>
      </Tooltip>
    </div>
  )
}

/** The note a paragraph control's tooltip gains while the type bar's character settings apply to
 * characters typed or words chosen in Story, `words`, which the paragraph's never do: it's the
 * whole box's (S11-f). Nothing otherwise. */
export function WholeBoxOnly({ words }: { words: WordRange | null }) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  if (!typing && !words) return null
  return <span className='ml-2 opacity-60'>{t('booharu.typebar.wholeBoxOnly')}</span>
}
