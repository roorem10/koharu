'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useLibrarySettings } from '@/components/files/settings'
import { PreferenceRow, PreferenceSection } from './PreferenceFields'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'

export function LibraryPreferences() {
  const { t } = useTranslation()
  const settings = useLibrarySettings()
  const [pattern, setPattern] = useState<string | null>(null)
  const [error, setError] = useState('')
  const value = pattern ?? settings.data?.export_name_pattern ?? ''

  const savePattern = async () => {
    if (!settings.data) return
    const tokens = value.match(/\{[^}]*\}/g) ?? []
    const unknown = tokens.find((token) => !['{project}', '{index}', '{number}', '{page}'].includes(token))
    if (unknown) { setError(t('files.unknownToken', { token: unknown })); return }
    if (!value.trim() || /[{}]/.test(value.replace(/\{[^}]*\}/g, ''))) {
      setError(t('files.invalidPattern')); return
    }
    setError('')
    try { await settings.save({ ...settings.data, export_name_pattern: value }); setPattern(null) }
    catch { /* call() reports command errors. */ }
  }

  return (
    <PreferenceSection title={t('files.title')}>
      <PreferenceRow title={t('files.exportPattern')} description={t('files.patternHelp')} align='start'>
        <div className='grid gap-2'>
          <Input aria-label={t('files.exportPattern')} aria-invalid={Boolean(error)}
            value={value} disabled={!settings.data || settings.saving}
            onChange={(event) => { setPattern(event.currentTarget.value); setError('') }} />
          {error && <p role='alert' className='text-xs text-destructive'>{error}</p>}
          <Button size='sm' variant='outline' disabled={!settings.data || settings.saving}
            onClick={() => void savePattern()}>{t('files.save')}</Button>
        </div>
      </PreferenceRow>
    </PreferenceSection>
  )
}
