'use client'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { commands, type McpOptions } from '@koharu/bridge/protocol'

const mcpKey = ['mcp-status'] as const

export function useMcpStatus() {
  const client = useQueryClient()
  return useQuery({
    queryKey: mcpKey,
    queryFn: () => call(commands.getMcpStatus),
    refetchInterval: () => (client.isMutating({ mutationKey: mcpKey }) ? false : 2000),
  })
}

export function useUpdateMcp() {
  const client = useQueryClient()
  return useMutation({
    mutationKey: mcpKey,
    scope: { id: 'mcp-settings' },
    mutationFn: (update: McpOptions | 'new-token') =>
      update === 'new-token' ? call(commands.newMcpToken) : call(commands.saveMcpSettings, update),
    onMutate: () => client.cancelQueries({ queryKey: mcpKey }),
    onSuccess: (status) => client.setQueryData(mcpKey, status),
    onSettled: () => client.invalidateQueries({ queryKey: mcpKey }),
  })
}
