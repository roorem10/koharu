'use client'

import { useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { commands, type CleanupSettings } from '@koharu/bridge/protocol'

export const cleanupSettingsKey = ['cleanup-settings'] as const
export const defaultCleanupSettings: Required<CleanupSettings> = {
  remove_mode: 'queue',
  paint_target: 'cleanup',
  grow_px: 2,
}

export function useCleanupSettings() {
  return useQuery({
    queryKey: cleanupSettingsKey,
    queryFn: async () => ({
      ...defaultCleanupSettings,
      ...(await call(commands.getCleanupSettings)),
    }),
  })
}
