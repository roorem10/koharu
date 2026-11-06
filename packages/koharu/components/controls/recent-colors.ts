'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'

export const recentColorsKey = ['recent-colors'] as const

export function useRecentColors(open: boolean) {
  const query = useQuery(
    {
      queryKey: recentColorsKey,
      queryFn: () => call(commands.getRecentColors),
      enabled: open,
    },
    queryClient,
  )
  const mutation = useMutation(
    {
      // All color wells share application history; preserve application order across them.
      scope: { id: 'recent-colors' },
      mutationFn: (color: [number, number, number, number]) => call(commands.rememberColor, color),
      onSuccess: async (colors) => {
        await queryClient.cancelQueries({ queryKey: recentColorsKey })
        queryClient.setQueryData(recentColorsKey, colors)
      },
    },
    queryClient,
  )
  return { colors: query.data ?? [], remember: mutation.mutate }
}
