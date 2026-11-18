'use client'

import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import type { BoxDetailsProps } from '@/components/booharu/details/sections'
import { AutoFontsOff, BlockCategoryPicker } from '@/components/booharu/lettering/CategoryPicker'
import {
  chosen,
  percent,
  setLetteringCategory,
  useBlockLettering,
} from '@/lib/booharu/boxLettering'
import { categoryName } from '@/lib/booharu/categories'
import type { BlockShape } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { ComboboxTrigger } from '@koharu/ui/components/combobox'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** Box details' Lettering section, for a block: its category picker, a button naming the category
 * it's lettered in, which opens Automatic and every category to pick from; why it has that
 * category; and its bubble's shape. While Pick fonts automatically is off it says a category
 * gives no font, and for a locked block, which takes no pick, why. Nothing shows until the block's
 * lettering is read, and a read that failed says why, as for a `[booharu-lettering]` the settings
 * refuse. */
export function LetteringSection({ page, box: { layer, locked } }: BoxDetailsProps) {
  const { t } = useTranslation()
  const { lettering, error } = useBlockLettering(page, layer.id)
  if (error !== null) {
    return (
      <p className='text-[11px] leading-4 break-words text-destructive'>
        {t('booharu.lettering.unread', { error })}
      </p>
    )
  }
  if (!lettering) return null

  return (
    <div className='grid gap-1'>
      <BlockCategoryPicker
        value={chosen(lettering)}
        readOnly={locked}
        onChange={(choice) => void setLetteringCategory([layer.id], choice).catch(() => undefined)}
      >
        <ComboboxTrigger
          render={
            <Button variant='outline' className='w-full min-w-0 justify-between font-normal' />
          }
          aria-label={t('booharu.lettering.category')}
          className='h-8 gap-2 px-2.5 text-[12px]'
        >
          <span className='min-w-0 truncate'>{t(categoryName(lettering.category))}</span>
        </ComboboxTrigger>
      </BlockCategoryPicker>
      <p className='text-[11px] leading-4 text-muted-foreground'>
        {t(`booharu.lettering.reasons.${lettering.reason}`)}
      </p>
      <ShapeLine shape={lettering.shape} />
      {locked ? (
        <p className='text-[11px] leading-4 text-muted-foreground'>
          {t('booharu.lettering.locked')}
        </p>
      ) : (
        <AutoFontsOff className='text-[11px]' />
      )}
    </div>
  )
}

/** A block's bubble's shape, as "Bubble shape: Round", with the measures that decided it in its
 * tooltip, which focus shows too, and in its description, for a screen reader; "No bubble" for a
 * block outside any. */
export function ShapeLine({ shape }: { shape: BlockShape | null }) {
  const { t } = useTranslation()
  const description = useId()
  if (!shape) {
    return (
      <p className='text-[11px] leading-4 text-muted-foreground'>
        {t('booharu.lettering.noBubble')}
      </p>
    )
  }
  const { rectangularity, solidity, spikes } = shape.measures
  const measures = t('booharu.lettering.measures', {
    rectangularity: percent(rectangularity),
    solidity: percent(solidity),
    spikes,
  })
  return (
    <>
      <Tooltip>
        <TooltipTrigger
          aria-describedby={description}
          render={
            <span
              tabIndex={0}
              className='justify-self-start text-[11px] leading-4 text-muted-foreground outline-none focus-visible:underline'
            />
          }
        >
          {t('booharu.lettering.bubbleShape', {
            shape: t(`booharu.lettering.shapes.${shape.shape}`),
          })}
        </TooltipTrigger>
        <TooltipContent side='left'>{measures}</TooltipContent>
      </Tooltip>
      {/* The tooltip mounts only while it shows, so the description stays mounted on its own. */}
      <span id={description} className='sr-only'>
        {measures}
      </span>
    </>
  )
}
