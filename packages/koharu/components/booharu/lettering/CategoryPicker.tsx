'use client'

import { useState, type ReactNode } from 'react'
import { Trans, useTranslation } from 'react-i18next'

import { PickerSearch } from '@/components/controls/PickerSearch'
import { choiceName, letteringChoices, type LetteringChoice } from '@/lib/booharu/boxLettering'
import {
  categoryName,
  letteringCategories,
  letteringCategoriesSection,
} from '@/lib/booharu/categories'
import { useBooharuPreferences } from '@/lib/booharu/preferences'
import { openSettingsSection } from '@/lib/booharu/settingsLink'
import type { LetteringCategory } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  ComboboxValue,
} from '@koharu/ui/components/combobox'
import { cn } from '@koharu/ui/lib/utils'

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
  const [query, setQuery] = useState('')

  return (
    <Combobox
      items={letteringCategories}
      value={value}
      itemToStringLabel={(category: LetteringCategory) => t(categoryName(category))}
      autoHighlight
      disabled={disabled}
      inputValue={query}
      onInputValueChange={setQuery}
      onOpenChange={(open) => !open && setQuery('')}
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
      <CategoryOptions query={query} onClear={() => setQuery('')} />
    </Combobox>
  )
}

/** A block's category picker, opened by `children`, its `ComboboxTrigger`, which names the
 * category the block is lettered in: Automatic and every category under a search line, which
 * filters and moves through them as CategoryPicker's does, `value` marked as chosen, with `footer`
 * under the list. A pick calls `onChange`, even of the choice already made: Automatic clears the
 * agent's label, and while Pick fonts automatically is on, a category picked again gives the block
 * its font as Settings has it now. `readOnly`, as for a locked block, it opens to be read and takes
 * no pick. */
export function BlockCategoryPicker({
  value,
  readOnly,
  onChange,
  footer,
  className,
  children,
}: {
  value: LetteringChoice
  readOnly?: boolean
  onChange: (choice: LetteringChoice) => void
  footer?: ReactNode
  /** The list's own classes, such as a width wider than a small trigger. */
  className?: string
  children: ReactNode
}) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')

  return (
    <Combobox
      items={letteringChoices}
      value={value}
      itemToStringLabel={(choice: LetteringChoice) => t(choiceName(choice))}
      autoHighlight
      readOnly={readOnly}
      inputValue={query}
      onInputValueChange={setQuery}
      onOpenChange={(open) => !open && setQuery('')}
      onValueChange={(choice) => {
        if (choice) onChange(choice)
      }}
    >
      {children}
      <CategoryOptions
        query={query}
        onClear={() => setQuery('')}
        footer={footer}
        className={className}
      />
    </Combobox>
  )
}

/** A category picker's list: Koharu's search line for pickers over `query`, which `onClear`
 * empties, then each choice by its name, or a line saying nothing matches, and `footer` under
 * them. */
function CategoryOptions({
  query,
  onClear,
  footer,
  className,
}: {
  query: string
  onClear: () => void
  footer?: ReactNode
  className?: string
}) {
  const { t } = useTranslation()

  return (
    <ComboboxContent className={className}>
      <div className='border-b p-1'>
        <PickerSearch query={query} label={t('booharu.lettering.search')} onClear={onClear} />
      </div>
      <ComboboxEmpty>{t('booharu.lettering.noMatch')}</ComboboxEmpty>
      <ComboboxList>
        {(choice: LetteringChoice) => (
          <ComboboxItem key={choice} value={choice}>
            {t(choiceName(choice))}
          </ComboboxItem>
        )}
      </ComboboxList>
      {footer}
    </ComboboxContent>
  )
}

/** The classes of a line under a block's category picker's list, such as AutoFontsOff's. */
export const pickerFooter = 'border-t border-border/60 px-2.5 py-2'

/** What a block's category picker says while Pick fonts automatically, `[booharu-lettering]`
 * `auto`, is off, as it reads the settings each time it shows: that fonts follow categories only
 * with it on, with a link to the switch in Settings > Typesetting. Nothing while it's on, or still
 * being read. */
export function AutoFontsOff({ className }: { className?: string }) {
  const { t, i18n } = useTranslation()
  const auto = useBooharuPreferences().data?.['booharu-lettering'].auto
  if (auto !== false) return null
  return (
    <p className={cn('text-[10px] leading-4 text-muted-foreground', className)}>
      <Trans
        t={t}
        i18n={i18n}
        i18nKey='booharu.lettering.autoOff'
        values={{ setting: t('booharu.lettering.auto'), tab: t('settings.tabs.typesetting') }}
        components={{
          a: (
            <button
              type='button'
              className='text-primary underline underline-offset-2 hover:text-foreground'
              onClick={() => openSettingsSection('typesetting', letteringCategoriesSection)}
            />
          ),
        }}
      />
    </p>
  )
}
