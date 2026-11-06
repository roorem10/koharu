'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { lineCategoryKey } from '@/lib/line-labels'
import { pageKey, projectKey, refresh, useProject } from '@/lib/queries'
import { commands, type EntityId, type SfxAction } from '@koharu/bridge/protocol'

export function useSfxHandling(layer: EntityId) {
  const revision = useProject().data?.revision
  return useQuery({
    queryKey: [...lineCategoryKey, 'sfx', layer, revision],
    queryFn: () => call(commands.getSfxHandling, layer),
  })
}

export function useSetSfxHandling() {
  return useMutation({
    mutationFn: ({ layer, action }: { layer: EntityId; action: SfxAction | null }) =>
      call(commands.setSfxHandling, layer, action),
    onSuccess: () => refresh(projectKey, pageKey, lineCategoryKey),
  })
}
