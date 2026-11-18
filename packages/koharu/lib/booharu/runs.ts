'use client'

import { useQuery } from '@tanstack/react-query'
import type { TFunction } from 'i18next'
import { useCallback, useRef, useState } from 'react'

import { prepareFinishSound } from '@/components/booharu/runs/sound'
import { call } from '@/lib/backend'
import {
  booharuPreferencesKey,
  booharuPreferencesQuery,
  saveBooharuSection,
  useBooharuPreferences,
} from '@/lib/booharu/preferences'
import i18n from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { pipelineStages, useKoharuStore } from '@/lib/store'
import {
  commands,
  type BooharuPreferences,
  type RunPreset,
  type RunsSettings,
  type RunTarget,
  type Stage,
  type Target,
} from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** `[booharu-runs]` with every value, as the desktop answers it. */
export type Runs = Required<RunsSettings>

/** The most presets the section keeps, as booharu-core checks it. */
export const MAX_PRESETS = 100
/** The most times a failed page stage is tried again, as booharu-core checks it. */
export const MAX_RETRIES = 5

/** The presets the section holds while the file sets none, as booharu-core's defaults are, each
 * with the key of the name it shows, translated, while its name and stages are still these. */
const builtInPresets = [
  {
    name: 'Detect + OCR',
    stages: ['detect', 'recognize'],
    label: 'booharu.runs.presets.builtIn.detectOcr',
  },
  {
    name: 'Translate + clean up',
    stages: ['inpaint', 'translate'],
    label: 'booharu.runs.presets.builtIn.translateCleanup',
  },
] as const satisfies ReadonlyArray<{ name: string; stages: readonly Stage[]; label: string }>

/** The section while the file sets nothing, as booharu-core's defaults are. */
const builtInRuns: Runs = {
  presets: builtInPresets.map(({ name, stages }) => ({ name, stages: [...stages] })),
  retries: 2,
  on_page_failure: 'continue',
  order: 'pages',
  finish_sound: true,
}

/** `section` with every value: one the desktop leaves out is the built-in one, as booharu-core
 * reads a missing key. */
function complete(section: RunsSettings | undefined): Runs {
  return {
    presets: section?.presets ?? builtInRuns.presets,
    retries: section?.retries ?? builtInRuns.retries,
    on_page_failure: section?.on_page_failure ?? builtInRuns.on_page_failure,
    order: section?.order ?? builtInRuns.order,
    finish_sound: section?.finish_sound ?? builtInRuns.finish_sound,
  }
}

/** `stages` in the order a page's stages run, each once, as a run takes them. */
export function inStageOrder(stages: readonly Stage[]): Stage[] {
  return pipelineStages.filter((stage) => stages.includes(stage))
}

/** Whether `a` and `b` hold the same stages, in whatever order. */
export function sameStages(a: readonly Stage[], b: readonly Stage[]): boolean {
  return inStageOrder(a).join() === inStageOrder(b).join()
}

/** The name `preset` shows: a built-in preset's, translated, while its name and stages are still
 * the built-in ones, otherwise its own. */
export function presetLabel(preset: RunPreset, t: TFunction): string {
  const builtIn = builtInPresets.find(
    (candidate) => candidate.name === preset.name && sameStages(candidate.stages, preset.stages),
  )
  return builtIn ? t(builtIn.label) : preset.name
}

/** Why `name` can't name a preset among `presets`, leaving out the one named `renamed`, which it
 * renames, as the message that says so: it's blank once trimmed, or another preset has it, or
 * shows it, translated, as no two presets may share a name. Null when it can. */
export function presetNameError(
  name: string,
  presets: readonly RunPreset[],
  t: TFunction,
  renamed?: string,
): string | null {
  const trimmed = name.trim()
  if (!trimmed) return t('booharu.runs.presets.blank')
  const taken = presets.some(
    (preset) =>
      preset.name !== renamed &&
      (preset.name.trim() === trimmed || presetLabel(preset, t) === trimmed),
  )
  return taken ? t('booharu.runs.presets.duplicate', { name: trimmed }) : null
}

/** Whether `retries` is a number of retries the section takes: a whole number from 0 to 5. */
export function isRetries(retries: number | null): retries is number {
  return retries !== null && Number.isInteger(retries) && retries >= 0 && retries <= MAX_RETRIES
}

/** A change to the section, made to what the file holds just before it's saved. Presets are
 * found by name, which no two share, so a change goes to the preset it was made on even when
 * another writer has added or removed one since. */
export type RunsEdit = (runs: Runs) => Runs

/** Adds a preset named `name`, trimmed, with `stages`, after the others. */
export function addPreset(name: string, stages: readonly Stage[]): RunsEdit {
  return (runs) => ({
    ...runs,
    presets: [...runs.presets, { name: name.trim(), stages: inStageOrder(stages) }],
  })
}

/** Renames the preset named `from` to `to`, trimmed. */
export function renamePreset(from: string, to: string): RunsEdit {
  return (runs) => ({
    ...runs,
    presets: runs.presets.map((preset) =>
      preset.name === from ? { ...preset, name: to.trim() } : preset,
    ),
  })
}

