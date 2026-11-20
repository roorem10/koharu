'use client'

import { RemoveFormatting } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { markedTargets } from '@/lib/booharu/typebar'
import { styleWords, unmarkTyping, type WordRange } from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'
import { commands, type Layer } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** Clear, after the type bar's star (S11-g): it takes word styles off, so the characters look as
 * their text. While the canvas types with characters selected, off those characters; with `words`,
 * the words chosen in the Story panel, off those words; otherwise off every word of the selected
 * texts, `targets`, in one step, while any of them carries a word style, leaving a locked one as it
 * is, which a notice counts. Disabled otherwise, at a caret while typing too, where nothing is
 * selected to clear. */
export function ClearWordStyles({ words, targets }: { words: WordRange | null; targets: Layer[] }) {
  const { t } = useTranslation()
  const typing = useKoharuStore((state) => state.typing !== null)
  const selected = useKoharuStore((state) => Boolean(state.typing?.selected))
  const marked = useMemo(() => markedTargets(targets).length > 0, [targets])
  const enabled = typing ? selected : words !== null || marked
  // One box is "this box", apart from the count, as a language's one form can count 21.
  const label =
    typing || words
      ? t('booharu.typebar.clearSelected')
      : targets.length === 1
        ? t('booharu.typebar.clearAll')
        : t('booharu.typebar.clearAllSelected', { count: targets.length })

  const clear = async () => {
    if (typing) return unmarkTyping('all')
    if (words) return void styleWords(words, { clear: true })
    const ids = targets.map((layer) => layer.id)
    const cleared = await call(commands.booharuClearWordStyles, ids).catch(() => null)
    if (cleared && cleared.locked > 0) {
      toast.add({
        type: 'info',
        title: t('booharu.typebar.clearedLocked', { count: cleared.locked }),
      })
    }
  }

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type='button'
            aria-label={label}
            disabled={!enabled}
            className='grid size-6 shrink-0 place-items-center rounded-md border border-input bg-background text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-50'
            onClick={() => void clear()}
          />
        }
      >
        <RemoveFormatting aria-hidden='true' className='size-3.5' />
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
