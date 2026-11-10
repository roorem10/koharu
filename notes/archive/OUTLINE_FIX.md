# WO-00: revised Windows patch 04 integration

Status: all five revised fixes are integrated with the existing outline alpha correction. The real Rust binding generator ran, 89 focused tests and TypeScript passed, the optimized Windows x64 application built, and the update/portable packages passed integrity checks. Owner Windows acceptance remains pending. WO-01 has not started.

## Resulting behavior

| Fix | Previous problem | Result |
| --- | --- | --- |
| Color picker, #866 | A lost drag-end event could leave the displayed color uncommitted | Closing applies an unfinished drag; Escape discards it. Transparent, hex input, sampling, and external values clear the older pending choice. |
| Focused controls, #1181 | A slider retaining focus disabled tool shortcuts | Bare tool keys switch tools from non-text inputs. Control keys stay with the input; text entry, handled events, and IME composition retain their keys. Existing Alt handling is preserved. |
| Saved shortcuts, #1136 | Key changes were lost on restart | Native preferences store the shortcut map. Browser writes are ordered and copied per edit; a late startup response cannot overwrite a newer local edit. |
| Tiny text areas, #1127 | An inset could consume the whole cell and abort export | Consumed axes retain half their extent. Other axes keep their previous inset. A genuinely zero-size cell remains invalid. |
| Multipage export, #1287 | Every frame and encoded page was retained before writing | Rendering, encoding, and writing run in four-page batches in project order. Names and archive order remain the same. |

The nested-detection change was withdrawn in the revised patch and is excluded. No changes were made to detection code. The Alpha-Fix blending correction, native straight-RGBA readback, filled Contour outlines, empty-path guard, Typography schema-test correction, font favorites, startup fix, and preview isolation are retained.

Shortcut preferences use a new section of the existing native configuration. There is no new project save-format revision or dependency. The private executable remains separate from the original installation and uses the same executable-relative PreviewData when updated in place.

## Integration and generated bindings

04-windows-bug-fixes-revised.patch applied cleanly to the alpha-corrected source. Its manually proposed protocol.ts hunk was excluded. The existing koharu-app generate program was compiled and executed instead. Its output changes exactly two command entries, getShortcuts and saveShortcuts, using the actual Rust signatures.

The generator emits string maps as `{ [key in string]: string }`. This differs from the manual Partial map in the supplied patch. The frontend and test types were checked against the generated result. The new test helper's mock callback annotation was corrected after TypeScript identified it; the color tests and final frontend build passed afterward.

No git commit or external submission was made, so the supplied patch's author/co-author footer was not added to commit history. The review outputs are pure git diffs, separated by feature.

## Verification

- **82 frontend tests:** 23 picker/workspace tests and 59 runtime/editor-component tests. New regressions cover an unfinished black drag, Escape cancellation, later Transparent/hex choices, slider key ownership, handled/composing keys, ordered shortcut snapshots, stale startup loads, and recovery after a failed save.
- **7 text-renderer tests:** the new inset regression plus the six outline/layout/prepared-frame checks. Insets are tested at width 7, exact width 8, height 5, and zero width.
- Total: **89 distinct focused tests**. The seven color tests were rerun after the test helper's TypeScript correction and are not counted twice.
- Rust formatting passed. The private frontend and TypeScript build passed. Its clean static export has 48 files and one canvas WASM asset.
- The real native binding generator executed successfully. Linux GTK/CEF build dependencies were restored for this step; a memory-backed build directory resolved temporary disk pressure. Temporary Linux build caches were removed after generation. These were build-environment changes, not application source or dependency changes.
- Windows application/generator code compiled, and the final private Windows x64 optimized application built with the production frontend protocol and static CRT.
- Two build attempts stopped on empty generated object files in koharu-translator and rars. Retrying those stages completed the optimized build; no application source workaround was added.
- All **244** full-portable entries passed decompressed size/SHA256 verification. All 10 executable/DLL files have complete x64 PE sections. The executable-update ZIP contains the exact same new executable. All **240** companion files are unchanged from the working Startup-Fix portable. No PreviewData is bundled, and no separate MSVC runtime imports were introduced.
- The executable contains the new native shortcut commands, the corrected native alpha shader, and the rebuilt canvas asset. The WASM bytes are unchanged from the tested Alpha-Fix renderer and match SHA256 ef3f79e63d2820f5ed233effe713a6c5436eeff90b4b4466107ad3d27500e4f6.
- All five separated patches apply to the Alpha-Fix baseline and reproduce all 14 changed code/generated/test files byte for byte.

No actual Windows GUI launch, original CCWildWords visual comparison, shortcut-relaunch test on the owner's PC, large-project memory benchmark, or real multipage export was performed here. The export loop was compiled and reviewed; owner checks remain in the test guide. Four pages bounds the active batch count, not total process memory or bytes per page.

Repository AGENTS.md requires: “A UI change needs a screenshot of the affected state.” A real browser screenshot attempt was blocked before a page opened because the host denies AF_UNIX socket creation. DOM interaction tests passed; the UI screenshot requirement remains pending for the owner's Windows test. No screenshot or native desktop acceptance is claimed.

## Export failure behavior

PNG and PSD pages are now written progressively. If a later batch fails, earlier files can remain in the destination folder. CBZ still uses a temporary file and publishes the completed archive only after success. This behavior is documented in the test instructions.

## Reviewable patches

Baseline: **Koharu-Preview2-Alpha-Fix-Source.zip**, the previously delivered alpha-corrected source. The full new source already has the changes applied.

- 04a-color-picker-drag.patch
- 04b-focused-tool-shortcuts.patch
- 04c-persist-keyboard-shortcuts.patch
- 04d-tiny-text-box-insets.patch
- 04e-bounded-page-export.patch

Each feature patch includes its relevant tests where applicable, and none mixes in preview branding or isolation plumbing. Alternatively, apply **04-windows-bug-fixes-integrated.patch**, which contains all five feature diffs. Use one route; do not apply both routes to the same source. Koharu-WO00-Review-Patches.zip bundles them for review. No accepted commit hash exists yet; the owner and Claude decide acceptance before the next baseline is recorded.

## Build identities

| Artifact | SHA256 |
| --- | --- |
| koharu.exe | 876a3cdab826810a415933e5eb4da59680b69662d82a30b538837bbb6c8175af |
| Koharu-WO00-Executable-Update.zip | f781ac8109d6f732e43674aac6559bcac309755cdd9a269395b6b7d1edbcc262 |
| Koharu-Typesetting-Preview-2-WO00-Portable.zip | 745d676069ebc2057403adfd5aa196748e64cbcf32602f207c75a3b4646748ed |

Executable update: 46947056 bytes. Full portable: 235400670 bytes.

## Owner handoff

Follow Koharu-WO00-Test-Instructions.md. Back up the current preview's executable and replace koharu.exe in the same working folder. Keep PreviewData in place. The unchanged companion runtime files support this executable-only update.

Retest the original outline page at full size, black color selection, tool keys after a slider, shortcut persistence after a full restart, a previously failing tiny text area, and at least a five-page PNG/CBZ export. Include PSD if used. Zip the original PNG and capture the picker/shortcut settings screenshots for review. Favorites and startup remain in the acceptance checklist. The original Koharu installation stays separate.
