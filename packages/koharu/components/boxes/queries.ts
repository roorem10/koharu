'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId } from '@koharu/bridge/protocol'

export function useBoxActionInfo(layer: EntityId) {
  const revision = useProject().data?.revision
  return useQuery({
    queryKey: ['boxes', 'actions', layer, revision],
    queryFn: () => call(commands.getBoxActions, layer),
    enabled: revision !== undefined,
  })
}

type Edit = { action: 'join' | 'split'; layer: EntityId } | { action: 'page'; page: EntityId }

export function useBoxEdits() {
  const selectLayers = useKoharuStore((state) => state.selectLayers)
  return useMutation({
    mutationKey: ['boxes', 'edit'],
    mutationFn: async (edit: Edit) => {
      switch (edit.action) {
        case 'join':
          selectLayers([await call(commands.joinBubbleBoxes, edit.layer)])
          break
        case 'split':
          selectLayers(await call(commands.splitBox, edit.layer))
          break
        case 'page':
          await call(commands.joinPageBubbles, edit.page)
          selectLayers([])
          break
      }
    },
    onSettled: () => refresh(projectKey, pagesKey, pageKey, ['boxes']),
  })
}

export function useBoxesBusy() {
  return useKoharuStore((state) => Object.values(state.jobs).some((job) => job.state === 'running'))
}
