'use client'

import { useMutation } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId } from '@koharu/bridge/protocol'

export function useCenterInBubble() {
  return useMutation({
    mutationFn: async (layers: EntityId[]) => {
      const revision = useKoharuStore.getState().canvasRevision
      if (revision === null) return
      await call(commands.centerInBubble, revision, layers)
    },
    onSuccess: () => refresh(projectKey, pagesKey, pageKey),
  })
}
