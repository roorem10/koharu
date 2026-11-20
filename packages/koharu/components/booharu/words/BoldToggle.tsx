'use client'

import { Bold } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  boldPatch,
  boldWords,
  isBold,
  isMixed,
  unmarkTyping,
  type MixedKeys,
} from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'
import type { CharStyle, WordChange } from '@koharu/ipc/protocol'
import { Toggle } from '@koharu/ui/components/toggle'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** B, beside the type bar's weight list (D12): pressed while the characters it styles share a
 * weight of 600 or more. A press makes them bold, 700, mixed weights too; another makes whole texts
 * regular, 400. While the canvas types, B off takes the weight mark off the characters for real,
 * or sets regular where the text itself is bold (S11-d), and for words chosen in the Story panel it
 * asks the words edit as the word toolbar's B does, whose off does the same, so the three agree.
 * The renderer makes a bold face a family lacks, so B is never disabled. While the weights differ,
 * its tooltip says so, and that a press makes them all bold (S11-h). */
export function BoldToggle({
  chars,
  mixed,
  className,
  onChange,
}: {
  /** What the characters share. */
  chars: CharStyle
  /** The keys whose values they don't share. */
  mixed: MixedKeys
  className?: string
  /** Styles the characters with `patch`, as the type bar's other character settings do, or words
   * chosen in Story as `words` asks the words edit. */
  onChange: (patch: CharStyle, words: WordChange) => void
}) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  const bold = isBold(chars, mixed)

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Toggle
            variant='outline'
            size='sm'
            aria-label={t('booharu.words.bold')}
            className={className}
            pressed={bold}
            onPressedChange={() =>
              typing && bold ? unmarkTyping('bold') : onChange(boldPatch(bold), boldWords(bold))
            }
          />
        }
      >
        <Bold className='size-3.5' />
      </TooltipTrigger>
      <TooltipContent>
        {/* Or that the characters' weights differ, which a press makes bold (S11-h). */}
        {isMixed(mixed, 'weight')
          ? t('booharu.typebar.mixedToggle', { field: t('booharu.words.bold') })
          : t('booharu.words.bold')}
      </TooltipContent>
    </Tooltip>
  )
}
