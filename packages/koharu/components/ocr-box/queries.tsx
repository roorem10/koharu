'use client'

import { useIsMutating, useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useBoxesBusy } from '@/components/boxes/queries'
import { useCompareStore } from '@/components/qc/compare'
import { call } from '@/lib/backend'
import { lineCategoryKey } from '@/lib/line-labels'
import { pageKey, pagesKey, projectKey, queryClient, refresh } from '@/lib/queries'
import {
  commands,
  type EntityId,
  type OcrBoxSettings,
  type RedoAction,
} from '@koharu/bridge/protocol'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@koharu/ui/components/alert-dialog'

export const ocrBoxSettingsKey = ['ocr-box-settings'] as const
export const ocrBoxEditKey = ['ocr-box-edit'] as const

export function useOcrBoxSettings() {
  return useQuery({ queryKey: ocrBoxSettingsKey, queryFn: () => call(commands.getOcrBoxSettings) })
}

export function useSaveOcrBoxSettings() {
  return useMutation({
    mutationFn: (settings: OcrBoxSettings) => call(commands.saveOcrBoxSettings, settings),
    onSuccess: (settings) => queryClient.setQueryData(ocrBoxSettingsKey, settings),
  })
}

export function useOcrBoxBusy() {
  const running = useBoxesBusy()
  const editing = useIsMutating({ mutationKey: ocrBoxEditKey }) > 0
  const original = useCompareStore((state) => state.held || state.pinned)
  return running || editing || original
}

export function refreshOcrBoxes() {
  return refresh(projectKey, pagesKey, pageKey, lineCategoryKey)
}

type Confirmation = { action: 'read' | 'translate'; layers: EntityId[] }

export function useRedoActions(layers: EntityId[]) {
  const { t } = useTranslation()
  const busy = useOcrBoxBusy()
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null)
  const mutation = useMutation({
    mutationKey: ocrBoxEditKey,
    mutationFn: async ({
      action,
      layers,
      overwrite,
    }: {
      action: RedoAction
      layers: EntityId[]
      overwrite: boolean
    }) => {
      if (action !== 'clean' && !overwrite) {
        const info = await call(commands.getOcrRedoInfo, layers)
        if (action === 'read' ? info.user_sources > 0 : info.user_translations > 0) {
          setConfirmation({ action, layers })
          return
        }
      }
      await call(commands.redoOcrBoxes, layers, action, overwrite)
      setConfirmation(null)
    },
    onSettled: refreshOcrBoxes,
  })
  return {
    disabled: layers.length === 0 || busy || mutation.isPending,
    redo: (action: RedoAction) =>
      mutation.mutate({ action, layers: [...layers], overwrite: false }),
    dialog: (
      <AlertDialog
        open={confirmation !== null}
        onOpenChange={(open) => {
          if (!open && !mutation.isPending) setConfirmation(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('ocrBox.overwriteTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t(
                confirmation?.action === 'read'
                  ? 'ocrBox.overwriteSource'
                  : 'ocrBox.overwriteTranslation',
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={mutation.isPending}>
              {t('ocrBox.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={busy || mutation.isPending}
              onClick={(event) => {
                event.preventDefault()
                if (confirmation) mutation.mutate({ ...confirmation, overwrite: true })
              }}
            >
              {t('ocrBox.overwrite')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    ),
  }
}
