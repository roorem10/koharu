'use client'

import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { call } from '@/lib/backend'
import { isTextLayer } from '@/lib/document'
import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Page } from '@koharu/bridge/protocol'

export function usePageEffects(page: Page | null | undefined) {
  const project = useProject().data
  const styles = useKoharuStore((state) => state.textStyles)
  const detectColors = useKoharuStore((state) => state.preferences?.pipeline.detect_text_color)
  const targets = useMemo(
    () => page?.layers.filter(isTextLayer).map((layer) => layer.id) ?? [],
    [page],
  )
  const query = useQuery({
    queryKey: [
      'box-effects',
      project?.name,
      page?.id,
      project?.revision,
      targets,
      styles,
      detectColors,
    ],
    queryFn: () => call(commands.getBoxEffects, targets),
    enabled: !!page && project?.revision !== undefined && targets.length > 0,
  })
  return useMemo(
    () => new Map(targets.map((id, index) => [id, query.data?.[index] ?? false])),
    [targets, query.data],
  )
}
