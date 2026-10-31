import type { EntityId } from '@koharu/bridge/protocol'

const none = new Map<EntityId, number>()

// Owned by WO-24: open QC issues per text box on a page, for the "?" highlight. Empty until
// WO-24 fills it in.
export function useBoxIssues(_page: EntityId | null): Map<EntityId, number> {
  return none
}
