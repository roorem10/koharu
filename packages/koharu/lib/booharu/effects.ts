'use client'

import { useQuery } from '@tanstack/react-query'

import { pagesKey } from '@/lib/queries'
import {
  commands,
  type LayerId,
  type PageId,
  type SkewAngles,
  type TextEffect,
  type TextEffects,
} from '@koharu/ipc/protocol'

// Booharu's text effects (S12-j, S12-k): what each text carries beyond its lettering default, as
// the fx tag names it, and the skew it shows, which Text effects' Skew fields show.

/** The text effects of a page's texts that carry any, by layer. */
export type PageEffects = ReadonlyMap<LayerId, TextEffects>

/** The key of `page`'s text effects, under Story's key for the page, so every edit, undo or redo,
 * which reads Story's pages again, reads them again too. */
export function effectsKey(page: PageId) {
  return [...pagesKey, page, 'booharu', 'effects'] as const
}

/** `effects` by layer; none for an answer of none. */
function byLayer(effects: TextEffects[] | null): PageEffects {
  return new Map((effects ?? []).map((entry) => [entry.layer, entry]))
}

/** The text effects of each text layer and translated block of `page` that carries any, by layer,
 * read while a view showing it is mounted and again after every change; none while `page` is. A
 * text with none isn't in it. A failed read shows no error toast, as one of a page just removed
 * fails as the page's views go; its views then show no effects. */
export function useTextEffects(page: PageId | null | undefined) {
  return useQuery({
    queryKey: effectsKey(page ?? ''),
    queryFn: () => commands.booharuTextEffects(page ?? ''),
    enabled: !!page,
    select: byLayer,
  })
}

/** The effects `layer` carries, as `effects` tell them: none for a text they leave out. */
export function effectsOf(effects: PageEffects | undefined, layer: LayerId): TextEffect[] {
  return effects?.get(layer)?.effects ?? []
}

/** `degrees` to the thousandth a number box shows, so angles that show alike are alike, and never
 * -0. */
const shownAngle = (degrees: number) => Math.round(degrees * 1000) / 1000 || 0

/** The skew `layer` shows, as `effects` tell it, to the thousandth of a degree: none for a text
 * they leave out. */
export function skewOf(effects: PageEffects, layer: LayerId): SkewAngles {
  const skew = effects.get(layer)?.skew
  return skew ? { x: shownAngle(skew.x), y: shownAngle(skew.y) } : { x: 0, y: 0 }
}

/** Whether one of `layers` shows a skew, as `effects` tell it. */
export function showsSkew(effects: PageEffects | undefined, layers: LayerId[]): boolean {
  return layers.some((layer) => effects?.get(layer)?.skew != null)
}

/** The skew `layers` share along each axis, as `effects` tell it: `null` along an axis where they
 * differ, and none along either for no layers. */
export function sharedSkew(
  effects: PageEffects,
  layers: LayerId[],
): { x: number | null; y: number | null } {
  const skews = layers.map((layer) => skewOf(effects, layer))
  const shared = (axis: keyof SkewAngles) => {
    const [first, ...rest] = skews
    if (!first) return 0
    return rest.every((skew) => skew[axis] === first[axis]) ? first[axis] : null
  }
  return { x: shared('x'), y: shared('y') }
}

/** `names` joined as `language` joins a list, Wu, which Intl may lack, as Mandarin does. */
export function joinNames(names: string[], language: string): string {
  const locales = language === 'wuu' ? ['wuu', 'zh-CN'] : [language]
  return new Intl.ListFormat(locales, { type: 'conjunction' }).format(names)
}
