'use client'

import { LockKeyhole, RotateCcw, X } from 'lucide-react'
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { saveShortcuts, saveWheelZoom } from '@/lib/backend'
import {
  bindingFromEvent,
  builtInShortcuts,
  formatBinding,
  loadShortcutLayout,
  shortcutActions,
  shortcutConflicts,
  shortcutLabels,
  shortcutSlots,
} from '@/lib/shortcuts'
import {
  defaultShortcuts,
  useKoharuStore,
  type ShortcutAction,
  type Shortcuts,
  type ShortcutSlot,
} from '@/lib/store'
import { Button } from '@koharu/ui/components/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { PreferencePage, PreferenceRow, PreferenceSection } from './PreferenceFields'

export interface ShortcutPreferencesHandle {
  canLeave: () => boolean
}

const canvasActions: readonly ShortcutAction[] = ['fit', 'zoom_in', 'zoom_out', 'zoom_actual']

export const ShortcutPreferences = forwardRef<ShortcutPreferencesHandle>(
  function ShortcutPreferences(_, ref) {
    const { t } = useTranslation()
    const saved = useKoharuStore((state) => state.shortcuts)
    const wheelZoom = useKoharuStore((state) => state.wheelZoom)
    const [draft, setDraft] = useState(saved)
    const edited = useRef(false)
    const [recording, setRecording] = useState<ShortcutSlot | null>(null)
    const conflicts = useMemo(() => shortcutConflicts(draft), [draft])
    const hasConflicts = Object.keys(conflicts).length > 0
    const banner = useRef<HTMLDivElement>(null)
    const [flash, setFlash] = useState(0)

    useEffect(() => {
      void loadShortcutLayout()
    }, [])

    useEffect(() => {
      if (!edited.current) setDraft(saved)
    }, [saved])

    useEffect(() => {
      if (!flash) return
      const timer = window.setTimeout(() => setFlash(0), 650)
      return () => window.clearTimeout(timer)
    }, [flash])

    useImperativeHandle(
      ref,
      () => ({
        canLeave: () => {
          if (!hasConflicts) return true
          banner.current?.scrollIntoView({ block: 'center' })
          banner.current?.focus({ preventScroll: true })
          setFlash((value) => value + 1)
          return false
        },
      }),
      [hasConflicts],
    )

    const update = (next: Shortcuts) => {
      edited.current = true
      setDraft(next)
      if (Object.keys(shortcutConflicts(next)).length) return
      useKoharuStore.getState().setShortcuts(next)
      void saveShortcuts(next).catch(() => undefined)
    }

    const rows = (actions: readonly ShortcutAction[]) =>
      actions.map((action) => {
        const slots = shortcutSlots(action)
        const errors = slots.flatMap((slot) => conflicts[slot] ?? [])
        const messages = [
          ...new Set(
            errors.map((conflict) =>
              'builtIn' in conflict
                ? t('settings.shortcuts.usedByBuiltIn', { action: t(conflict.builtIn.labelKey) })
                : t('settings.shortcuts.alsoUsedBy', {
                    action: t(shortcutLabels[conflict.slot.replace(/_alt$/, '') as ShortcutAction]),
                  }),
            ),
          ),
        ]
        return (
          <div
            key={action}
            data-conflict={errors.length > 0}
            className='data-[conflict=true]:text-destructive'
          >
            <PreferenceRow title={t(shortcutLabels[action])}>
              <div className='grid grid-cols-2 gap-3'>
                {slots.map((slot, index) => {
                  const slotName = t(`settings.shortcuts.${index ? 'alternate' : 'primary'}`)
                  const label = t('settings.shortcuts.inputLabel', {
                    action: `${t(shortcutLabels[action])} · ${slotName}`,
                  })
                  return (
                    <div key={slot} className='min-w-0'>
                      <p className='mb-1 text-[10px] text-muted-foreground'>{slotName}</p>
                      <div className='flex min-w-0 gap-1'>
                        <Button
                          type='button'
                          variant='outline'
                          aria-label={label}
                          aria-pressed={recording === slot}
                          aria-invalid={!!conflicts[slot]}
                          aria-describedby={
                            errors.length ? `shortcut-conflict-${action}` : undefined
                          }
                          title={formatBinding(draft[slot]) || t('settings.shortcuts.unassigned')}
                          className='h-8 min-w-0 flex-1 px-2 font-mono text-[11px] aria-[invalid=true]:border-destructive'
                          onClick={() => setRecording(slot)}
                          onBlur={() =>
                            setRecording((current) => (current === slot ? null : current))
                          }
                          onKeyDown={(event) => {
                            if (recording !== slot) return
                            event.preventDefault()
                            event.stopPropagation()
                            if (event.key === 'Escape') {
                              setRecording(null)
                              return
                            }
                            const binding = bindingFromEvent(event.nativeEvent)
                            if (binding === null) return
                            update({ ...draft, [slot]: binding })
                            setRecording(null)
                          }}
                        >
                          <span className='truncate'>
                            {recording === slot
                              ? t('settings.shortcuts.pressKeys')
                              : formatBinding(draft[slot]) || t('settings.shortcuts.unassigned')}
                          </span>
                        </Button>
                        <Button
                          type='button'
                          size='icon'
                          variant='ghost'
                          className='size-8 shrink-0 text-muted-foreground'
                          aria-label={t('settings.shortcuts.clear', {
                            action: `${t(shortcutLabels[action])} · ${slotName}`,
                          })}
                          disabled={!draft[slot]}
                          onClick={() => {
                            setRecording(null)
                            update({ ...draft, [slot]: '' })
                          }}
                        >
                          <X className='size-3.5' />
                        </Button>
                      </div>
                    </div>
                  )
                })}
              </div>
            </PreferenceRow>
            {messages.length > 0 && (
              <p id={`shortcut-conflict-${action}`} className='pb-3 text-[11px] text-destructive'>
                {messages.join(' · ')}
              </p>
            )}
          </div>
        )
      })

    return (
      <PreferencePage
        title={t('settings.shortcuts.title')}
        description={t('settings.shortcuts.description')}
      >
        {hasConflicts && (
          <div
            ref={banner}
            tabIndex={-1}
            role='alert'
            data-flash={flash > 0}
            className='rounded-lg border border-destructive bg-destructive/10 p-3 text-[12px] text-destructive outline-none data-[flash=true]:bg-destructive/20 data-[flash=true]:ring-2 data-[flash=true]:ring-destructive'
          >
            {t('settings.shortcuts.conflictBanner')}
          </div>
        )}
        <div>
          <Button
            type='button'
            variant='outline'
            size='sm'
            onClick={() => {
              setRecording(null)
              update({ ...defaultShortcuts })
              useKoharuStore.getState().setWheelZoom('ctrl')
              void saveWheelZoom('ctrl').catch(() => undefined)
            }}
          >
            <RotateCcw className='size-3.5' />
            {t('settings.shortcuts.reset')}
          </Button>
        </div>
        <PreferenceSection title={t('settings.shortcuts.tools')}>
          {rows(shortcutActions.filter((action) => !canvasActions.includes(action)))}
        </PreferenceSection>
        <PreferenceSection title={t('settings.shortcuts.canvas')}>
          {rows(canvasActions)}
        </PreferenceSection>
        <PreferenceSection title={t('settings.shortcuts.mouseWheel')}>
          <PreferenceRow title={t('settings.shortcuts.zoomWith')}>
            <Select
              value={wheelZoom}
              items={{
                ctrl: t('settings.shortcuts.ctrlWheel'),
                wheel: t('settings.shortcuts.wheelAlone'),
              }}
              onValueChange={(value) => {
                if (value !== 'ctrl' && value !== 'wheel') return
                useKoharuStore.getState().setWheelZoom(value)
                void saveWheelZoom(value).catch(() => undefined)
              }}
            >
              <SelectTrigger
                aria-label={t('settings.shortcuts.zoomWith')}
                className='h-8 w-full text-[11px]'
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='ctrl'>{t('settings.shortcuts.ctrlWheel')}</SelectItem>
                <SelectItem value='wheel'>{t('settings.shortcuts.wheelAlone')}</SelectItem>
              </SelectContent>
            </Select>
          </PreferenceRow>
          <PreferenceRow title={t('settings.shortcuts.scrollVertical')}>
            <p className='text-right text-[11px] text-muted-foreground'>
              {t(`settings.shortcuts.${wheelZoom === 'ctrl' ? 'wheelAlone' : 'ctrlWheel'}`)}
            </p>
          </PreferenceRow>
          {builtInShortcuts
            .filter((entry) => entry.id === 'brush' || entry.id === 'horizontal')
            .map((entry) => (
              <PreferenceRow key={entry.id} title={t(entry.labelKey)}>
                <div className='flex items-center justify-end gap-2 text-right text-[11px] text-muted-foreground'>
                  <LockKeyhole className='size-3 shrink-0' aria-hidden />
                  <span>{entry.keys.map((key) => t(key)).join(' / ')}</span>
                </div>
              </PreferenceRow>
            ))}
        </PreferenceSection>
        <PreferenceSection
          title={t('settings.shortcuts.builtIn')}
          description={t('settings.shortcuts.builtInDescription')}
        >
          {(['edit', 'canvas', 'mouse', 'fonts', 'agent', 'import'] as const).map((group) => (
            <div key={group}>
              <h4 className='pt-4 text-[11px] font-semibold text-muted-foreground'>
                {t(`settings.shortcuts.groups.${group}`)}
              </h4>
              {builtInShortcuts
                .filter(
                  (entry) =>
                    entry.group === group && entry.id !== 'brush' && entry.id !== 'horizontal',
                )
                .map((entry) => (
                  <PreferenceRow key={entry.id} title={t(entry.labelKey)}>
                    <div className='flex items-center justify-end gap-2 text-right text-[11px] text-muted-foreground'>
                      <LockKeyhole className='size-3 shrink-0' aria-hidden />
                      <span>
                        {entry.keys
                          .map((key) => (key.startsWith('settings.') ? t(key) : key))
                          .join(' / ')}
                      </span>
                    </div>
                  </PreferenceRow>
                ))}
            </div>
          ))}
        </PreferenceSection>
      </PreferencePage>
    )
  },
)
