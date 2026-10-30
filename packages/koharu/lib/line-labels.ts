'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import {
  commands,
  type EntityId,
  type LetteringCategory,
  type LineCategory,
} from '@koharu/bridge/protocol'

import { call } from './backend'
import { queryClient, useProject } from './queries'

// Shared by WO-18 and WO-19. Settings order; keep in sync with LetteringCategory::ALL.
export const LETTERING_CATEGORIES = [
  'normal_dialogue',
  'thoughts',
  'floating',
  'small_text',
  'narration_box',
  'shout',
  'weak_shout',
  'sfx',
  'moan',
  'title',
  'phone_text',
  'whisper',
  'shy',
  'crying',
  'menacing',
] as const satisfies readonly LetteringCategory[]

export const lineCategoryKey = ['line-category'] as const

export function useLineCategory(layer: EntityId | undefined) {
  const revision = useProject().data?.revision
  return useQuery<LineCategory | null>({
    queryKey: [...lineCategoryKey, layer, revision],
    queryFn: () => (layer ? call(commands.getLineCategory, layer) : Promise.resolve(null)),
    enabled: layer !== undefined,
  })
}

export function useSetLineCategory() {
  return useMutation({
    mutationKey: lineCategoryKey,
    mutationFn: ({ layer, category }: { layer: EntityId; category: LetteringCategory | null }) =>
      call(commands.setLineCategory, layer, category),
    onSuccess: () => invalidateLineLabels(),
  })
}

/** Call after anything writes labels to the sidecar outside useSetLineCategory. */
export async function invalidateLineLabels(): Promise<void> {
  await queryClient.invalidateQueries({ queryKey: lineCategoryKey })
}
