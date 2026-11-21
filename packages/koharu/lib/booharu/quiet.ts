import type { Query } from '@tanstack/react-query'

// Booharu's quiet reads. Koharu 0.90.0 runs every read as a query whose failure Koharu's QueryCache
// shows as an error toast (lib/queries.ts). A few of Booharu's reads stay quiet on purpose, as they
// did before: a read the view shows its own way, such as an empty Context tab, or one that only
// adds to what Koharu shows, such as the fonts' names in other languages. Such a read runs its
// command directly in its `queryFn` and carries `meta: { booharuQuiet: true }`.

declare module '@tanstack/react-query' {
  interface Register {
    /** `booharuQuiet` keeps a read's failure from showing as a toast (lib/booharu/quiet.ts). */
    queryMeta: { booharuQuiet?: boolean }
  }
}

/** Koharu's QueryCache `onError`, `report`, for every query but one Booharu keeps quiet. */
export function unlessQuiet(report: (error: unknown) => void) {
  return (error: unknown, query: Query<unknown, unknown, unknown>) => {
    if (!query.meta?.booharuQuiet) report(error)
  }
}
