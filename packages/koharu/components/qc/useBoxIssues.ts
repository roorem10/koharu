'use client'

import { useMemo } from 'react'

import type { EntityId } from '@koharu/bridge/protocol'

import { useQcResults } from './data'

export function useBoxIssues(page: EntityId | null | undefined): Map<EntityId, number> {
  const findings = useQcResults().data?.findings
  return useMemo(() => {
    const counts = new Map<EntityId, number>()
    if (page)
      for (const finding of findings ?? []) {
        if (finding.page === page) counts.set(finding.layer, (counts.get(finding.layer) ?? 0) + 1)
      }
    return counts
  }, [findings, page])
}
