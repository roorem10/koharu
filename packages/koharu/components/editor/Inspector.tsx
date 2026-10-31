'use client'

import type { TFunction } from 'i18next'
import {
  ArrowDown,
  ArrowUp,
  Brush,
  Eye,
  EyeOff,
  Folder,
  GripVertical,
  Image as ImageIcon,
  Layers3,
  Lock,
  RotateCcw,
  Trash2,
  Type,
} from 'lucide-react'
import { type DragEvent, type MouseEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { layerDropIndex } from '@/components/files/layer-order'
import { useBoxIssues } from '@/components/qc/useBoxIssues'
import { BoxDialogs } from '@/components/redesign/BoxDialogs'
import { RowTags } from '@/components/redesign/RowTags'
import { TypeInspector } from '@/components/redesign/TypeControls'
import { usePageEffects } from '@/components/redesign/usePageEffects'
import { call } from '@/lib/backend'
import { isGroupLayer, isLockedLayer, isTextLayer, layerChildren } from '@/lib/document'
import { pageKey, projectKey, queryClient, refresh, usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { previewCanvasOpacity } from '@koharu/bridge/canvas'
import { commands, type EntityId, type Layer } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { ScrollArea } from '@koharu/ui/components/scroll-area'
import { Slider } from '@koharu/ui/components/slider'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

export function Inspector() {
  const { t } = useTranslation()
  return (
    <aside className='flex h-full min-h-0 flex-col bg-[var(--surface-panel)]'>
      <div className='flex h-8 shrink-0 items-center gap-1.5 border-b border-border/80 px-2'>
        <Type className='size-3 text-primary' />
        <h2 className='text-[10px] font-semibold'>{t('inspector.type')}</h2>
      </div>

      <section className='min-w-0 shrink-0 border-b'>
        <TypeInspector />
      </section>

      <section className='flex min-h-0 flex-1 flex-col'>
        <LayersInspector />
        <BoxDialogs />
      </section>
    </aside>
  )
}

function displayedLayers(layers: Layer[], page: EntityId) {
  const indexes = new Map(layers.map((layer, index) => [layer.id, index]))
  const rows: { layer: Layer; index: number; depth: number }[] = []
  const append = (layer: Layer, depth: number) => {
    rows.push({ layer, index: indexes.get(layer.id) ?? 0, depth })
    if (!isGroupLayer(layer)) return
    const children = layerChildren(layers, layer.id)
    const ordered = layer.role === 'text' ? children : [...children].reverse()
    for (const child of ordered) append(child, depth + 1)
  }
  const roots = layers.filter((layer) => (layer.parent ?? page) === page).reverse()
  for (const layer of roots) append(layer, 0)
  return rows
}

function LayersInspector() {
  const { t } = useTranslation()
  const page = usePage().data
  const effects = usePageEffects(page)
  const issues = useBoxIssues(page?.id ?? null)
  const selected = useKoharuStore((state) => state.selectedLayers)
  const selectLayers = useKoharuStore((state) => state.selectLayers)
  const [expandedLayer, setExpandedLayer] = useState<EntityId | null>(
    selected.length === 1 ? (selected[0] ?? null) : null,
  )
  const [movingLayer, setMovingLayer] = useState<EntityId | null>(null)
  const drag = useRef<{ layer: EntityId; page: EntityId } | null>(null)
  const [dropTarget, setDropTarget] = useState<{ layer: EntityId; after: boolean } | null>(null)
  const anchor = useRef<EntityId | null>(null)

  useEffect(() => {
    setExpandedLayer(selected.length === 1 ? (selected[0] ?? null) : null)
  }, [selected])

  const layers = useMemo(() => (page ? displayedLayers(page.layers, page.id) : []), [page])

  if (!page) return <EmptyInspector>{t('inspector.selectPage')}</EmptyInspector>

  const reorder = (layer: EntityId, parent: EntityId, index: number) => {
    setMovingLayer(layer)
    void call(commands.moveLayer, layer, parent, index).then(
      (next) => {
        queryClient.setQueryData(pageKey, next)
        setMovingLayer(null)
        void refresh(projectKey)
      },
      () => setMovingLayer(null),
    )
  }

  const dragPosition = (layer: Layer, event: DragEvent<HTMLDivElement>) => {
    if (!drag.current || drag.current.page !== page.id || movingLayer !== null) return null
    const rect = event.currentTarget.getBoundingClientRect()
    const after = event.clientY > rect.top + rect.height / 2
    const position = layerDropIndex(page.layers, page.id, drag.current.layer, layer.id, after)
    return position ? { ...position, after } : null
  }

  const endDrag = () => {
    drag.current = null
    setDropTarget(null)
  }

  const move = (layer: Layer, displayDelta: number) => {
    if (movingLayer !== null || isLockedLayer(layer)) return
    const parent = layer.parent ?? page.id
    const storedSiblings = page.layers.filter(
      (candidate) => !isLockedLayer(candidate) && (candidate.parent ?? page.id) === parent,
    )
    const parentLayer = page.layers.find((candidate) => candidate.id === parent)
    const shownSiblings =
      parentLayer && isGroupLayer(parentLayer) && parentLayer.role === 'text'
        ? storedSiblings
        : [...storedSiblings].reverse()
    const shownSource = shownSiblings.findIndex((candidate) => candidate.id === layer.id)
    const shownTarget = shownSource + displayDelta
    const targetLayer = shownSiblings[shownTarget]
    if (shownSource < 0 || !targetLayer) return
    const target = storedSiblings.findIndex((candidate) => candidate.id === targetLayer.id)

    reorder(layer.id, parent, target)
  }

  const deleteLayer = (layer: EntityId) =>
    void call(commands.deleteLayers, [layer])
      .then(() => {
        if (selected.includes(layer)) {
          selectLayers(selected.filter((selectedLayer) => selectedLayer !== layer))
        }
        setExpandedLayer((current) => (current === layer ? null : current))
        return refresh(projectKey, pageKey)
      })
      .catch(() => undefined)

  // Modifier semantics mirror the page rail: ctrl/meta toggles, shift selects a display range.
  const selectLayer = (layer: EntityId, additive: boolean, range: boolean) => {
    if (!additive && !range) {
      anchor.current = layer
      if (selected.length === 1 && selected[0] === layer) {
        setExpandedLayer((current) => (current === layer ? null : layer))
        return
      }
      selectLayers([layer])
      setExpandedLayer(layer)
      return
    }
    const order = layers.map((row) => row.layer.id)
    const anchorIndex = anchor.current ? order.indexOf(anchor.current) : -1
    if (range && anchorIndex >= 0) {
      const targetIndex = order.indexOf(layer)
      const span = layers
        .slice(Math.min(anchorIndex, targetIndex), Math.max(anchorIndex, targetIndex) + 1)
        .filter((row) => !isLockedLayer(row.layer))
        .map((row) => row.layer.id)
      selectLayers(additive ? [...selected, ...span] : span)
      return
    }
    anchor.current = layer
    if (!additive) {
      selectLayers([layer])
      return
    }
    selectLayers(
      selected.includes(layer)
        ? selected.filter((selectedLayer) => selectedLayer !== layer)
        : [...selected, layer],
    )
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <header className='flex h-8 shrink-0 items-center gap-1.5 border-b border-border/80 px-2'>
        <Layers3 className='size-3 text-primary' />
        <h2 className='text-[10px] font-semibold'>{t('layers.title')}</h2>
        <span className='text-[9px] text-muted-foreground tabular-nums'>
          {page.layers.filter((layer) => !isGroupLayer(layer)).length}
        </span>
      </header>

      <ScrollArea className='min-h-0 flex-1'>
        <div className='py-0.5'>
          {layers.map(({ layer, index, depth }) => {
            const locked = isLockedLayer(layer)
            const storedSiblings = page.layers.filter(
              (candidate) =>
                !isLockedLayer(candidate) &&
                (candidate.parent ?? page.id) === (layer.parent ?? page.id),
            )
            const parentLayer = page.layers.find((candidate) => candidate.id === layer.parent)
            const siblings =
              parentLayer && isGroupLayer(parentLayer) && parentLayer.role === 'text'
                ? storedSiblings
                : [...storedSiblings].reverse()
            const position = siblings.findIndex((candidate) => candidate.id === layer.id)
            return (
              <LayerRow
                key={`${layer.type}:${layer.id}`}
                layer={layer}
                hasEffects={effects.get(layer.id) ?? false}
                issues={issues.get(layer.id) ?? 0}
                index={index}
                depth={depth}
                selected={selected.includes(layer.id)}
                expanded={!locked && expandedLayer === layer.id}
                locked={locked}
                onSelect={(event) =>
                  selectLayer(layer.id, event.ctrlKey || event.metaKey, event.shiftKey)
                }
                onToggle={() =>
                  void call(commands.setVisibility, [layer.id], !layer.visibility.visible, null)
                    .then(() => refresh(projectKey, pageKey))
                    .catch(() => undefined)
                }
                onMove={(delta) => move(layer, delta)}
                canMoveUp={!locked && position > 0}
                canMoveDown={!locked && position >= 0 && position < siblings.length - 1}
                reordering={movingLayer !== null}
                dropSide={
                  dropTarget?.layer === layer.id ? (dropTarget.after ? 'after' : 'before') : null
                }
                onDragStart={(event) => {
                  if (locked || movingLayer !== null) {
                    event.preventDefault()
                    return
                  }
                  drag.current = { layer: layer.id, page: page.id }
                  event.dataTransfer.effectAllowed = 'move'
                  event.dataTransfer.setData('application/x-booharu-layer', layer.id)
                }}
                onDragEnd={endDrag}
                onDragLeave={() => setDropTarget(null)}
                onDragOver={(event) => {
                  const position = dragPosition(layer, event)
                  if (!position) {
                    setDropTarget(null)
                    return
                  }
                  event.preventDefault()
                  event.dataTransfer.dropEffect = 'move'
                  setDropTarget({ layer: layer.id, after: position.after })
                }}
                onDrop={(event) => {
                  const position = dragPosition(layer, event)
                  const source = drag.current?.layer
                  endDrag()
                  if (!source || !position) return
                  event.preventDefault()
                  reorder(source, position.parent, position.index)
                }}
                onDelete={isGroupLayer(layer) ? undefined : () => deleteLayer(layer.id)}
              />
            )
          })}
          {layers.length === 0 && <EmptyInspector>{t('layers.empty')}</EmptyInspector>}
        </div>
      </ScrollArea>
    </div>
  )
}

function LayerRow({
  layer,
  hasEffects,
  issues,
  index,
  depth,
  selected,
  expanded,
  locked,
  onSelect,
  onToggle,
  onMove,
  canMoveUp,
  canMoveDown,
  reordering,
  dropSide,
  onDragStart,
  onDragEnd,
  onDragLeave,
  onDragOver,
  onDrop,
  onDelete,
}: {
  layer: Layer
  hasEffects: boolean
  issues: number
  index: number
  depth: number
  selected: boolean
  expanded: boolean
  locked: boolean
  onSelect: (event: MouseEvent<HTMLButtonElement>) => void
  onToggle: () => void
  onMove: (delta: number) => void
  canMoveUp: boolean
  canMoveDown: boolean
  reordering: boolean
  dropSide: 'before' | 'after' | null
  onDragStart: (event: DragEvent<HTMLButtonElement>) => void
  onDragEnd: () => void
  onDragLeave: () => void
  onDragOver: (event: DragEvent<HTMLDivElement>) => void
  onDrop: (event: DragEvent<HTMLDivElement>) => void
  onDelete?: () => void
}) {
  const { t } = useTranslation()
  const name = localizedLayerName(layer, index, t)
  const detail = localizedLayerKind(layer, t)
  const Icon = layerIcon(layer)
  return (
    <div className='group min-w-0 px-1 py-px' style={{ paddingLeft: `${depth * 10 + 4}px` }}>
      <div
        data-selected={selected}
        data-expanded={expanded}
        className='min-w-0 overflow-hidden rounded-lg transition-colors duration-150 data-[selected=true]:bg-accent motion-reduce:transition-none'
      >
        <div
          className='relative flex min-w-0 items-center gap-0.5'
          data-layer-row={layer.id}
          onDragOver={onDragOver}
          onDrop={onDrop}
          onDragLeave={(event) => {
            if (
              event.relatedTarget instanceof Node &&
              event.currentTarget.contains(event.relatedTarget)
            )
              return
            onDragLeave()
          }}
        >
          {dropSide && (
            <span
              aria-hidden='true'
              className={`pointer-events-none absolute inset-x-0 z-20 h-0.5 bg-primary ${dropSide === 'before' ? 'top-0' : 'bottom-0'}`}
            />
          )}
          {!locked && (
            <button
              type='button'
              draggable={!reordering}
              disabled={reordering}
              aria-label={t('files.dragLayer', { name })}
              className='cursor-grab px-0.5 text-muted-foreground active:cursor-grabbing'
              onDragStart={onDragStart}
              onDragEnd={onDragEnd}
            >
              <GripVertical className='size-3' />
            </button>
          )}
          <button
            type='button'
            aria-label={t('layers.edit', { name })}
            aria-expanded={locked ? undefined : expanded}
            disabled={locked}
            className='flex min-w-0 flex-1 items-center gap-1.5 rounded-lg px-1.5 py-1 text-left hover:bg-foreground/[0.05] focus-visible:ring-2 focus-visible:ring-ring/25'
            onClick={onSelect}
          >
            <Icon className='size-3.5 shrink-0 text-muted-foreground' />
            <span className='min-w-0 flex-1'>
              <span className='block truncate text-[11px] font-medium'>{name}</span>
              <span className='block truncate text-[9px] leading-3 text-muted-foreground capitalize'>
                {detail}
              </span>
            </span>
          </button>
          {!locked && (
            <div className='pointer-events-none absolute top-1/2 left-6 z-10 flex -translate-y-1/2 rounded-md bg-background/90 p-0.5 opacity-0 shadow-sm ring-1 ring-border/40 transition-opacity duration-150 group-hover:pointer-events-auto group-hover:opacity-100 focus-within:pointer-events-auto focus-within:opacity-100 motion-reduce:transition-none'>
              <button
                type='button'
                aria-label={t('layers.moveUp', { name })}
                disabled={reordering || !canMoveUp}
                className='grid size-5 place-items-center rounded-sm text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/25 disabled:pointer-events-none disabled:opacity-30'
                onClick={() => onMove(-1)}
              >
                <ArrowUp className='size-3' />
              </button>
              <button
                type='button'
                aria-label={t('layers.moveDown', { name })}
                disabled={reordering || !canMoveDown}
                className='grid size-5 place-items-center rounded-sm text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/25 disabled:pointer-events-none disabled:opacity-30'
                onClick={() => onMove(1)}
              >
                <ArrowDown className='size-3' />
              </button>
            </div>
          )}
          {isTextLayer(layer) && (
            <RowTags layer={layer.id} name={name} hasEffects={hasEffects} issues={issues} />
          )}
          <span className='w-7 shrink-0 text-right text-[9px] text-muted-foreground tabular-nums'>
            {Math.round(layer.visibility.opacity * 100)}%
          </span>
          {locked ? (
            <Tooltip>
              <TooltipTrigger
                render={
                  <span
                    role='img'
                    className='grid size-6 shrink-0 place-items-center text-muted-foreground'
                    aria-label={t('layers.lockedLabel', { name })}
                  />
                }
              >
                <Lock className='size-3.5' />
              </TooltipTrigger>
              <TooltipContent side='left'>{t('layers.locked')}</TooltipContent>
            </Tooltip>
          ) : (
            <button
              type='button'
              aria-label={
                layer.visibility.visible ? t('layers.hide', { name }) : t('layers.show', { name })
              }
              className='grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/25'
              onClick={onToggle}
            >
              {layer.visibility.visible ? (
                <Eye className='size-3.5' />
              ) : (
                <EyeOff className='size-3.5' />
              )}
            </button>
          )}
        </div>
        {expanded && (
          <div className='animate-in duration-150 fade-in slide-in-from-top-1 motion-reduce:animate-none'>
            <LayerEditor layer={layer} onDelete={onDelete} />
          </div>
        )}
      </div>
    </div>
  )
}

function LayerEditor({ layer, onDelete }: { layer: Layer; onDelete?: () => void }) {
  const { t } = useTranslation()
  const name = localizedLayerName(layer, 0, t)
  const [opacity, setOpacity] = useState(layer.visibility.opacity * 100)
  const page = usePage().data
  const frames = useKoharuStore((state) => state.layerFrames)
  const canvasPage = useKoharuStore((state) => state.canvasPage)
  const revision = useKoharuStore((state) => state.canvasRevision)
  const [fixing, setFixing] = useState(false)
  const parent = page?.layers.find((candidate) => candidate.id === layer.parent)
  // A page has one text group; the automatic region identifies the actual bubble.
  const siblings =
    isTextLayer(layer) &&
    layer.automatic_region &&
    parent?.type === 'group' &&
    parent.role === 'text'
      ? layerChildren(page?.layers ?? [], parent.id)
          .filter(isTextLayer)
          .filter((candidate) => candidate.automatic_region === layer.automatic_region)
      : []
  const automatic =
    siblings.length >= 2 && canvasPage === page?.id && revision !== null
      ? siblings
          .filter((candidate) => !candidate.geometry && frames[candidate.id])
          .map((candidate) => ({ element: candidate.id, frame: frames[candidate.id] }))
      : []

  const fixBubbleFrames = () => {
    if (fixing || revision === null || automatic.length === 0) return
    setFixing(true)
    // Reuse the canvas commit's native contour conversion and single undo entry.
    void call(commands.commitTransform, revision, automatic, true)
      .then(() => refresh(projectKey, pageKey))
      .catch(() => undefined)
      .finally(() => setFixing(false))
  }

  useEffect(() => {
    setOpacity(layer.visibility.opacity * 100)
  }, [layer.id, layer.visibility.opacity])

  const commitOpacity = (next: number) => {
    void call(commands.setVisibility, [layer.id], null, next / 100)
      .then(() => refresh(projectKey, pageKey))
      .catch(() => {
        setOpacity(layer.visibility.opacity * 100)
        previewCanvasOpacity(layer.id, null)
      })
  }

  const previewOpacity = (next: number) => {
    setOpacity(next)
    previewCanvasOpacity(layer.id, next / 100)
  }

  const resetTextFrame = () => {
    if (!isTextLayer(layer) || !layer.automatic_region || !layer.geometry) return
    void call(commands.setGeometry, [{ layer: layer.id, points: null }])
      .then(() => refresh(projectKey, pageKey))
      .catch(() => undefined)
  }

  return (
    <div className='grid min-w-0 gap-1.5 px-1.5 pt-0.5 pb-1.5'>
      <div className='flex min-w-0 items-center gap-1.5'>
        <span className='shrink-0 text-[8px] font-medium text-muted-foreground uppercase'>
          {t('inspector.opacity')}
        </span>
        <div className='flex min-w-0 flex-1 items-center gap-1.5'>
          <Slider
            aria-label={t('layers.opacityLabel', { name })}
            min={0}
            max={100}
            step={1}
            value={opacity}
            className='[&_[data-slot=slider-thumb]]:size-2'
            onValueChange={previewOpacity}
            onValueCommitted={commitOpacity}
          />
          <span className='w-7 shrink-0 text-right text-[8px] text-muted-foreground tabular-nums'>
            {Math.round(opacity)}%
          </span>
        </div>
        {onDelete && (
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type='button'
                  variant='ghost'
                  size='icon-xs'
                  aria-label={t('layers.delete', { name })}
                  className='size-5 rounded-md text-muted-foreground hover:text-foreground'
                  onClick={onDelete}
                />
              }
            >
              <Trash2 className='size-3' />
            </TooltipTrigger>
            <TooltipContent side='left'>{t('layers.delete', { name })}</TooltipContent>
          </Tooltip>
        )}
      </div>
      {isTextLayer(layer) && (
        <>
          <div className='flex h-5 min-w-0 items-center gap-1.5'>
            <span className='text-[8px] font-medium text-muted-foreground uppercase'>
              {t('inspector.placement')}
            </span>
            <span className='rounded-md bg-foreground/[0.055] px-1.5 py-0.5 text-[9px] leading-none text-foreground/75'>
              {layer.geometry
                ? t('inspector.customFrame')
                : layer.automatic_region
                  ? t('inspector.autoFit')
                  : t('inspector.unplaced')}
            </span>
            {automatic.length > 0 && (
              <Tooltip>
                <TooltipTrigger
                  render={
                    <Button
                      type='button'
                      variant='ghost'
                      size='icon-xs'
                      className='ml-auto size-5 shrink-0 rounded-md text-muted-foreground hover:text-foreground'
                      aria-label={t('inspector.fixBubbleFrames')}
                      disabled={fixing}
                      onClick={fixBubbleFrames}
                    />
                  }
                >
                  <Lock className='size-3' />
                </TooltipTrigger>
                <TooltipContent side='left'>{t('inspector.fixBubbleFramesHelp')}</TooltipContent>
              </Tooltip>
            )}
            {layer.geometry && layer.automatic_region && (
              <Tooltip>
                <TooltipTrigger
                  render={
                    <Button
                      type='button'
                      variant='ghost'
                      size='xs'
                      aria-label={t('inspector.resetAutoFit')}
                      className='ml-auto h-5 gap-1 rounded-md px-1.5 text-[9px] font-normal text-muted-foreground hover:text-foreground'
                      onClick={resetTextFrame}
                    />
                  }
                >
                  <RotateCcw className='size-3' />
                  {t('common.reset')}
                </TooltipTrigger>
                <TooltipContent side='left'>{t('inspector.resetAutoFit')}</TooltipContent>
              </Tooltip>
            )}
          </div>
        </>
      )}
    </div>
  )
}

function EmptyInspector({ children }: { children: React.ReactNode }) {
  return (
    <div className='px-4 py-8 text-center text-[10px] leading-4 text-muted-foreground'>
      {children}
    </div>
  )
}

function layerIcon(layer: Layer): typeof Type {
  if (layer.type === 'group') return Folder
  if (layer.type === 'raster') return Brush
  if (layer.type === 'text') return Type
  return ImageIcon
}

function localizedLayerName(layer: Layer, index: number, t: TFunction): string {
  if (layer.type === 'group' || layer.type === 'raster') return layer.name
  if (layer.type === 'text') {
    const text = layer.content.translation?.text || layer.content.source?.text
    return text?.trim() || t('layers.textName', { index: index + 1 })
  }
  if (layer.type === 'artwork') return t('layers.originalArtwork')
  return t('layers.imageName', { index: index + 1 })
}

function localizedLayerKind(layer: Layer, t: TFunction): string {
  if (layer.type === 'group') {
    return t(layer.role === 'text' ? 'layers.kinds.textGroup' : 'layers.kinds.group')
  }
  if (layer.type === 'raster') {
    return t(`layers.kinds.${layer.kind}`, { defaultValue: layer.kind })
  }
  if (layer.type === 'text') {
    const role = layer.content.role?.split('.').at(-1)
    if (role === 'dialogue') return t('layers.kinds.dialogue')
    if (role === 'free-text') return t('layers.kinds.freeText')
    return t('layers.kinds.text')
  }
  return t('layers.kinds.image')
}
