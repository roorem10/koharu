import { vi } from 'vitest'

import { commands } from '@koharu/bridge/protocol'

export function mockWo13() {
  vi.spyOn(commands, 'getLibrarySettings').mockResolvedValue({
    export_name_pattern: '{index}_{page}',
    projects_folder: null,
  })
  vi.spyOn(commands, 'saveLibrarySettings').mockImplementation(async (settings) => settings)
  vi.spyOn(commands, 'exportSelectedPages').mockResolvedValue(null)
  vi.spyOn(commands, 'exportScript').mockResolvedValue(null)
  vi.spyOn(commands, 'previewScriptImport').mockResolvedValue(null)
  vi.spyOn(commands, 'applyScriptImport').mockResolvedValue(null)
  vi.spyOn(commands, 'getLibraryPaths').mockResolvedValue({
    projects_folder: 'C:\\Projects',
    default_projects_folder: 'C:\\Projects',
  })
  vi.spyOn(commands, 'chooseLibraryFolder').mockResolvedValue(null)
  vi.spyOn(commands, 'resetLibraryFolder').mockResolvedValue({
    export_name_pattern: '{index}_{page}',
    projects_folder: null,
  })
  vi.spyOn(commands, 'openLibraryFolder').mockResolvedValue(null)
  vi.spyOn(commands, 'renameProject').mockResolvedValue(null)
  vi.spyOn(commands, 'duplicateProject').mockResolvedValue('Project copy')
}
