'use client'

import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview'
import { FilePlus2 } from 'lucide-react'
import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useImportPages, useProject } from '@/lib/queries'
import { toast } from '@koharu/ui/components/toast'

export function FileDropImport() {
  const { t } = useTranslation()
  const project = useProject().data
  const { importDroppedPages, importing } = useImportPages()
  const [dragging, setDragging] = useState(false)
  const pending = useRef(false)

  const receive = useEffectEvent((event: DragDropEvent) => {
    if (event.type === 'enter' || event.type === 'over') {
      setDragging(Boolean(project) && !importing && !pending.current)
      return
    }
    setDragging(false)
    if (event.type !== 'drop') return
    if (!project) {
      toast.add({ type: 'info', title: t('navigator.openProjectFirst') })
      return
    }
    if (importing || pending.current) return
    pending.current = true
    void importDroppedPages(event.paths)
      .catch(() => undefined) // The shared command wrapper reports native import errors.
      .finally(() => {
        pending.current = false
      })
  })
  const unavailable = useEffectEvent(() => {
    toast.add({ type: 'error', title: t('navigator.dropUnavailable') })
  })

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void (async () => {
      const stop = await getCurrentWebview().onDragDropEvent((event) => {
        if (!disposed) receive(event.payload)
      })
      if (disposed) stop()
      else unlisten = stop
    })().catch(() => {
      if (!disposed) unavailable()
    })
    const clear = () => setDragging(false)
    const cancel = (event: KeyboardEvent) => {
      if (event.key === 'Escape') clear()
    }
    const accept = (event: DragEvent) => {
      // Chromium fires drop, and the runtime reports native paths, only when the
      // page accepts the drag. Checking the types does not read any file data.
      if (!event.dataTransfer?.types.includes('Files')) return
      event.preventDefault()
      event.dataTransfer.dropEffect = 'copy'
    }
    const drop = (event: DragEvent) => {
      // Cancel browser navigation; only native Tauri paths reach the importer.
      event.preventDefault()
      clear()
    }
    window.addEventListener('keydown', cancel, true)
    window.addEventListener('blur', clear)
    window.addEventListener('dragend', clear)
    window.addEventListener('dragenter', accept, true)
    window.addEventListener('dragover', accept, true)
    window.addEventListener('drop', drop, true)
    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', cancel, true)
      window.removeEventListener('blur', clear)
      window.removeEventListener('dragend', clear)
      window.removeEventListener('dragenter', accept, true)
      window.removeEventListener('dragover', accept, true)
      window.removeEventListener('drop', drop, true)
    }
  }, [])

  if (!dragging || !project || importing) return null
  return (
    <div
      role='status'
      aria-live='polite'
      className='pointer-events-none absolute inset-x-3 top-12 bottom-3 z-40 grid place-items-center rounded-2xl border-2 border-dashed border-primary bg-background/90 text-primary backdrop-blur-sm'
    >
      <div className='flex flex-col items-center gap-3 px-6 text-center'>
        <FilePlus2 aria-hidden='true' className='size-10' />
        <span className='text-lg font-medium'>{t('navigator.dropToImport')}</span>
      </div>
    </div>
  )
}