/** Switches `stage` on or off in the preset named `name`, leaving it at least one stage. */
export function tickPresetStage(name: string, stage: Stage, on: boolean): RunsEdit {
  return (runs) => ({
    ...runs,
    presets: runs.presets.map((preset) => {
      if (preset.name !== name) return preset
      const stages = on
        ? inStageOrder([...preset.stages, stage])
        : preset.stages.filter((candidate) => candidate !== stage)
      return stages.length ? { ...preset, stages } : preset
    }),
  })
}

/** Deletes the preset named `name`. */
export function deletePreset(name: string): RunsEdit {
  return (runs) => ({ ...runs, presets: runs.presets.filter((preset) => preset.name !== name) })
}

/** Sets the run policy's keys, or the finish sound, that `values` name. */
export function setRuns(values: Partial<Omit<Runs, 'presets'>>): RunsEdit {
  return (runs) => ({ ...runs, ...values })
}

/** Saves `edit`, made to the section as the file holds it just before the save, with every other
 * section as the file holds it then, and answers the section as saved. Saves run one after
 * another, and a failure shows as every failed command's does. */
export async function saveRuns(edit: RunsEdit): Promise<Runs> {
  const saved = await saveBooharuSection('booharu-runs', (section) => edit(complete(section)))
  return complete(saved['booharu-runs'])
}

/** The section as the last read or save left it in the shared settings. */
function readRuns(): Runs {
  const preferences = queryClient.getQueryData<BooharuPreferences>(booharuPreferencesKey)
  return complete(preferences?.['booharu-runs'])
}

/** `[booharu-runs]` as the settings file holds it, read again each time a view showing it opens,
 * with the changes being saved over it, and `change`, which saves one at once over what the file
 * holds just before, so a key or a preset another writer changed meanwhile stays as that writer
 * left it. Once the last save ends, what the file holds shows again, so after a failed save, whose
 * error shows as every failed command's does, the file's values show. `runs` is undefined until
 * the first read ends, and `failed` says whether it failed. */
export function useRunsSettings() {
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<Runs | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still being saved. */
  const changes = useRef(0)

  const change = useCallback((edit: RunsEdit) => {
    changes.current += 1
    const last = changes.current
    setUnsaved((shown) => edit(shown ?? readRuns()))
    void saveRuns(edit)
      .catch(() => undefined)
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }, [])

  const saved = preferences.data ? complete(preferences.data['booharu-runs']) : undefined
  return { runs: unsaved ?? saved, failed: !saved && preferences.isError, change }
}

const selectPresets = (preferences: BooharuPreferences) =>
  complete(preferences['booharu-runs']).presets

/** The run presets as the settings file holds them: read once, then shared with the settings
 * pages, which read the file again as they open, and refreshed by every save. A read that failed
 * has shown its error, and isn't tried again each time a view needing them opens. */
export function useRunPresets(): RunPreset[] | undefined {
  return useQuery({ ...booharuPreferencesQuery, select: selectPresets, retryOnMount: false }).data
}

/** Switches the Run menu's stages to `preset`'s, in the order they run, and starts nothing. The
 * click readies the finish chime, for the run it sets up. */
export function applyPreset(preset: RunPreset): void {
  prepareFinishSound()
  useKoharuStore.getState().setProcessingStages(inStageOrder(preset.stages))
}

/** Whether a run over pages chimes as it ends: the finish sound as the last read or save of the
 * Booharu settings left it, or as the file holds it while nothing has read them. A read that fails
 * shows its error as every failed command's does, and leaves the chime silent. */
export async function finishSoundOn(): Promise<boolean> {
  try {
    const preferences = await queryClient.fetchQuery(booharuPreferencesQuery)
    return complete(preferences['booharu-runs']).finish_sound
  } catch {
    return false
  }
}

/** What carrying on works on for the Run button's `target`: the project, or the pages chosen; none
 * for blocks or an area. */
function carryOnTarget(target: Target): RunTarget | null {
  if (target === 'project') return { kind: 'project' }
  return target.pages ? { kind: 'pages', pages: target.pages } : null
}

/** What the Run button does with `stages` on `target`, the project or the pages chosen. With every
 * stage on, it carries on with the work the pages still need, through `booharu_run`, or, with none
 * left, says so, offering Run all stages again, which is `run`. With fewer, it's `run`, Koharu's
 * own run of them. The click readies the finish chime. A refusal or an error shows as every failed
 * command's does. */
export async function runOrCarryOn(
  target: Target,
  stages: Stage[],
  run: () => Promise<unknown>,
): Promise<void> {
  prepareFinishSound()
  const carried = carryOnTarget(target)
  if (!carried || pipelineStages.some((stage) => !stages.includes(stage))) {
    await run()
    return
  }
  const plan = await call(commands.booharuRun, carried, stages)
  if (plan.run !== null) return
  const shown = toast.add({
    type: 'info',
    title: i18n.t('booharu.runs.alreadyDone'),
    actionProps: {
      children: i18n.t('booharu.runs.runAllAgain'),
      onClick: () => {
        toast.close(shown)
        prepareFinishSound()
        void run().catch(() => undefined)
      },
    },
  })
}
