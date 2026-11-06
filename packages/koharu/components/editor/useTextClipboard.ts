'use client'

import { useIsMutating, useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { expandLayerSelection } from '@/lib/document'
import { pageKey, pagesKey, projectKey, queryClient, refresh } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type EntityId, type Page } from '@koharu/bridge/protocol'

const clipboardKey = ['text-clipboard'] as const
type ClipboardAction =
  | { action: 'copy' | 'cut'; page: EntityId; layers: EntityId[] }
  | { action: 'paste'; page: EntityId }

function canPaste() {
  return (
    queryClient.getQueryData<boolean>(clipboardKey) === true ||
    queryClient.isMutating({ mutationKey: clipboardKey }) > 0
  )
}

export function useTextClipboard() {
  const available =
    useQuery({
      queryKey: clipboardKey,
      queryFn: () => call(commands.getTextClipboardAvailable),
    }).data ?? false
  const busy = useIsMutating({ mutationKey: clipboardKey }) > 0
  const mutation = useMutation({
    mutationKey: clipboardKey,
    // A quick Copy, Paste sequence must observe the newly copied native payload.
    scope: { id: 'text-clipboard' },
    mutationFn: async (edit: ClipboardAction) => {
      if (edit.action === 'copy' || edit.action === 'cut') {
        await call(
          edit.action === 'copy' ? commands.copyTextBoxes : commands.cutTextBoxes,
          edit.layers,
        )
        await queryClient.cancelQueries({ queryKey: clipboardKey })
        queryClient.setQueryData(clipboardKey, true)
        if (edit.action === 'copy') return
        const page = queryClient.getQueryData<Page | null>(pageKey)
        if (page?.id === edit.page) {
          const state = useKoharuStore.getState()
          state.selectLayers(
            state.selectedLayers.filter((id) =>
              expandLayerSelection(page.layers, [id]).some((child) => !edit.layers.includes(child)),
            ),
          )
        }
      } else {
        const result = await call(commands.pasteTextBoxes, edit.page)
        await refresh(projectKey, pagesKey, pageKey)
        // A page switch while native rendering finishes keeps its own selection.
        if (queryClient.getQueryData<Page | null>(pageKey)?.id === edit.page)
          useKoharuStore.getState().selectLayers(result.layers)
        return
      }
      await refresh(projectKey, pagesKey, pageKey)
    },
  })
  return { available, busy, canPaste, run: mutation.mutate }
}
