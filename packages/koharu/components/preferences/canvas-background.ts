'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { workspaceColor, type WorkspaceColor } from '@koharu/bridge/canvas'
import {
  commands,
  type CanvasBackgroundColors,
  type CanvasBackgroundTheme,
} from '@koharu/bridge/protocol'

export const canvasBackgroundsKey = ['canvas-backgrounds'] as const

export function useCanvasBackgrounds() {
  return useQuery(
    {
      queryKey: canvasBackgroundsKey,
      queryFn: () => call(commands.getCanvasBackgrounds),
    },
    queryClient,
  )
}

export function useSaveCanvasBackground() {
  return useMutation(
    {
      mutationKey: canvasBackgroundsKey,
      scope: { id: 'canvas-backgrounds' },
      mutationFn: ({ theme, color }: { theme: CanvasBackgroundTheme; color: WorkspaceColor }) =>
        call(commands.saveCanvasBackground, theme, color),
      onSuccess: (colors) => queryClient.setQueryData(canvasBackgroundsKey, colors),
    },
    queryClient,
  )
}

export function canvasBackgroundColor(colors: CanvasBackgroundColors | undefined): WorkspaceColor {
  const theme = document.documentElement.classList.contains('dark') ? 'dark' : 'light'
  return colors?.[theme] ?? workspaceColor()
}
