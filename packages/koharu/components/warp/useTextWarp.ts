'use client'

import { useIsMutating, useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { pageKey, projectKey, queryClient, refresh, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId, type TextWarp } from '@koharu/bridge/protocol'

const warpKey = ['text-warp'] as const
const editKey = ['text-warp-edit'] as const

export function useTextWarp(layers: EntityId[]) {
  const project = useProject().data
  const revision = useKoharuStore((state) => state.canvasRevision)
  const busy = useIsMutating({ mutationKey: editKey }) > 0
  const query = useQuery({
    queryKey: [...warpKey, project?.name, project?.revision, revision, layers],
    queryFn: () => call(commands.getTextWarp, layers),
    enabled: Boolean(project && layers.length),
    gcTime: 30_000,
  })
  const mutation = useMutation({
    mutationKey: editKey,
    meta: { activity: 'warp.saving' },
    mutationFn: (warp: TextWarp | null) => call(commands.setTextWarp, layers, warp),
    onSuccess: async () => {
      await queryClient.cancelQueries({ queryKey: warpKey })
      await refresh(warpKey, projectKey, pageKey)
    },
  })
  return {
    warps: query.data,
    ready: query.isSuccess && query.data.length === layers.length && layers.length > 0,
    busy,
    failed: query.isError || mutation.isError,
    setWarp: (warp: TextWarp | null) => mutation.mutate(warp),
  }
}
