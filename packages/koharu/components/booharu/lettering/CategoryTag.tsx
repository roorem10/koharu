'use client'

import { Hand } from 'lucide-react'
import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import {
  AutoFontsOff,
  BlockCategoryPicker,
  pickerFooter,
} from '@/components/booharu/lettering/CategoryPicker'
import { chosen, setLetteringCategory, useBlockLettering } from '@/lib/booharu/boxLettering'
import { categoryName } from '@/lib/booharu/categories'
import { isBlockLayer, type BlockLayer, type TextLayer } from '@/lib/document'
import type { LayerId, PageId } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { ComboboxTrigger } from '@koharu/ui/components/combobox'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A Story row's lettering category tag, at the top of its text column, beside its reading number
 * and ? tag: a block's category, marked with a hand when the user picked it, its tooltip saying why
 * it has it and its bubble's shape, which its description also gives a screen reader. Clicking it
 * opens the block's category picker, so a change takes two clicks; a locked block's picker opens
 * only to be read, saying why. `name` names the text as the row does. A text layer has no
 * category, and no tag, nor has a block whose category couldn't be read, as Box details says. */
export function CategoryTag({
  page,
  layer,
  name,
  locked,
}: {
  page: PageId
  layer: BlockLayer | TextLayer
  name: string
  locked: boolean
}) {
  return isBlockLayer(layer) ? (
    <BlockTag page={page} block={layer.id} name={name} locked={locked} />
  ) : null
}

function BlockTag({
  page,
  block,
  name,
  locked,
}: {
  page: PageId
  block: LayerId
  name: string
  locked: boolean
}) {
  const { t } = useTranslation()
  const description = useId()
  const { lettering, error } = useBlockLettering(page, block)
  // The row keeps the tag's line while it's read, so its fields don't move when it shows; a read
  // that failed shows no tag, and Box details says why.
  if (lettering === undefined) return error === null ? <span aria-hidden className='h-4' /> : null
  if (lettering === null) return null

  const category = t(categoryName(lettering.category))
  const reason = t(`booharu.lettering.reasons.${lettering.reason}`)
  const hand = lettering.reason === 'user'
  const { shape } = lettering
  const bubble = shape
    ? t('booharu.lettering.bubbleShape', { shape: t(`booharu.lettering.shapes.${shape.shape}`) })
    : t('booharu.lettering.noBubble')
  return (
    <BlockCategoryPicker
      value={chosen(lettering)}
      readOnly={locked}
      className='w-56'
      onChange={(choice) => void setLetteringCategory([block], choice).catch(() => undefined)}
      footer={
        locked ? (
          <p className={`${pickerFooter} text-[10px] leading-4 text-muted-foreground`}>
            {t('booharu.lettering.locked')}
          </p>
        ) : (
          <AutoFontsOff className={pickerFooter} />
        )
      }
    >
      <Tooltip>
        <TooltipTrigger
          render={
            <ComboboxTrigger
              render={<Button variant='outline' />}
              aria-label={
                name.trim()
                  ? t('booharu.lettering.tagFor', { name, category, reason })
                  : t('booharu.lettering.tag', { category, reason })
              }
              aria-describedby={description}
              data-hand={hand}
              className='h-4 max-w-full min-w-0 gap-0.5 justify-self-start rounded-full px-1.5 text-[9px] leading-none font-medium text-muted-foreground hover:border-primary/40 hover:text-foreground data-[hand=true]:border-primary/40 data-[hand=true]:bg-primary/10 data-[hand=true]:text-foreground [&>svg]:size-2.5'
            />
          }
        >
          {hand && <Hand aria-hidden />}
          <span className='min-w-0 truncate'>{category}</span>
        </TooltipTrigger>
        <TooltipContent side='left'>
          <span className='grid gap-0.5'>
            <span>{reason}</span>
            <span>{bubble}</span>
          </span>
        </TooltipContent>
      </Tooltip>
      {/* The tooltip mounts only while it shows, so a screen reader hears the shape from here. */}
      <span id={description} className='sr-only'>
        {bubble}
      </span>
    </BlockCategoryPicker>
  )
}
