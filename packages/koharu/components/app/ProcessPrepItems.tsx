'use client'

import { useQuery } from '@tanstack/react-query'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { usePrepDialog, useRawPrepBusy, useRestorePages } from '@/components/raw-prep/data'
import { RawPrepDialog } from '@/components/raw-prep/RawPrepDialog'
import { call } from '@/lib/backend'
import { usePages, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/bridge/protocol'
import { MenubarItem, MenubarSeparator } from '@koharu/ui/components/menubar'

export function ProcessPrepItems() {
  const { t } = useTranslation()
  const project = useProject().data
  const pages = usePages(!!project).data ?? []
  const selected = useKoharuStore((state) => state.selectedPages)
  const targets = pages.filter((page) => selected.includes(page.id)).map((page) => page.id)
  const open = usePrepDialog((state) => state.open)
  const busy = useRawPrepBusy()
  const restore = useRestorePages()
  const status = useQuery({
    queryKey: ['raw-prep-status', project?.name, project?.revision, targets],
    queryFn: () => call(commands.getPrepPageStatus, targets),
    enabled: !!project && targets.length > 0,
  })
  return (
    <>
      <MenubarSeparator />
      <MenubarItem
        className='min-h-8 px-2 py-1 text-xs'
        disabled={!project || pages.length === 0 || busy}
        onClick={() => project && open(project.name)}
      >
        {t('rawPrep.fixLevels')}
      </MenubarItem>
      <MenubarItem
        className='min-h-8 px-2 py-1 text-xs'
        disabled={!project || busy || !status.data?.some((page) => page.has_original)}
        onClick={() => {
          restore.mutate(targets)
        }}
      >
        {t('rawPrep.restore')}
      </MenubarItem>
    </>
  )
}

// Mounted outside the Process popup so closing the menu cannot unmount the dialog.
export function RawPrepDialogs() {
  const project = useProject().data
  const requested = usePrepDialog((state) => state.project)
  const close = usePrepDialog((state) => state.close)
  useEffect(() => {
    if (requested && requested !== project?.name) close()
  }, [project?.name, requested, close])
  return project && requested === project.name ? (
    <RawPrepDialog key={project.name} onClose={close} />
  ) : null
}
