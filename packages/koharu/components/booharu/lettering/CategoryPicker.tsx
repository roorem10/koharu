'use client'

import { useTranslation } from 'react-i18next'

import { categoryName, letteringCategories } from '@/lib/booharu/categories'
import type { LetteringCategory } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  ComboboxValue,
} from '@koharu/ui/components/combobox'

/** A searchable picker of the lettering categories, named `label`: a button naming the chosen one,
 * `value`, which opens the list of every category in Settings' order under a search line, as the
 * font picker opens its fonts. Typing filters the list by the names it shows, the first match
 * marked; the arrows move through it and Enter picks, as a click does, calling `onChange`, and the
 * focus goes back to the button. */
export function CategoryPicker({
  value,
  label,
  disabled,
  onChange,
}: {
  value: LetteringCategory
  label: string
  disabled?: boolean
  onChange: (category: LetteringCategory) => void
}) {
  const { t } = useTranslation()
  const search = t('booharu.lettering.search')

  return (
    <Combobox
      items={letteringCategories}
      value={value}
      itemToStringLabel={(category: LetteringCategory) => t(categoryName(category))}
      autoHighlight
      disabled={disabled}
      onValueChange={(category) => {
        if (category) onChange(category)
      }}
    >
      <ComboboxTrigger
        render={<Button variant='outline' className='w-full min-w-0 justify-between font-normal' />}
        aria-label={label}
        className='h-8 gap-2 px-2.5 text-[12px]'
      >
        <span className='min-w-0 truncate'>
          <ComboboxValue />
        </span>
      </ComboboxTrigger>
      <ComboboxContent>
        <ComboboxInput
          showTrigger={false}
          aria-label={search}
          placeholder={search}
          className='text-[11px]'
        />
        <ComboboxEmpty>{t('booharu.lettering.noMatch')}</ComboboxEmpty>
        <ComboboxList>
          {(category: LetteringCategory) => (
            <ComboboxItem key={category} value={category} className='text-[12px]'>
              {t(categoryName(category))}
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}
