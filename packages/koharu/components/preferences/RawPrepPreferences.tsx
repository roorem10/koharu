'use client'

import { useTranslation } from 'react-i18next'

import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { useRawPrepSettings, useSaveRawPrepSettings } from '@/components/raw-prep/data'
import { PrepOptionFields } from '@/components/raw-prep/PrepOptionFields'
import { Checkbox } from '@koharu/ui/components/checkbox'

export function RawPrepPreferences() {
  const { t } = useTranslation()
  const settings = useRawPrepSettings()
  const save = useSaveRawPrepSettings()
  const value = settings.data
  return (
    <PreferenceSection title={t('rawPrep.title')} description={t('rawPrep.defaultsHelp')}>
      {!value ? (
        <p role='status' className='py-3 text-xs'>
          {t(settings.isError ? 'rawPrep.loadError' : 'rawPrep.loading')}
        </p>
      ) : (
        <>
          <PreferenceRow title={t('rawPrep.defaults')} align='start'>
            <PrepOptionFields
              value={value.options}
              disabled={save.isPending}
              onChange={(options) => save.mutate({ ...value, options })}
            />
          </PreferenceRow>
          <PreferenceRow title={t('rawPrep.onImport')} description={t('rawPrep.onImportHelp')}>
            <Checkbox
              aria-label={t('rawPrep.onImport')}
              checked={value.fix_on_import}
              disabled={save.isPending}
              onCheckedChange={(checked) => save.mutate({ ...value, fix_on_import: !!checked })}
            />
          </PreferenceRow>
        </>
      )}
    </PreferenceSection>
  )
}
