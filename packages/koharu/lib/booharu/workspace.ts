// Koharu's store keeps the tool picked between runs, in its 'koharu.workspace' (lib/store.ts), and
// the canvas takes only a tool it has: a kept one it lacks, as the O tool's `ocr_box`, which the
// test builds of 0.0.5 kept before the O tool folded into K, would fail the canvas's set_tool each
// time the editor opens, with an error toast naming it, and leave no tool shown as picked. So a
// kept tool the app doesn't have reads as none, leaving the store's own, Select at the start, as a
// kept brush value out of range reads as none (lib/booharu/brush.ts).

import { toolKeys } from '@/lib/shortcuts'
import type { Tool } from '@koharu/ipc/protocol'

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/** Whether `tool` is one of the app's tools, which Koharu's `toolKeys` lists. */
export function isTool(tool: unknown): tool is Tool {
  return typeof tool === 'string' && Object.prototype.hasOwnProperty.call(toolKeys, tool)
}

/** Koharu's kept workspace, `kept`, over the store's `current` state, merged shallowly as
 * persist's own merge does, but for a kept tool the app doesn't have, which leaves `current`'s. */
export function mergeWorkspace<S extends { tool: Tool }>(kept: unknown, current: S): S {
  if (!isRecord(kept)) return current
  const { tool, ...rest } = kept
  return { ...current, ...rest, ...(isTool(tool) ? { tool } : {}) }
}
