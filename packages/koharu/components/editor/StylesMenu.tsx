'use client'

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PresetActions, PresetSummary, usePresetDialogs } from '@/components/editor/PresetControls'
import { TextStyleDialog } from '@/components/editor/TextStyleDialog'
import { call, refreshTextStyles, saveTextStyles } from '@/lib/backend'
import { usePage, pageKey, pagesKey, projectKey, refresh } from '@/lib/queries'
import { formatBinding } from '@/lib/shortcuts'
import { useKoharuStore } from '@/lib/store'
import { emptyTextStyle } from '@/lib/text-styles'
import { commands, type EntityId, type StyleScope, type TextStyle } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
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
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
} from '@koharu/ui/components/dropdown-menu'
import { toast } from '@koharu/ui/components/toast'

export function StylesMenu({ selected }: { selected: EntityId[] }) {
  const { t } = useTranslation()
  const page = usePage().data
  const presetsUI = usePresetDialogs(selected)
  const styles = useKoharuStore((state) => state.textStyles)
  const [capture, setCapture] = useState<EntityId | null>(null)
  const [confirmation, setConfirmation] = useState<{
    scope: StyleScope
    restylable: number
    manual: number
    shape: boolean
  } | null>(null)
  const [includeUser, setIncludeUser] = useState(false)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (!styles) void refreshTextStyles().catch(() => undefined)
  }, [styles])

  const apply = async (scope: StyleScope, style?: TextStyle, includeUser = false) => {
    setBusy(true)
    try {
      const result = style
        ? await call(commands.applyTextStyle, scope, style)
        : await call(commands.applyShapeRules, scope, includeUser)
      await refresh(projectKey, pagesKey, pageKey)
      toast.add({
        type: 'success',
        title: t('textStyles.applied', { count: result.changed }),
        description:
          [
            result.skipped ? t('textStyles.skipped', { count: result.skipped }) : '',
            result.kept_manual ? t('textStyles.keptManual', { count: result.kept_manual }) : '',
          ]
            .filter(Boolean)
            .join(' ') || undefined,
      })
      setConfirmation(null)
    } finally {
      setBusy(false)
    }
  }
  const confirm = async (scope: StyleScope, shape = false) => {
    const counts = await call(commands.countTextStyleTargets, scope)
    setIncludeUser(false)
    setConfirmation({ scope, ...counts, shape })
  }
  const presets = styles?.presets ?? []
  const defaultStyle = styles?.default_style ?? emptyTextStyle
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={<Button variant='outline' size='sm' className='w-full' />}
          disabled={!styles || busy}
        >
          {t('textStyles.styles')}
        </DropdownMenuTrigger>
        <DropdownMenuContent className='w-72'>
          {presets.map((preset) => (
            <div key={preset.id} className='flex min-w-0 items-stretch'>
              <DropdownMenuItem
                className='min-w-0 flex-1'
                disabled={!selected.length}
                onClick={() =>
                  void apply({ layers: selected }, preset.style).catch(() => undefined)
                }
              >
                <span className='min-w-0'>
                  <span className='block break-words'>{preset.name}</span>
                  <PresetSummary preset={preset} />
                  {preset.key && (
                    <kbd className='text-[10px] text-muted-foreground'>
                      {formatBinding(preset.key)}
                    </kbd>
                  )}
                </span>
              </DropdownMenuItem>
              <PresetActions
                preset={preset}
                nested
                canUpdate={selected.length > 0}
                onAction={presetsUI.manage}
              />
            </div>
          ))}
          <DropdownMenuItem
            disabled={!selected.length || presets.length >= 32}
            title={presets.length >= 32 ? t('textStyles.presetLimit') : undefined}
            onClick={presetsUI.create}
          >
            {t('textStyles.savePreset')}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuSub>
            <DropdownMenuSubTrigger disabled={!styles?.shape_rules?.enabled}>
              {t('textStyles.applyShapeRules')}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className='w-56'>
              <DropdownMenuItem
                disabled={!selected.length}
                onClick={() => void apply({ layers: selected }).catch(() => undefined)}
              >
                {t('textStyles.scopeSelection')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!page}
                onClick={() => {
                  if (page) void confirm({ page: page.id }, true).catch(() => undefined)
                }}
              >
                {t('textStyles.scopePage')}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!page}
                onClick={() => void confirm('project', true).catch(() => undefined)}
              >
                {t('textStyles.scopeProject')}
              </DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          <DropdownMenuItem
            disabled={!selected.length}
            onClick={() => void apply({ layers: selected }, defaultStyle).catch(() => undefined)}
          >
            {t('textStyles.applyDefault')}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!page}
            onClick={() => {
              if (page) void confirm({ page: page.id }).catch(() => undefined)
            }}
          >
            {t('textStyles.applyPage')}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!page}
            onClick={() => void confirm('project').catch(() => undefined)}
          >
            {t('textStyles.applyProject')}
          </DropdownMenuItem>
          <DropdownMenuItem disabled={!selected.length} onClick={() => setCapture(selected[0]!)}>
            {t('textStyles.setDefault')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {presetsUI.dialogs}
      {capture && (
        <TextStyleDialog
          layer={capture}
          title={t('textStyles.setDefault')}
          onClose={() => setCapture(null)}
          onSave={async (default_style) => {
            const current = useKoharuStore.getState().textStyles
            if (current) await saveTextStyles({ ...current, default_style })
          }}
        />
      )}
      <Dialog
        open={confirmation !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setConfirmation(null)
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogTitle>
            {t(confirmation?.shape ? 'textStyles.applyShapeRules' : 'textStyles.applyDefault')}
          </DialogTitle>
          <DialogDescription>
            {t('textStyles.confirmApply', {
              count: confirmation
                ? confirmation.restylable +
                  (confirmation.shape && !includeUser ? 0 : confirmation.manual)
                : 0,
            })}
          </DialogDescription>
          {confirmation?.shape && (
            <>
              <p className='text-sm text-muted-foreground'>
                {t('textStyles.manualCount', { count: confirmation.manual })}
              </p>
              <label className='flex items-center gap-2 text-sm'>
                <Checkbox
                  checked={includeUser}
                  onCheckedChange={(checked) => setIncludeUser(checked === true)}
                />
                {t('textStyles.includeManual')}
              </label>
            </>
          )}
          <DialogFooter>
            <Button variant='outline' disabled={busy} onClick={() => setConfirmation(null)}>
              {t('textStyles.cancel')}
            </Button>
            <Button
              disabled={
                busy ||
                !confirmation ||
                !(
                  confirmation.restylable +
                  (confirmation.shape && !includeUser ? 0 : confirmation.manual)
                )
              }
              onClick={() => {
                if (confirmation)
                  void apply(
                    confirmation.scope,
                    confirmation.shape ? undefined : defaultStyle,
                    includeUser,
                  ).catch(() => undefined)
              }}
            >
              {t('textStyles.apply')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
