'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh } from '@/lib/queries'
import { commands, type ScriptPreview } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'

export function useScriptFiles() {
  const { t } = useTranslation()
  const [preview, setPreview] = useState<ScriptPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const exportScript = async () => {
    if (busy) return
    setBusy(true)
    try {
      await call(commands.exportScript)
    } finally {
      setBusy(false)
    }
  }
  const importScript = async () => {
    if (busy) return
    setBusy(true)
    try {
      setPreview(await call(commands.previewScriptImport))
    } finally {
      setBusy(false)
    }
  }
  const apply = async () => {
    if (!preview || busy) return
    setBusy(true)
    try {
      await call(commands.applyScriptImport, preview)
      await refresh(projectKey, pagesKey, pageKey)
      setPreview(null)
    } finally {
      setBusy(false)
    }
  }
  const dialog = (
    <Dialog
      open={preview !== null}
      onOpenChange={(open) => {
        if (!open && !busy) setPreview(null)
      }}
    >
      <DialogContent className='sm:max-w-2xl' showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>{t('files.scriptPreview')}</DialogTitle>
          <DialogDescription>
            {t('files.scriptCounts', {
              changed: preview?.changes.length ?? 0,
              unchanged: preview?.unchanged ?? 0,
              missing: preview?.not_found ?? 0,
            })}
          </DialogDescription>
        </DialogHeader>
        <div className='max-h-[55vh] overflow-y-auto'>
          {preview?.changes.map((change) => (
            <section key={change.layer} className='min-w-0 border-b border-border py-3'>
              <h3 className='text-xs font-medium break-words'>
                {t('files.scriptBox', {
                  page: change.page,
                  label: change.page_label,
                  box: change.box_number,
                })}
              </h3>
              <div className='mt-2 grid min-w-0 gap-3 sm:grid-cols-2'>
                {(
                  [
                    ['oldText', change.old_text],
                    ['newText', change.new_text],
                  ] as const
                ).map(([label, text]) => (
                  <div key={label} className='min-w-0'>
                    <p className='mb-1 text-[10px] text-muted-foreground'>{t(`files.${label}`)}</p>
                    <p className='rounded-md bg-muted p-2 text-xs break-words whitespace-pre-wrap'>
                      {text || t('files.emptyTranslation')}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
        <DialogFooter>
          <Button variant='outline' disabled={busy} onClick={() => setPreview(null)}>
            {t('files.cancel')}
          </Button>
          <Button
            disabled={busy || !preview?.changes.length}
            onClick={() => void apply().catch(() => undefined)}
          >
            {t('files.apply')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
  return { exportScript, importScript, dialog, busy }
}
