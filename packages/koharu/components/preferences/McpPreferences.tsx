'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useMcpStatus, useUpdateMcp } from '@/components/mcp/queries'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { type McpOptions } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'
import { Switch } from '@koharu/ui/components/switch'

export function McpPreferences() {
  const { t } = useTranslation()
  const status = useMcpStatus()
  const update = useUpdateMcp()
  const settings = status.data
    ? {
        enabled: status.data.settings.enabled ?? false,
        port: status.data.settings.port ?? 47821,
        token: status.data.settings.token ?? '',
        allow_edits: status.data.settings.allow_edits ?? true,
      }
    : undefined
  const disabled = !settings || update.isPending
  const endpoint = status.data?.endpoint ?? ''
  const header = settings ? `Authorization: Bearer ${settings.token}` : ''
  const save = (change: Partial<McpOptions>) => {
    if (settings) {
      update.mutate({
        enabled: settings.enabled,
        port: settings.port,
        allow_edits: settings.allow_edits,
        ...change,
      })
    }
  }
  const state = status.data?.error
    ? t('mcp.serverError', { message: status.data.error })
    : status.data?.running
      ? t('mcp.running', { endpoint })
      : t('mcp.off')

  return (
    <PreferenceSection title={t('mcp.title')} description={t('mcp.description')}>
      <PreferenceRow title={t('mcp.enabled')}>
        <Switch
          aria-label={t('mcp.enabled')}
          checked={settings?.enabled ?? false}
          disabled={disabled}
          onCheckedChange={(enabled) => save({ enabled })}
        />
      </PreferenceRow>
      <PreferenceRow title={t('mcp.port')}>
        <PortEditor
          key={settings?.port}
          port={settings?.port ?? 47821}
          disabled={disabled}
          onSave={(port) => save({ port })}
        />
      </PreferenceRow>
      <PreferenceRow title={t('mcp.token')} description={t('mcp.tokenHelp')} align='start'>
        <div className='grid gap-2'>
          <Input
            aria-label={t('mcp.token')}
            value={settings?.token ?? ''}
            readOnly
            spellCheck={false}
            className='h-8 font-mono text-[11px]'
          />
          <div className='flex flex-wrap items-center gap-2'>
            <CopyButton
              key={settings?.token}
              value={settings?.token ?? ''}
              label={t('mcp.copyToken')}
            />
            <Button
              size='sm'
              variant='outline'
              disabled={disabled}
              onClick={() => update.mutate('new-token')}
            >
              {t('mcp.newToken')}
            </Button>
          </div>
        </div>
      </PreferenceRow>
      <PreferenceRow title={t('mcp.allowEdits')} description={t('mcp.allowEditsHelp')}>
        <Switch
          aria-label={t('mcp.allowEdits')}
          checked={settings?.allow_edits ?? true}
          disabled={disabled}
          onCheckedChange={(allow_edits) => save({ allow_edits })}
        />
      </PreferenceRow>
      <div className='py-3.5 text-[11px]'>
        <p role='status' className='break-words'>
          {status.isPending ? t('mcp.loading') : status.isError ? t('mcp.loadError') : state}
        </p>
        {update.isError && <p role='alert'>{t('mcp.saveError')}</p>}
      </div>
      {settings && (
        <div className='grid gap-4 py-4'>
          <div>
            <h4 className='text-[12px] font-medium'>{t('mcp.connectTitle')}</h4>
            <p className='mt-1 text-[11px] text-muted-foreground'>{t('mcp.connectHelp')}</p>
          </div>
          <ConnectionSnippet
            label={t('mcp.claudeCode')}
            value={`claude mcp add --transport http booharu ${endpoint} --header "${header}"`}
          />
          <ConnectionSnippet label={t('mcp.urlClients')} value={`${endpoint}\n${header}`} />
          <ConnectionSnippet
            label={t('mcp.stdioClients')}
            description={t('mcp.nodeRequired')}
            value={`npx mcp-remote ${endpoint} --header "${header}"`}
          />
        </div>
      )}
    </PreferenceSection>
  )
}

function PortEditor({
  port,
  disabled,
  onSave,
}: {
  port: number
  disabled: boolean
  onSave: (port: number) => void
}) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState(String(port))
  const value = Number(draft)
  const valid = draft.trim() !== '' && Number.isInteger(value) && value >= 1 && value <= 65535
  const apply = () => {
    if (valid && !disabled && value !== port) onSave(value)
  }
  return (
    <div className='grid gap-1'>
      <div className='flex gap-2'>
        <Input
          aria-label={t('mcp.port')}
          type='number'
          min={1}
          max={65535}
          step={1}
          value={draft}
          disabled={disabled}
          aria-invalid={!valid}
          className='h-8 min-w-0 text-[12px]'
          onChange={(event) => setDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              apply()
            }
          }}
        />
        <Button
          size='sm'
          variant='outline'
          disabled={disabled || !valid || value === port}
          onClick={apply}
        >
          {t('mcp.applyPort')}
        </Button>
      </div>
      {!valid && <p className='text-[10px] text-destructive'>{t('mcp.invalidPort')}</p>}
    </div>
  )
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const { t } = useTranslation()
  const [result, setResult] = useState<'copied' | 'copyError' | null>(null)
  return (
    <span className='inline-flex flex-wrap items-center gap-2'>
      <Button
        size='sm'
        variant='outline'
        aria-label={label}
        disabled={!value}
        onClick={() => {
          void Promise.resolve()
            .then(() => navigator.clipboard.writeText(value))
            .then(() => setResult('copied'))
            .catch(() => setResult('copyError'))
        }}
      >
        {t('mcp.copy')}
      </Button>
      {result && (
        <span role='status' className='text-[10px] text-muted-foreground'>
          {t(`mcp.${result}`)}
        </span>
      )}
    </span>
  )
}

function ConnectionSnippet({
  label,
  description,
  value,
}: {
  label: string
  description?: string
  value: string
}) {
  const { t } = useTranslation()
  return (
    <div className='grid min-w-0 gap-1.5'>
      <div className='flex flex-wrap items-center justify-between gap-2'>
        <span className='text-[11px] font-medium'>{label}</span>
        <CopyButton key={value} label={t('mcp.copySnippet', { name: label })} value={value} />
      </div>
      {description && <p className='text-[10px] text-muted-foreground'>{description}</p>}
      <textarea
        aria-label={label}
        value={value}
        readOnly
        rows={3}
        spellCheck={false}
        className='w-full resize-none rounded-md border border-input bg-muted/30 px-3 py-2 font-mono text-[11px] leading-5'
      />
    </div>
  )
}
