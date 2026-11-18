'use client'

import { useQuery } from '@tanstack/react-query'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands, type BooharuPreferences } from '@koharu/ipc/protocol'

/** Every Booharu settings section, a `[booharu-<area>]` table each, as the file holds them. */
export const booharuPreferencesKey = ['booharu', 'preferences'] as const

/** Every Booharu settings section, read again each time a settings page showing one opens: the
 * agent's settings tool may have changed one since. */
export function useBooharuPreferences() {
  return useQuery({
    queryKey: booharuPreferencesKey,
    queryFn: () => call(commands.booharuGetPreferences),
    refetchOnMount: 'always',
  })
}

/** The last write to the Booharu settings, which the next one waits for. */
let saving: Promise<unknown> = Promise.resolve()

/** Runs `write`, a write to the Booharu settings file, once the writes queued before it end. A
 * command that changes one key of a section, such as a font's star, goes through here too, so a
 * save, which writes every section as it read them just before, never puts back a value it
 * changed in between. */
export function queueBooharuWrite<T>(write: () => Promise<T>): Promise<T> {
  const done = saving.catch(() => undefined).then(write)
  saving = done
  return done
}

/** Saves the section `name` as `change` makes it from what the file holds just before the save,
 * with every other section as the file holds it then, so no section goes back to a value read
 * earlier. Saves run one after another, and the settings pages show what each leaves in the file,
 * or what the file holds when one fails; a failure shows as every failed command does. */
export function saveBooharuSection<Name extends keyof BooharuPreferences>(
  name: Name,
  change: (section: BooharuPreferences[Name]) => BooharuPreferences[Name],
): Promise<BooharuPreferences> {
  return queueBooharuWrite(async () => {
    const current = await call(commands.booharuGetPreferences)
    queryClient.setQueryData(booharuPreferencesKey, current)
    const saved = await call(commands.booharuSavePreferences, {
      ...current,
      [name]: change(current[name]),
    })
    queryClient.setQueryData(booharuPreferencesKey, saved)
    return saved
  })
}
