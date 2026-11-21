'use client'

import { useTranslation } from 'react-i18next'

import { call } from '@/lib/booharu/call'
import { styleWords, type WordRange } from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A small text button of the type bar's, as the Story panel's word toolbar draws its buttons. */
const BUTTON =
  'grid h-6 min-w-6 shrink-0 place-items-center rounded-md border border-input bg-background px-1.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground'

/** A− and A+, after the type bar's size field (S11-e): each click makes the characters the canvas
 * types, selected or at a caret the next ones typed, or the words chosen in the Story panel,
 * `words`, a step smaller or bigger, each from its own size, so words of different sizes keep
 * their order, in one call. The box's fit is never touched. Only while typing or with words
 * chosen, as the size field sizes a whole box. */
export function WordSizeSteps({ words }: { words: WordRange | null }) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  if (!typing && !words) return null

  const step = (bigger: boolean) => {
    if (typing) return void call(commands.booharuStepTypingSize, bigger).catch(() => undefined)
    if (words) void styleWords(words, { step: bigger ? 'bigger' : 'smaller' })
  }

  return (
    <>
      {(
        [
          [false, t('booharu.typebar.smaller'), 'A−'],
          [true, t('booharu.typebar.bigger'), 'A+'],
        ] as const
      ).map(([bigger, label, mark]) => (
        <Tooltip key={mark}>
          <TooltipTrigger
            render={
              <button
                type='button'
                aria-label={label}
                className={BUTTON}
                onClick={() => step(bigger)}
              />
            }
          >
            <span aria-hidden='true'>{mark}</span>
          </TooltipTrigger>
          <TooltipContent>{label}</TooltipContent>
        </Tooltip>
      ))}
    </>
  )
}
