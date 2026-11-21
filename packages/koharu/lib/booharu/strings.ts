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

/** The type of `B` deep-merged over `A`, as `mergeStrings` merges them: objects merge key by key,
 * and any other value of `B` replaces `A`'s at the same path. */
export type MergedStrings<A, B> = A extends object
  ? B extends object
    ? {
        [K in keyof A | keyof B]: K extends keyof B
          ? K extends keyof A
            ? MergedStrings<A[K], B[K]>
            : B[K]
          : K extends keyof A
            ? A[K]
            : never
      }
    : B
  : B

/** Koharu's i18next resources `R` with en-US's booharu.json merged into each language's
 * translation, so i18next's typed keys (lib/i18n.ts types them from en-US) take every Booharu
 * string and every key a booharu.json adds at Koharu's paths, such as `settings.tabs.<tab>` for a
 * Booharu tab. Every language has each of them: Booharu's own from its booharu.json, as
 * tests/booharu/strings.test.ts checks, and the rest from its translation.json. */
export type WithBooharu<R extends Record<string, { translation: object }>> = {
  [L in keyof R]: {
    [K in keyof R[L]]: K extends 'translation' ? MergedStrings<R[L][K], typeof enUS> : R[L][K]
  }
}

/** Koharu's i18next resources with each language's Booharu strings merged over its translation. A
 * language without a booharu.json keeps Koharu's strings, and tests/booharu flags it. */
export function withBooharu<R extends Record<string, { translation: object }>>(
  resources: R,
): WithBooharu<R> {
  return Object.fromEntries(
    Object.entries(resources).map(([language, resource]) => [
      language,
      {
        ...resource,
        translation: mergeStrings(resource.translation, booharuStrings[language] ?? {}),
      },
    ]),
  ) as WithBooharu<R>
}
