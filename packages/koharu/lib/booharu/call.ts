'use client'

// Booharu's command helper. Koharu 0.90.0 took out lib/backend.ts, whose `call()` ran a desktop
// command, showed its failure as an error toast and threw it again, and runs every command as a
// query or a mutation (lib/queries.ts), whose caches show a failure as a toast. Koharu's `send()`
// runs a command whose answer nothing waits on, so it hands back nothing. Booharu's code awaits a
// command's answer and catches its failure in many places, so `call()` runs the command as a
// keyless mutation, as `send()` does, but hands the answer back: a failure toasts once, through
// Koharu's MutationCache, and the promise rejects with it as an Error, as lib/backend's did. Once
// the command settles, `call()` lets go of its mutation (`reset()`), whose gcTime is 0, so the
// cache drops it, answer and all, on the next timer turn (a 0 ms timeout); a mutation still
// observed would stay for the whole session, and with it every answer, such as the agent history
// read again after each turn.
//
// Where `send()` is enough, because nothing waits on the answer, use Koharu's `send()`. Inside a
// query's `queryFn` or a mutation's `mutationFn`, run the command directly (`commands.x(...)`),
// never through `call()`: their caches toast its failure already, and `call()` there would toast
// it twice. A read Booharu keeps quiet on purpose runs its command directly too, in a query
// carrying `meta: { booharuQuiet: true }`, which Koharu's QueryCache then shows no toast for.

import { MutationObserver } from '@tanstack/react-query'

import i18n from '@/lib/i18n'
import { queryClient } from '@/lib/queries'

/** A desktop command, as `commands` in `@koharu/ipc/protocol` lists them. */
type Command<Args extends unknown[], Result> = (...args: Args) => Promise<Result>

/** Runs `command` with `args` and answers what it answers. A failure shows as every mutation's
 * does, as one error toast, and rejects as an Error carrying the command's message. The mutation
 * is let go once the command settles, and the cache drops it on the next timer turn
 * (a 0 ms timeout). */
export function call<Args extends unknown[], Result>(
  command: Command<Args, Result>,
  ...args: Args
): Promise<Result> {
  const observer = new MutationObserver<Result, Error, void>(queryClient, {
    mutationFn: async () => {
      try {
        return await command(...args)
      } catch (error) {
        throw asError(error)
      }
    },
    gcTime: 0,
  })
  return observer.mutate().finally(() => observer.reset())
}

/** A command's failure as an Error: an Error as it is, a string, as the desktop rejects with its
 * error's message, as one with that message, and anything else as an unknown error. */
function asError(error: unknown): Error {
  if (error instanceof Error) return error
  return new Error(typeof error === 'string' ? error : i18n.t('errors.unknown'))
}
