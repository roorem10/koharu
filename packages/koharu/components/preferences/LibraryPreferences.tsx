'use client'

import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { FolderPreference } from '@/components/files/FolderPreference'
import { useLibrarySettings } from '@/components/files/settings'
import { call } from '@/lib/backend'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'

import { PreferenceRow, PreferenceSection } from './PreferenceFields'

export function LibraryPreferences() {
  const { t } = useTranslation()
  const settings = useLibrarySettings()
  const paths = useQuery({
    queryKey: ['library-paths'],
    queryFn: () => call(commands.getLibraryPaths),
  })
  const [pattern, setPattern] = useState<string | null>(null)
  const [error, setError] = useState('')
  const value = pattern ?? settings.data?.export_name_pattern ?? ''

  const savePattern = async () => {
    if (!settings.data) return
    const tokens = value.match(/\{[^}]*\}/g) ?? []
    const unknown = tokens.find(
      (token) => !['{project}', '{index}', '{number}', '{page}'].includes(token),
    )
    if (unknown) {
      setError(t('files.unknownToken', { token: unknown }))
      return
    }
    if (!value.trim() || /[{}]/.test(value.replace(/\{[^}]*\}/g, ''))) {
      setError(t('files.invalidPattern'))
      return
    }
    setError('')
    try {
      await settings.save({ ...settings.data, export_name_pattern: value })
      setPattern(null)
    } catch {
      /* call() reports command errors. */
    }
  }

  return (
    <PreferenceSection title={t('files.title')}>
      <PreferenceRow
        title={t('files.projectsFolder')}
        description={t('files.projectsRestart')}
        align='start'
      >
        <FolderPreference
          kind='projects'
          active={paths.data?.projects_folder}
          defaultPath={paths.data?.default_projects_folder}
          selected={settings.data?.projects_folder}
        />
      </PreferenceRow>
      <PreferenceRow
        title={t('files.modelsFolder')}
        description={t('files.modelsRestart')}
        align='start'
      >
        <FolderPreference
          kind='models'
          active={paths.data?.models_folder}
          defaultPath={paths.data?.default_models_folder}
          selected={settings.data?.models_folder}
        />
      </PreferenceRow>
      <PreferenceRow
        title={t('files.exportPattern')}
        description={t('files.patternHelp')}
        align='start'
      >
        <div className='grid gap-2'>
          <Input
            aria-label={t('files.exportPattern')}
            aria-invalid={Boolean(error)}
            value={value}
            disabled={!settings.data || settings.saving}
            onChange={(event) => {
              setPattern(event.currentTarget.value)
              setError('')
            }}
          />
          {error && (
            <p role='alert' className='text-xs text-destructive'>
              {error}
            </p>
          )}
          <Button
            size='sm'
            variant='outline'
            disabled={!settings.data || settings.saving}
            onClick={() => void savePattern()}
          >
            {t('files.save')}
          </Button>
        </div>
      </PreferenceRow>
    </PreferenceSection>
  )
}
