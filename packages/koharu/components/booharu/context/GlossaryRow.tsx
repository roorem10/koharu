'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { GlossaryDialog } from '@/components/booharu/context/GlossaryDialog'
import { PreferenceRow } from '@/components/preferences/PreferenceFields'
import { useChapterContext } from '@/lib/booharu/context'
import { Button } from '@koharu/ui/components/button'

/** The Glossary row, the last of Settings > Pipeline's Translator section, as the glossary is what
 * every translation follows (D25): Open full glossary opens the glossary editor, on the series the
 * open project's chapter follows when one is open. */
export function GlossaryRow() {
  const { t } = useTranslation()
  const linked = useChapterContext().data?.series?.id ?? null
  const [open, setOpen] = useState(false)

  return (
    <>
      <PreferenceRow
        title={t('booharu.context.glossary')}
        description={t('booharu.context.settingsDescription')}
      >
        <Button type='button' size='sm' variant='outline' onClick={() => setOpen(true)}>
          {t('booharu.context.openGlossary')}
        </Button>
      </PreferenceRow>
      {open && <GlossaryDialog initial={linked} onClose={() => setOpen(false)} />}
    </>
  )
}
