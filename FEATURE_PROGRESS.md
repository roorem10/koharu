# Koharu checklist — on temporary hold

Updated October 24, 2026. Baseline: the supplied Koharu 0.83.5 source.

## Completed first delivery

- [x] **1. Favorite/starred fonts:** persistent native preferences; Favorites above All Fonts; independent star actions; search, filtering, previews and keyboard focus retained.
- [x] **2. Contour-based outlines:** actual glyph curves; Expansion/Contour methods; Outside/Center/Inside placement; Round/Sharp/Bevel joins; width/color controls; editable text and saved typography.
- [x] Update scene, project, Agent, detection and fixture consumers; regenerate the typed IPC bridge from Rust.
- [x] Localize the new controls in all 12 existing locales.
- [x] Complete focused rendering/interaction checks and normal/compact component screenshot review, including long labels.
- [x] Build the final canvas WASM and private production frontend.
- [x] Rebuild the Windows x64 optimized release with the production frontend embedded and preview error reporting disabled.
- [x] Inspect all 10 runtime PE binaries as x64; find no separate Visual C++ runtime imports.
- [x] Recover two incomplete local DLL extractions from the intact, verified CEF archive. All 237 CEF runtime/notice files match archive members by size and SHA256.
- [x] Save the portable ZIP: all 242 files match staging by size and SHA256; temporary files are excluded.
- [x] Build the Windows setup with NSIS warnings treated as errors; no NSIS warnings remain.
- [x] Verify all 242 staged files against installer contents. Incomplete local extraction outputs were streamed again from the installer, matched to their full sizes/SHA256, and restored before the final comparison passed.
- [x] Write the detailed implementation handoff, installation/build guides and recovery notes.
- [x] Generate the exact text patch against the original supplied archive, check/apply it to a fresh baseline, and confirm all changed files match the modified source.
- [x] Save compiled downloads, source checkpoints and documentation; freeze a separate Preview 1 source archive with the patch.
- [x] Put the project on temporary hold as requested.

## User acceptance pending

- [ ] Install/open on the user's Windows 11 computer.
- [ ] Confirm favorites persist across a real restart.
- [ ] Test the previously problematic font and all outline positions/joins.
- [ ] Save/reopen a trial project; verify settings and Undo/Redo.
- [ ] Compare the final desktop canvas, PNG and exported layer pixels.

A Windows GUI launch was not performed in this Linux build environment. These checks are the purpose of the user's trial.

## Remaining additions — not started

- [ ] **3. Perspective Transform:** independent corners, true perspective, guides/live preview, saved editable transforms, reset/apply/cancel and undo/redo.
- [ ] **4. Rich text:** selected-range font, weight, italic, size and color; continuous layout; saved runs and undo/redo.
- [ ] **5. Mesh Warp:** curved grid/handles, added divisions, multiple-point selection, smooth/sharp nodes, live preview and saved editable warps.

Do not start these additions until the user explicitly resumes the project.

## Verification record

- Native Rust app/generator compiled; the actual generator regenerated protocol.ts.
- Renderer cargo check and Rust formatting passed.
- Focused compositor pixel test passed for alignment, transparent interiors and artwork preservation.
- Real contour-glyph prepared-packet round-trip passed.
- Font picker/localization checks: 7 passed; updated font picker checks after focus hardening: 2 passed.
- TypeScript and focused frontend lint passed.
- Actual FontPicker/Inspector components reviewed at 1100×780 and 900×600 with a mocked native backend; this was not Windows desktop end-to-end testing.
- WASM build passed; unavailable optional wasm-opt post-optimization was skipped. Private production frontend builds passed, including after restoration.
- Resumed Windows release passed. Missing Microsoft SDK PDB warnings affect debugger metadata, not runtime completeness.
- Setup: 176,398,823 bytes. SHA256: c89c4b5b414a761eabc43cb180a9bd2966d0861629c430c32d866c9d71986ea5.

## Limits and preservation

Text remains editable in Koharu. PSD cached pixels share the prepared rendering path; this milestone does not add a separately editable Photoshop stroke effect. Keep trial projects separate from files opened with older Koharu versions.

The original upload is unchanged. The frozen source and ongoing checkpoint include IMPLEMENTATION_HANDOFF.md and the exact patch. Build caches, credentials, model weights, transient TypeScript metadata and internal review screenshots are excluded. Saved compiled downloads can be recovered without rebuilding the temporary toolchain. The project is on hold.
