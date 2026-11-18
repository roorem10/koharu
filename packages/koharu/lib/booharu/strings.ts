import ar from '@/public/locales/ar/booharu.json'
import enUS from '@/public/locales/en-US/booharu.json'
import esES from '@/public/locales/es-ES/booharu.json'
import frFR from '@/public/locales/fr-FR/booharu.json'
import jaJP from '@/public/locales/ja-JP/booharu.json'
import koKR from '@/public/locales/ko-KR/booharu.json'
import ptBR from '@/public/locales/pt-BR/booharu.json'
import ruRU from '@/public/locales/ru-RU/booharu.json'
import trTR from '@/public/locales/tr-TR/booharu.json'
import wuu from '@/public/locales/wuu/booharu.json'
import zhCN from '@/public/locales/zh-CN/booharu.json'
import zhTW from '@/public/locales/zh-TW/booharu.json'

/** Booharu's interface strings per language, in public/locales/<lng>/booharu.json: its own under
 * `booharu` and `history.steps.booharu_*`, its tools' names at `tools.<tool>`, where Koharu's tool
 * rail looks a tool's name up, and an override of a Koharu string at Koharu's path where the
 * language's text must differ, such as to say Booharu or to add a plural form. Koharu's
 * translation.json files stay as upstream ships them; these are merged over them, so a Booharu
 * value wins where both name a key. */
export const booharuStrings: Record<string, object> = {
  'en-US': enUS,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  'ja-JP': jaJP,
  'ru-RU': ruRU,
  'es-ES': esES,
  'tr-TR': trTR,
  'ko-KR': koKR,
  'pt-BR': ptBR,
  wuu,
  'fr-FR': frFR,
  ar,
}

function isObject(value: unknown): value is object {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/** `upstream` with `overlay` deep-merged over it, as a new object: objects merge key by key, and
 * any other value in `overlay` replaces upstream's at the same path. */
export function mergeStrings<T extends object>(upstream: T, overlay: object): T {
  const merged = { ...upstream } as Record<string, unknown>
  for (const [key, value] of Object.entries(overlay)) {
    const base = merged[key]
    merged[key] = isObject(base) && isObject(value) ? mergeStrings(base, value) : value
  }
  return merged as T
}

/** Koharu's i18next resources with each language's Booharu strings merged over its translation. A
 * language without a booharu.json keeps Koharu's strings, and tests/booharu flags it. */
export function withBooharu<R extends Record<string, { translation: object }>>(resources: R): R {
  return Object.fromEntries(
    Object.entries(resources).map(([language, resource]) => [
      language,
      {
        ...resource,
        translation: mergeStrings(resource.translation, booharuStrings[language] ?? {}),
      },
    ]),
  ) as R
}
