'use client'

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { saveTextStyles } from '@/lib/backend'
import {
  bindingFromEvent,
  formatBinding,
  formatShortcutConflict,
  isBinding,
  loadShortcutLayout,
  presetShortcutConflicts,
} from '@/lib/shortcuts'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@koharu/ui/components/dialog'

export function PresetShortcutDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const { t } = useTranslation()
  const styles = useKoharuStore((state) => state.textStyles)
  const shortcuts = useKoharuStore((state) => state.shortcuts)
  const preset = styles?.presets?.find((preset) => preset.id === id)
  const [binding, setBinding] = useState(preset?.key ?? '')
  const [recording, setRecording] = useState(false)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    void loadShortcutLayout()
  }, [])
  if (!preset || !styles) return null
  const conflicts = presetShortcutConflicts(binding, shortcuts, styles.presets ?? [], id)
  const valid = isBinding(binding)
  const messages = [...new Set(conflicts.map((conflict) => formatShortcutConflict(conflict, t)))]
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent showCloseButton={false}>
        <DialogTitle>{t('textStyles.shortcutFor', { name: preset.name })}</DialogTitle>
        <DialogDescription>{t('textStyles.shortcutHelp')}</DialogDescription>
        <Button
          variant='outline'
          disabled={busy}
          aria-label={t('textStyles.recordShortcut')}
          aria-pressed={recording}
          aria-invalid={!valid || conflicts.length > 0}
          aria-describedby={messages.length || !valid ? 'preset-shortcut-errors' : undefined}
          onClick={() => setRecording(true)}
          onBlur={() => setRecording(false)}
          onKeyDown={(event) => {
            if (!recording) return
            event.preventDefault()
            event.stopPropagation()
            if (event.key === 'Escape') {
              setRecording(false)
              return
            }
            const next = bindingFromEvent(event.nativeEvent)
            if (next !== null) {
              setBinding(next)
              setRecording(false)
            }
          }}
        >
          {recording
            ? t('settings.shortcuts.pressKeys')
            : formatBinding(binding) || t('settings.shortcuts.unassigned')}
        </Button>
        {(messages.length > 0 || !valid) && (
          <p id='preset-shortcut-errors' role='alert' className='text-xs text-destructive'>
            {messages.join(' · ') || t('textStyles.invalidShortcut')}
          </p>
        )}
        <DialogFooter>
          <Button
            variant='ghost'
            disabled={busy || !binding}
            onClick={() => {
              setBinding('')
              setRecording(false)
            }}
          >
            {t('textStyles.clear')}
          </Button>
          <Button variant='outline' disabled={busy} onClick={onClose}>
            {t('textStyles.cancel')}
          </Button>
          <Button
            disabled={busy || !valid || conflicts.length > 0}
            onClick={() => {
              const current = useKoharuStore.getState().textStyles
              if (!current) return
              setBusy(true)
              void saveTextStyles({
                ...current,
                presets: (current.presets ?? []).map((other) =>
                  other.id === id ? { ...other, key: binding || null } : other,
                ),
              })
                .then(onClose)
                .catch(() => undefined)
                .finally(() => setBusy(false))
            }}
          >
            {t('textStyles.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
