'use client'

import { commands } from '@koharu/bridge/protocol'
import type {
  PipelineConfig,
  Preferences,
  ProviderPreferences,
  TypesettingConfig,
  WheelZoom,
  TextStyles,
} from '@koharu/bridge/protocol'

import {
  receiveError,
  receivePreferences,
  receiveShortcuts,
  receiveTranslationModels,
  type Shortcuts,
  useKoharuStore,
} from './store'

type Command<Args extends unknown[], Result> = (...args: Args) => Promise<Result>

let translationModelsRequest: Promise<void> | null = null
let translationModelsGeneration = 0
let preferencesWriteGeneration = 0
let preferencesWriteQueue: Promise<void> = Promise.resolve()
let shortcutsWriteGeneration = 0
let shortcutsWriteQueue: Promise<void> = Promise.resolve()
let textStylesGeneration = 0
let textStylesQueue: Promise<void> = Promise.resolve()
let textStylesRequest: Promise<void> | null = null
let wheelZoomWriteGeneration = 0
let wheelZoomWriteQueue: Promise<void> = Promise.resolve()

export async function call<Args extends unknown[], Result>(
  command: Command<Args, Result>,
  ...args: Args
): Promise<Result> {
  try {
    return await command(...args)
  } catch (error) {
    throw report(error)
  }
}

export function dispatch<Args extends unknown[], Result>(
  command: Command<Args, Result>,
  ...args: Args
): void {
  void call(command, ...args).catch(() => undefined)
}

export async function refreshPreferences(): Promise<void> {
  const generation = preferencesWriteGeneration
  await preferencesWriteQueue
  const preferences = await call(commands.getPreferences)
  if (generation === preferencesWriteGeneration) receivePreferences(preferences)
}

export function savePreferences(
  pipeline: PipelineConfig,
  providers: ProviderPreferences,
  typesetting: TypesettingConfig,
): Promise<Preferences> {
  preferencesWriteGeneration += 1
  const pending = preferencesWriteQueue
    .catch(() => undefined)
    .then(() => call(commands.savePreferences, pipeline, providers, typesetting))
  preferencesWriteQueue = pending.then(
    () => undefined,
    () => undefined,
  )
  return pending
}

/**
 * Load saved shortcuts unless the user changed one since loading began; a newer local edit
 * always wins over the startup response.
 */
export async function refreshShortcuts(): Promise<void> {
  const generation = shortcutsWriteGeneration
  await shortcutsWriteQueue
  const saved = await call(commands.getShortcuts)
  if (generation === shortcutsWriteGeneration) receiveShortcuts(saved)
}

/**
 * Save the complete shortcut map. Writes run one at a time in edit order, each with its own
 * copy of the map, so an older save can never land after a newer one. A failed write is
 * reported by `call` and does not block later saves.
 */
export function saveShortcuts(shortcuts: Shortcuts): Promise<void> {
  shortcutsWriteGeneration += 1
  const snapshot = { ...shortcuts }
  const pending = shortcutsWriteQueue
    .catch(() => undefined)
    .then(() => call(commands.saveShortcuts, snapshot))
    .then(() => undefined)
  shortcutsWriteQueue = pending.then(
    () => undefined,
    () => undefined,
  )
  return pending
}

export async function refreshWheelZoom(): Promise<void> {
  const generation = wheelZoomWriteGeneration
  await wheelZoomWriteQueue
  const saved = await call(commands.getWheelZoom)
  if (generation === wheelZoomWriteGeneration) useKoharuStore.getState().setWheelZoom(saved)
}

export function saveWheelZoom(wheelZoom: WheelZoom): Promise<void> {
  wheelZoomWriteGeneration += 1
  const pending = wheelZoomWriteQueue
    .catch(() => undefined)
    .then(() => call(commands.saveWheelZoom, wheelZoom))
    .then(() => undefined)
  wheelZoomWriteQueue = pending.then(
    () => undefined,
    () => undefined,
  )
  return pending
}

export function refreshTranslationModels(force = false): Promise<void> {
  if (!force && translationModelsRequest) return translationModelsRequest

  const generation = ++translationModelsGeneration
  const request = call(commands.getTranslationModels)
    .then((models) => {
      if (generation === translationModelsGeneration) receiveTranslationModels(models)
    })
    .finally(() => {
      if (translationModelsRequest === request) translationModelsRequest = null
    })
  translationModelsRequest = request
  return request
}

function report(error: unknown): Error {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === 'string'
        ? error
        : 'The native application returned an unknown error.'
  receiveError(message)
  return error instanceof Error ? error : new Error(message)
}

export function refreshTextStyles(): Promise<void> {
  if (textStylesRequest) return textStylesRequest
  const generation = textStylesGeneration
  const request = textStylesQueue
    .then(() => call(commands.getTextStyles))
    .then((textStyles) => {
      if (generation === textStylesGeneration) useKoharuStore.setState({ textStyles })
    })
    .finally(() => {
      if (textStylesRequest === request) textStylesRequest = null
    })
  textStylesRequest = request
  return request
}

export function saveTextStyles(styles: TextStyles): Promise<void> {
  const generation = ++textStylesGeneration
  const snapshot = structuredClone(styles)
  useKoharuStore.setState({ textStyles: snapshot })
  const pending = textStylesQueue
    .catch(() => undefined)
    .then(() => call(commands.saveTextStyles, snapshot))
    .then((textStyles) => {
      if (generation === textStylesGeneration) useKoharuStore.setState({ textStyles })
    })
  textStylesQueue = pending.catch(() => undefined)
  return pending.catch(async (error) => {
    if (generation === textStylesGeneration) {
      await textStylesQueue
      await call(commands.getTextStyles)
        .then((textStyles) => {
          if (generation === textStylesGeneration) useKoharuStore.setState({ textStyles })
        })
        .catch(() => undefined)
    }
    throw error
  })
}
