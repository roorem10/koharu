'use client'

import { Bold } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { boldPatch, boldWords, isBold, typedWeight, type MixedKeys } from '@/lib/booharu/words'
import { usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { CharStyle, WordChange } from '@koharu/ipc/protocol'
import { Toggle } from '@koharu/ui/components/toggle'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** B, beside the type bar's weight list (D12): pressed while the characters it styles share a
 * weight of 600 or more. A press makes them bold, 700, mixed weights too; another makes them
 * regular, 400, or while the canvas types into a text lighter than 600, gives that text's own
 * weight back, as a typed mark can't be taken off. For words chosen in the Story panel it asks
 * the words edit as the word toolbar's B does, whose off takes the words' own weight off. The
 * renderer makes a bold face a family lacks, so B is never disabled. */
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
  const typing = useKoharuStore((state) => state.typing)
  const layers = usePage().data?.layers
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
              onChange(
                boldPatch(bold, typing ? typedWeight(layers, typing.layer) : undefined),
                boldWords(bold),
              )
            }
          />
        }
      >
        <Bold className='size-3.5' />
      </TooltipTrigger>
      <TooltipContent>{t('booharu.words.bold')}</TooltipContent>
    </Tooltip>
  )
}
