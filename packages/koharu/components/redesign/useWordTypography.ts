'use client'

import { useMutation, useQuery } from '@tanstack/react-query'

import type { ApplyTypography } from '@/components/text-extras/SpacingControls'
import { call } from '@/lib/backend'
import { pageKey, projectKey, refresh, usePage, useProject } from '@/lib/queries'
import { charLength, type RichTextSelection } from '@/lib/richText'
import { useRichTextSelection } from '@/lib/richTextSelection'
import {
  commands,
  type Layer,
  type RunStyle,
  type RunStylePatch,
  type StyleRun,
  type Typography,
} from '@koharu/bridge/protocol'

import { defaultFont, defaultTypography } from './typography'

type TextLayer = Extract<Layer, { type: 'text' }>

const runFields = {
  preferred_font: 'font',
  font_weight: 'font_weight',
  font_style: 'font_style',
  size: 'size_scale',
  color: 'color',
  fill_gradient: 'fill_gradient',
  stroke_color: 'stroke_color',
  stroke_width: 'stroke_width',
  outer_stroke_color: 'outer_stroke_color',
  outer_stroke_width: 'outer_stroke_width',
} as const satisfies Partial<Record<keyof Typography, keyof RunStyle>>

export const emptyRunStyle: RunStyle = {
  font: null,
  font_weight: null,
  font_style: null,
  size_scale: null,
  color: null,
  fill_gradient: null,
  stroke_color: null,
  stroke_width: null,
  outer_stroke_color: null,
  outer_stroke_width: null,
}

// Include unstyled gaps: selecting a styled word and its plain neighbour is mixed too.
export function selectedTypography(
  base: Typography,
  runs: StyleRun[],
  selection: RichTextSelection,
): Typography[] {
  const styles: Array<RunStyle | null> = []
  let cursor = selection.start
  for (const run of runs) {
    if (run.end <= cursor) continue
    if (run.start >= selection.end) break
    if (run.start > cursor) styles.push(null)
    styles.push(run.style)
    cursor = Math.min(run.end, selection.end)
  }
  if (cursor < selection.end) styles.push(null)
  return styles.map((style) => ({
    ...base,
    preferred_font: style?.font ?? base.preferred_font ?? defaultFont.name,
    font_weight: style?.font_weight ?? base.font_weight ?? 400,
    font_style: style?.font_style ?? base.font_style ?? 'normal',
    size: (base.size ?? 24) * (style?.size_scale ?? 1),
    auto_fit: false,
    color: style?.color ?? base.color ?? defaultTypography.color,
    fill_gradient: style?.fill_gradient ?? base.fill_gradient,
    stroke_color: style?.stroke_color ?? base.stroke_color ?? defaultTypography.stroke_color,
    stroke_width: style?.stroke_width ?? base.stroke_width ?? 0,
    outer_stroke_color:
      style?.outer_stroke_color ?? base.outer_stroke_color ?? defaultTypography.outer_stroke_color,
    outer_stroke_width: style?.outer_stroke_width ?? base.outer_stroke_width ?? 0,
  }))
}

export function wordStylePatch(
  value: Typography,
  fields: readonly (keyof Typography)[],
  baseSize: number,
): RunStylePatch {
  const patch: RunStylePatch = { set: { ...emptyRunStyle }, clear: [] }
  for (const field of fields) {
    if (!(field in runFields)) continue
    const target = runFields[field as keyof typeof runFields]
    const next = field === 'size' && value.size !== null ? value.size / baseSize : value[field]
    if (next === null) patch.clear.push(target)
    else Object.assign(patch.set, { [target]: next })
  }
  return patch
}

export function useWordTypography(layers: TextLayer[]) {
  const selection = useRichTextSelection((state) => state.selection)
  const project = useProject().data
  const page = usePage().data
  const layer = layers.length === 1 ? layers[0] : undefined
  const text = layer?.content.translation?.text ?? layer?.content.source?.text ?? ''
  const active =
    layer &&
    selection?.layer === layer.id &&
    Number.isInteger(selection.start) &&
    Number.isInteger(selection.end) &&
    selection.start >= 0 &&
    selection.start < selection.end &&
    selection.end <= charLength(text)
      ? selection
      : null
  const runs = useQuery({
    queryKey: ['word-styles', project?.name, page?.id, project?.revision, layer?.id],
    queryFn: () => call(commands.getWordStyles, layer!.id),
    enabled: active !== null,
  })
  const mutation = useMutation({
    scope: { id: `word-styles:${project?.name}:${layer?.id}` },
    mutationFn: ({ selection, patch }: { selection: RichTextSelection; patch: RunStylePatch }) =>
      call(commands.setWordStyle, selection.layer, selection.start, selection.end, patch),
    onSuccess: () => refresh(projectKey, pageKey, ['word-styles'], ['box-effects']),
  })
  const clear = useMutation({
    mutationFn: (layer: string) => call(commands.clearWordStyles, [layer], null),
    onSuccess: () => refresh(projectKey, pageKey, ['word-styles'], ['box-effects']),
  })
  const base = layer?.typography ?? defaultTypography
  const values = active ? selectedTypography(base, runs.data ?? [], active) : []
  const apply: ApplyTypography = (update, fields = []) => {
    // An open colour picker can commit again while the previous edit is saving.
    // Its absolute patch is queued in order rather than silently dropping that edit.
    if (!active || !runs.data || runs.isError) return
    const patch = wordStylePatch(update(values[0]), fields, base.size ?? 24)
    if (patch.clear.length || Object.values(patch.set).some((value) => value !== null))
      mutation.mutate({ selection: active, patch })
  }
  return {
    selection: active,
    text,
    values,
    typography: values[0] ?? base,
    disabled: runs.isPending || runs.isFetching || runs.isError || mutation.isPending,
    inheritedGradient: !!base.fill_gradient,
    apply,
    clearing: clear.isPending,
    clearAll: () => {
      if (layer && !active && !clear.isPending) clear.mutate(layer.id)
    },
  }
}
