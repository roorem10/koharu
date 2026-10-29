'use client'

import { Ellipsis } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { TextStyleDialog } from '@/components/editor/TextStyleDialog'
import { saveTextStyles } from '@/lib/backend'
import { useFonts } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { styleGroups } from '@/lib/text-styles'
import type { EntityId, TextPreset } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@koharu/ui/components/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
} from '@koharu/ui/components/dropdown-menu'
import { Input } from '@koharu/ui/components/input'

type PresetAction = 'rename' | 'update' | 'delete'
type PresetEdit = ({ preset: TextPreset; action: PresetAction } | { action: 'create' }) & {
  layer?: EntityId
}

export function PresetSummary({ preset }: { preset: TextPreset }) {
  const { t } = useTranslation()
  const fonts = useFonts().data
  const font = preset.style.font?.preferred_font
  const normalize = (name: string) =>
    name.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
  const missing =
    font && fonts && !fonts.some((family) => normalize(family.name) === normalize(font))
  return (
    <span className='block text-[10px] leading-4 text-muted-foreground'>
      {styleGroups
        .filter((group) => preset.style[group] != null)
        .map((group) => t(`textStyles.groups.${group}`))
        .join(' · ') || t('textStyles.notSet')}
      {missing && <span className='ml-2 text-destructive'>{t('textStyles.fontMissing')}</span>}
    </span>
  )
}

export function PresetActions({
  preset,
  nested = false,
  canUpdate = false,
  onAction,
}: {
  preset: TextPreset
  nested?: boolean
  canUpdate?: boolean
  onAction: (preset: TextPreset, action: PresetAction) => void
}) {
  const { t } = useTranslation()
  const items = (
    <>
      <DropdownMenuItem onClick={() => onAction(preset, 'rename')}>
        {t('textStyles.rename')}
      </DropdownMenuItem>
      {canUpdate && (
        <DropdownMenuItem onClick={() => onAction(preset, 'update')}>
          {t('textStyles.updatePreset')}
        </DropdownMenuItem>
      )}
      <DropdownMenuItem variant='destructive' onClick={() => onAction(preset, 'delete')}>
        {t('textStyles.delete')}
      </DropdownMenuItem>
    </>
  )
  const label = t('textStyles.managePreset', { name: preset.name })
  return nested ? (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger
        aria-label={label}
        className='shrink-0 px-2 [&>svg:last-child]:hidden'
      >
        <Ellipsis aria-hidden='true' className='size-3.5' />
        <span className='sr-only'>{label}</span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent>{items}</DropdownMenuSubContent>
    </DropdownMenuSub>
  ) : (
    <DropdownMenu>
      <DropdownMenuTrigger aria-label={label} render={<Button variant='ghost' size='icon-sm' />}>
        <Ellipsis className='size-4' />
      </DropdownMenuTrigger>
      <DropdownMenuContent className='w-56'>{items}</DropdownMenuContent>
    </DropdownMenu>
  )
}

export function usePresetDialogs(selected: EntityId[]) {
  const [editing, setEditing] = useState<PresetEdit | null>(null)
  return {
    create: () => setEditing({ action: 'create', layer: selected[0] }),
    manage: (preset: TextPreset, action: PresetAction) =>
      setEditing({ preset, action, layer: selected[0] }),
    dialogs: editing && (
      <PresetDialog editing={editing} layer={editing.layer} onClose={() => setEditing(null)} />
    ),
  }
}

