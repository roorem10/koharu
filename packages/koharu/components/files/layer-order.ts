import { isGroupLayer, isLockedLayer } from '@/lib/document'
import type { EntityId, Layer } from '@koharu/bridge/protocol'

export function layerDropIndex(
  layers: Layer[],
  page: EntityId,
  source: EntityId,
  target: EntityId,
  after: boolean,
) {
  const moving = layers.find((layer) => layer.id === source)
  const destination = layers.find((layer) => layer.id === target)
  if (
    !moving ||
    !destination ||
    source === target ||
    isLockedLayer(moving) ||
    isLockedLayer(destination)
  )
    return null
  const parent = moving.parent ?? page
  if ((destination.parent ?? page) !== parent) return null
  const siblings = layers.filter(
    (layer) => !isLockedLayer(layer) && (layer.parent ?? page) === parent,
  )
  const group = layers.find((layer) => layer.id === parent)
  const forward = group && isGroupLayer(group) && group.role === 'text'
  const remaining = (forward ? siblings : [...siblings].reverse()).filter(
    (layer) => layer.id !== source,
  )
  const insertion = remaining.findIndex((layer) => layer.id === target) + (after ? 1 : 0)
  const index = forward ? insertion : remaining.length - insertion
  return index === siblings.findIndex((layer) => layer.id === source) ? null : { parent, index }
}
