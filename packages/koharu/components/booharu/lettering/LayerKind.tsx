'use client'

import { useTranslation } from 'react-i18next'

import { useBlockLettering } from '@/lib/booharu/boxLettering'
import { categoryName } from '@/lib/booharu/categories'
import { isBlockLayer } from '@/lib/document'
import { usePage } from '@/lib/queries'
import type { Layer, LayerId } from '@koharu/ipc/protocol'

/** A Layers row's second line: a block's lettering category, as 0.0.4's rows named a text's kind,
 * else `kind`, Koharu's name for the layer's kind, for any other layer and while the category is
 * read. */
export function LayerKind({ layer, kind }: { layer: Layer; kind: string }) {
  return isBlockLayer(layer) ? <BlockKind block={layer.id} kind={kind} /> : kind
}

/** The category of `block`, on the page the canvas shows, which Layers lists. */
function BlockKind({ block, kind }: { block: LayerId; kind: string }) {
  const { t } = useTranslation()
  const lettering = useBlockLettering(usePage().data?.id, block)
  return lettering ? t(categoryName(lettering.category)) : kind
}