function PresetDialog({
  editing,
  layer,
  onClose,
}: {
  editing: PresetEdit
  layer?: EntityId
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [name, setName] = useState(editing.action === 'create' ? '' : editing.preset.name)
  const [busy, setBusy] = useState(false)
  const styles = useKoharuStore((state) => state.textStyles)
  const preset = editing.action === 'create' ? null : editing.preset
  const validName = !!name.trim() && [...name.trim()].length <= 64 && !/\p{Cc}/u.test(name)
  const duplicate = styles?.presets?.some(
    (other) =>
      other.id !== preset?.id &&
      other.name.normalize('NFKC').trim().toLowerCase() ===
        name.normalize('NFKC').trim().toLowerCase(),
  )

  if ((editing.action === 'create' || editing.action === 'update') && layer) {
    return (
      <TextStyleDialog
        layer={layer}
        title={t(editing.action === 'create' ? 'textStyles.savePreset' : 'textStyles.updatePreset')}
        initialName={editing.action === 'create' ? '' : undefined}
        initialGroups={
          preset ? styleGroups.filter((group) => preset.style[group] != null) : undefined
        }
        presetId={preset?.id}
        onClose={onClose}
        onSave={async (style, name) => {
          const current = useKoharuStore.getState().textStyles
          if (!current) return
          const presets = current.presets ?? []
          await saveTextStyles({
            ...current,
            presets: preset
              ? presets.map((other) => (other.id === preset.id ? { ...other, style } : other))
              : [...presets, { id: crypto.randomUUID(), name, style, key: null }],
          })
        }}
      />
    )
  }
  if (!preset) return null
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent showCloseButton={false}>
        <DialogTitle>
          {t(editing.action === 'delete' ? 'textStyles.delete' : 'textStyles.rename')}
        </DialogTitle>
        <DialogDescription>
          {editing.action === 'delete'
            ? t('textStyles.deleteQuestion', { name: preset.name })
            : t('textStyles.nameHelp')}
        </DialogDescription>
        {editing.action === 'rename' && (
          <label className='grid gap-2 text-sm'>
            {t('textStyles.presetName')}
            <Input
              aria-label={t('textStyles.presetName')}
              value={name}
              disabled={busy}
              aria-invalid={!validName}
              onChange={(event) => setName(event.target.value)}
            />
            {duplicate && (
              <span role='status' className='text-xs text-muted-foreground'>
                {t('textStyles.duplicateName')}
              </span>
            )}
          </label>
        )}
        <DialogFooter>
          <Button variant='outline' disabled={busy} onClick={onClose}>
            {t('textStyles.cancel')}
          </Button>
          <Button
            variant={editing.action === 'delete' ? 'destructive' : 'default'}
            disabled={busy || (editing.action === 'rename' && !validName)}
            onClick={() => {
              const current = useKoharuStore.getState().textStyles
              if (!current) return
              setBusy(true)
              const presets = current.presets ?? []
              void saveTextStyles({
                ...current,
                presets:
                  editing.action === 'delete'
                    ? presets.filter((other) => other.id !== preset.id)
                    : presets.map((other) =>
                        other.id === preset.id ? { ...other, name: name.trim() } : other,
                      ),
              })
                .then(onClose)
                .catch(() => undefined)
                .finally(() => setBusy(false))
            }}
          >
            {t(editing.action === 'delete' ? 'textStyles.delete' : 'textStyles.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function TextStylePresets() {
  const { t } = useTranslation()
  const presets = useKoharuStore((state) => state.textStyles?.presets)
  const actions = usePresetDialogs([])
  return (
    <section className='mb-8' aria-labelledby='text-style-presets'>
      <h3 id='text-style-presets' className='mb-1 text-[12px] font-semibold'>
        {t('textStyles.presets')}
      </h3>
      <p className='mb-3 text-[11px] text-muted-foreground'>{t('textStyles.presetsHelp')}</p>
      {presets?.length ? (
        <ul className='divide-y rounded-xl border bg-[var(--surface-panel)]'>
          {presets.map((preset) => (
            <li key={preset.id} className='flex min-w-0 items-center gap-3 px-4 py-3'>
              <div className='min-w-0 flex-1'>
                <p className='text-[12px] font-medium break-words'>{preset.name}</p>
                <PresetSummary preset={preset} />
              </div>
              <PresetActions preset={preset} onAction={actions.manage} />
            </li>
          ))}
        </ul>
      ) : (
        <p className='rounded-xl border px-4 py-4 text-[11px] text-muted-foreground'>
          {t('textStyles.noPresets')}
        </p>
      )}
      {actions.dialogs}
    </section>
  )
}
