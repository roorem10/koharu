'use client'

import { queryOptions, useQuery, type QueryKey } from '@tanstack/react-query'

import { call } from '@/lib/booharu/call'
import { preferencesKey, queryClient, refresh } from '@/lib/queries'
import { commands, type BooharuPreferences } from '@koharu/ipc/protocol'

/** Every Booharu settings section, a `[booharu-<area>]` table each, as the file holds them. */
export const booharuPreferencesKey = ['booharu', 'preferences'] as const

/** The read of every Booharu settings section, which every view showing one shares. A failure shows
 * as every failed read's does. */
export const booharuPreferencesQuery = queryOptions({
  queryKey: booharuPreferencesKey,
  queryFn: () => commands.booharuGetPreferences(),
})

/** Every Booharu settings section, read again each time a settings page showing one opens: the
 * agent's settings tool may have changed one since. */
export function useBooharuPreferences() {
  return useQuery({ ...booharuPreferencesQuery, refetchOnMount: 'always' })
}

/** The last write to the Booharu settings, which the next one waits for. */
let saving: Promise<unknown> = Promise.resolve()

/** Runs `write`, a write to the Booharu settings file, once the writes queued before it end. A
 * command that changes one key of a section, such as a font's star, goes through here too, so a
 * save, which writes every section as it read them just before, never puts back a value it
 * changed in between; so does one that reads what they write, such as Apply's count. */
export function queueBooharuWrite<T>(write: () => Promise<T>): Promise<T> {
  const done = saving.catch(() => undefined).then(write)
  saving = done
  return done
}

/** Saves the section `name` as `change` makes it from what the file holds just before the save,
 * with every other section as the file holds it then, so no section goes back to a value read
 * earlier. Saves run one after another, and the settings pages show what each leaves in the file,
 * or what the file holds when one fails. It shows no failure itself: it's for a mutation's
 * `mutationFn`, whose failure Koharu's MutationCache shows; elsewhere use `saveBooharuSection`. */
export function writeBooharuSection<Name extends keyof BooharuPreferences>(
  name: Name,
  change: (section: BooharuPreferences[Name]) => BooharuPreferences[Name],
): Promise<BooharuPreferences> {
  return queueBooharuWrite(async () => {
    const current = await commands.booharuGetPreferences()
    queryClient.setQueryData(booharuPreferencesKey, current)
    const saved = await commands.booharuSavePreferences({
      ...current,
      [name]: change(current[name]),
    })
    queryClient.setQueryData(booharuPreferencesKey, saved)
    return saved
  })
}

/** `writeBooharuSection`, queued at once, whose failure shows as every failed command's does, as
 * one error toast, and rejects as an Error. Inside a mutation's `mutationFn` use
 * `writeBooharuSection`, as the mutation shows its failure already. */
export function saveBooharuSection<Name extends keyof BooharuPreferences>(
  name: Name,
  change: (section: BooharuPreferences[Name]) => BooharuPreferences[Name],
): Promise<BooharuPreferences> {
  const written = writeBooharuSection(name, change)
  return call(() => written)
}

/** Resolves once no mutation keyed `key` is under way. */
function settled(key: QueryKey): Promise<void> {
  const busy = () => queryClient.isMutating({ mutationKey: key }) > 0
  if (!busy()) return Promise.resolve()
  return new Promise((resolve) => {
    const stop = queryClient.getMutationCache().subscribe(() => {
      if (busy()) return
      stop()
      resolve()
    })
  })
}

/** Reads Koharu's settings again, as after the desktop changed them without telling, once no save
 * of them is under way: such a save answers what the file holds itself, and a read before it ends
 * could show the settings as they were before it. */
export async function refreshPreferences(): Promise<void> {
  await settled(preferencesKey)
  await refresh(preferencesKey)
}
