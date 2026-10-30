'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { lineCategoryKey } from '@/lib/line-labels'
import { queryClient, useProject } from '@/lib/queries'
import { commands, type EntityId, type LetteringSettings } from '@koharu/bridge/protocol'

const letteringSettingsKey = ['lettering-settings'] as const

export function useLetteringSettings() {
  return useQuery({
    queryKey: letteringSettingsKey,
    queryFn: () => call(commands.getLetteringSettings),
  })
}

export function useSaveLetteringSettings() {
  return useMutation({
    mutationFn: (settings: LetteringSettings) => call(commands.saveLetteringSettings, settings),
    onSuccess: async (settings) => {
      queryClient.setQueryData(letteringSettingsKey, settings)
      await queryClient.invalidateQueries({ queryKey: lineCategoryKey })
    },
  })
}

export function useEffectiveCategory(layer: EntityId) {
  const revision = useProject().data?.revision
  return useQuery({
    queryKey: [...lineCategoryKey, 'effective', layer, revision],
    queryFn: () => call(commands.getEffectiveCategory, layer),
  })
}
