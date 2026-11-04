'use client'

import { useEffect, useRef, useState } from 'react'

import { call } from '@/lib/backend'
import { isTextLayer } from '@/lib/document'
import { pageKey, projectKey, refresh, usePage } from '@/lib/queries'
import { commands, type EntityId, type Typography } from '@koharu/bridge/protocol'

import { defaultTypography } from './typography'

export function useTypography(targets: EntityId[]) {
  const page = usePage().data
  const layers =
    page?.layers.filter(isTextLayer).filter((layer) => targets.includes(layer.id)) ?? []
  const [draft, setDraft] = useState<Map<EntityId, Typography> | null>(null)
  const optimistic = useRef<Map<EntityId, Typography> | null>(null)
  const sequence = useRef(0)
  const key = JSON.stringify([page?.id, targets])

  useEffect(() => {
    optimistic.current = null
    setDraft(null)
    sequence.current += 1
  }, [key])

  const apply = (update: (value: Typography) => Typography) => {
    if (!layers.length) return
    const updates = layers.map((layer) => ({
      layer: layer.id,
      typography: update(
        optimistic.current?.get(layer.id) ?? layer.typography ?? defaultTypography,
      ),
    }))
    optimistic.current = new Map(updates.map(({ layer, typography }) => [layer, typography]))
    setDraft(optimistic.current)
    const latest = ++sequence.current
    void call(commands.setTypography, updates)
      .then(() => refresh(projectKey, pageKey))
      .catch(() => undefined)
      .finally(() => {
        if (sequence.current !== latest) return
        optimistic.current = null
        setDraft(null)
      })
  }

  const values = layers.map(
    (layer) => draft?.get(layer.id) ?? layer.typography ?? defaultTypography,
  )
  return {
    layers,
    values,
    typography: values[0] ?? defaultTypography,
    disabled: !layers.length,
    pending: draft !== null,
    apply,
  }
}

export function isMixed<T>(values: Typography[], field: (value: Typography) => T): boolean {
  const first = JSON.stringify(values[0] && field(values[0]))
  return values.some((value) => JSON.stringify(field(value)) !== first)
}
