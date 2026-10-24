# Koharu checklist — combined WO-00 build ready; owner acceptance pending

## Current handoff: October 24, 2026

- Revised patch 04 is now included. All five fixes are implemented alongside the existing alpha correction. The withdrawn nested-detection change is excluded. WO-01 and later features have not started.
- The actual Rust generator produced the two shortcut command bindings; the generated string maps are complete Record-style maps, rather than the manual Partial proposal. No hand edit of protocol.ts was shipped.
- All 89 distinct focused tests passed (82 frontend, 7 renderer), plus TypeScript, formatting, the clean private frontend build, and the optimized Windows x64 build. All 244 portable files were verified; all 10 PE files are complete x64; 240 companion files remain unchanged.
- Delivered Koharu-WO00-Executable-Update.zip and Koharu-Typesetting-Preview-2-WO00-Portable.zip. Executable SHA256: 876a3cdab826810a415933e5eb4da59680b69662d82a30b538837bbb6c8175af. Full portable SHA256: 745d676069ebc2057403adfd5aa196748e64cbcf32602f207c75a3b4646748ed. New source: Koharu-Preview2-WO00-Source.zip. The five feature patches and combined integrated diff target the previously delivered Alpha-Fix source and reproduce all 14 changed code/test/generated files.
- Owner Windows acceptance remains pending: outline quality on the exact CCWildWords page, startup/favorites, color picker, focused tool keys, shortcut restart persistence, tiny cells, and five-page or larger PNG/CBZ/PSD export. Browser screenshot capture was blocked by the host socket restriction; UI screenshots remain required before upstream submission. No Windows GUI or memory benchmark is claimed.
- Keep the original Koharu installation isolated. Updating only koharu.exe in D:\KoharuStartupTest\Koharu-Typesetting-Preview-2-Startup-Fix keeps the same PreviewData path. Back up the current executable first. Prior downloads remain frozen. See WO-00-Windows-Fixes-Integration-Report.md and Koharu-WO00-Test-Instructions.md.
- Linux build caches and its CEF runtime were removed after the real binding generator ran to recover disk space. The Windows build cache and stable Rust 1.99.1 toolchain remain. The unused Rust 1.97.1 fallback was removed. Do not assume the earlier native target is warm.

## Earlier checkpoint history

## Earlier integration checkpoint: revised patch 04

- The owner supplied 04-windows-bug-fixes-revised.patch. It applied cleanly to the alpha-corrected source. The five fixes are integrated: pending color drags/cancel semantics, focused control shortcuts, ordered shortcut persistence with stale-load protection, tiny-box insets, and four-page export batches. The withdrawn nested detection change is excluded.
- The manually proposed protocol.ts hunk was excluded. The actual koharu-app generate binary was built and run on Linux; the regenerated protocol adds exactly getShortcuts/saveShortcuts, with string maps matching the real Rust signatures. No project save-format or dependency change was introduced.
- 89 focused tests passed (82 frontend tests across four files and 7 text-renderer tests). TypeScript and the clean private frontend build passed. Rust formatting passed. The new ColorWell test helper needed a TypeScript annotation correction, after which its 7 tests passed again; do not count that rerun twice.
- The Windows generator/application code compiled successfully. The final combined Windows application build is in progress. The previously delivered Alpha-Fix executable is frozen and still usable; do not call this checkpoint a new tested portable yet.
- A browser UI screenshot attempt was blocked by the host's AF_UNIX socket restriction. DOM interaction tests passed; native Windows GUI/visual acceptance and screenshot capture remain pending.
- Build recovery: GTK 3/4 and the Linux CEF runtime were restored for the real generator. Disk pressure was resolved by compiling its last step on /dev/shm; the completed Linux build cache/runtime was then removed. Do not attempt to reuse the deleted alpha-native-target cache. The private stable Rust 1.99.1 toolchain and Windows target remain. Native protocol output is complete and preserved in this source checkpoint.
- The alpha compositing correction, native straight-RGBA readback, filled outlines, empty-path guard, Typography revision-test correction, startup fix, favorites, and executable-relative PreviewData isolation are retained. WO-01 and later features have not started.

## Prior checkpoint history

## Current handoff: October 24, 2026

- Confirmed the vector straight-alpha versus premultiplied-blending mismatch identified in Claude's 08 patch. A real raster-background regression fails with 0 partially blended white edge pixels before the fix and passes afterward. Earlier comparisons put blue inside the Vello batch and missed this boundary.
- Applied 08, added native straight-RGBA readback conversion and a regression for transparent vector/raster colors, retained 07's filled outlines, guarded empty border paths, and corrected the existing Typography revision test to 2. There is no additional project save-format revision.
- All 13 focused checks passed: 6 compositor, 6 text-renderer, and 1 scene schema check. Formatting, rebuilt/optimized canvas WASM, private frontend/TypeScript, and the optimized Windows build passed. The executable embeds the rebuilt canvas asset. No native Windows GUI test was run here.
- Delivered Koharu-Alpha-Fix-Executable-Update.zip (47458901 bytes) and Koharu-Typesetting-Preview-2-Alpha-Fix-Portable.zip (235914487 bytes). Executable SHA256: ec10bdbce08039b3077e90e08f156465411123c70e9eb9559fff9cdc10a57816. Full portable SHA256: 081481c07c4052a4dbc43e532dcfd57d695a2b8b0dd8a436683e06c2ab1e4f71. All 244 portable entries were verified; 240 companion files remain unchanged.
- The full source is Koharu-Preview2-Alpha-Fix-Source.zip. The combined WO-00 feature diff targets the working Startup-Fix source; the separate schema-test correction is patch 10. An alternative incremental path is existing 07, then original 08, then 09, then 10. Both patch paths reproduce the tested code. See WO-00-Alpha-Fix-Report.md.
- Owner's earlier filled-outline-only export still had rough white outer edges. The exact CCWildWords Windows case is now pending a retest of this alpha-corrected executable. Do not mark Affinity parity, RTX performance, or Windows visual acceptance as passed.
- Revised Windows patch 04 is **not included**: the named revised file was absent; only the old 02 patch was found. Do not substitute it or mark those Windows fixes complete. This prevents claiming the tracker's full P2 + 04 baseline. WO-01 and later features have not begun.
- Keep the original Koharu installation isolated. The existing preview folder remains D:\KoharuStartupTest\Koharu-Typesetting-Preview-2-Startup-Fix. The executable-only update preserves that PreviewData path and favorites; backup the existing executable first. All prior downloads remain frozen.

## Earlier checkpoint history

Updated October 24, 2026. Baseline: the supplied Koharu 0.83.5 source.

## Current authorized work

On October 24, 2026, the user explicitly authorized the changes needed to fix contour outline smoothness. The filled-region implementation and six focused native tests are complete. The optimized Windows build and portable verification passed. The user's visual acceptance remains pending. Other editing additions remain on hold.

The user postponed the Preview 1 trial after Claude's static review and authorized fixes plus complete separation from official Koharu data. Work is limited to these corrections. Perspective, rich text and Mesh Warp remain on hold. Preview 1 downloads are preserved for reference and should not be used for the isolated trial.

- [x] Trace official settings, project, runtime/model, browser, log, window-state and credential locations.
- [x] Implement separate executable-relative PreviewData paths and credential namespace; no legacy migration or writable-path fallback.
- [x] Merge glyph boundaries and fill cleaned offset regions for Contour outlines.
- [x] Paint opaque Outside outlines beneath the glyph fill; use true outside-only filled regions for clear/translucent text.
- [x] Select Contour explicitly for newly created and newly detected text, preserving existing choices.
- [x] Verify isolation and failure behavior: two focused tests passed, including copied executables and persistence across new processes; Windows x64 release compiled.
- [x] Verify union/counter geometry and antialiased edges, transparent/translucent fill pixels; prepared packet round-trip passed.
- [x] Build and verify isolated Preview 2 downloads: exact 242-file installer directory, all content hashes, portable entries, PE sections/imports, and 237 CEF archive members.
- [x] Update reproduction/installation notes, preserve all feature requirements, save source and exact patches, then return to hold.

The original Preview 2 delivery is preserved. After its Windows trial reached the tauri.localhost connection-error page, the reviewed explicit child-profile correction was compiled into a separate ready-to-run portable ZIP. The user has now confirmed that the corrected portable reached the Projects page on Windows; restart and font-favorite persistence have also passed the user's trial; outline, project-state, export, and storage-location acceptance remain pending. The remaining editing features stay on hold. See the startup correction record below and PREVIEW2_IMPLEMENTATION.md for the earlier delivery.

## Startup correction — October 24, 2026; portable trial ready

- Inspected Claude's 05-preview2-blank-window-fix.patch, its diagnosis, the supplied cef.log, and the exact pinned Tauri/CEF source.
- Found that Tauri supplies app_local_data_dir when data_directory is omitted on Windows/Linux. Dropping the override would normally select a hashed child profile, rather than the default profile described in the diagnosis.
- Prepared 06-preview2-explicit-browser-profile.patch as an alternative to 05, applied directly to unpatched Preview 2. The reviewed code retains the browser root and explicitly uses PreviewData/browser/editor for the window profile.
- The candidate changes one Rust source file. Settings, projects, packages, logs, credentials, the private identifier and updater exclusion retain the Preview 2 separation.
- The supplied four-line CEF log does not prove that profile selection caused the error. CEF permits root-equality, so the precise internal mechanism remains a hypothesis. The corrected candidate subsequently passed its first user-observed Windows launch.
- Passed patch applicability/round-trip checks, Rust formatting, and a Windows x64 debug compiler check of koharu-app with private preview settings. This is not native linking or GUI acceptance.
- Saved Koharu-Preview2-Startup-Fix-Review.md, the reviewed patch, and Koharu-Preview2-Startup-Fix-Source.zip. The ongoing feature checkpoint contains the same source candidate and this updated progress record.
- Built the private optimized Windows x64 executable with the production frontend protocol and existing static CRT settings. Reused the 48-file private frontend after confirming its source matches the frozen Preview 2 baseline. The build passed; the linker reported only unavailable SDK PDB debug metadata.
- Packaged Koharu-Typesetting-Preview-2-Startup-Fix-Portable.zip with 243 files. Every ZIP entry passed size/SHA256 comparison. Its 240 unchanged baseline files match the original audited portable; the executable and instructions were replaced and START-HERE.txt was added. All 10 PE binaries have complete x64 sections. No preview user data was bundled.
- Portable ZIP: 235458789 bytes; SHA256 6007e666268bcacb68241366e7f361f30047b9ed886657c9df228c83fa6eba35.
- Compiled executable SHA256: b3d937d090baab29faae6fd9233880521ef24844da68da000201ebdb6a8d9c25.
- Included beginner instructions and saved Koharu-Startup-Fix-Test-Instructions.md. This trial opens directly from its extracted folder; no developer tools or installation are required.
- Initial Windows launch passed: the user reached the Projects page. The user subsequently confirmed that the application relaunches and font favorites remain saved at the top of the font selection list. Pending: verify browser-profile/storage locations, outline behavior, project-state persistence/export, and that the original installation's data remains untouched. The earlier Preview 2 setup/portable files remain unchanged; use the separately named startup-fix ZIP for this trial.
- No Perspective Transform, rich text, Mesh Warp or unrelated Windows patch was implemented in this investigation.

## Windows trial — first startup observed

On October 24, 2026, the user launched the startup-fix portable from D:\KoharuStartupTest\Koharu-Typesetting-Preview-2-Startup-Fix. The first supplied screenshot showed the application's Preparing Koharu / Initializing local runtimes screen. The next screenshot showed the Projects page with an empty project list, and the user explicitly confirmed reaching the main page and continuing testing.

Evidence: Screenshot 2026-10-24 044628.png, Library ID libfile_dec88192b994819188f001534fa9dfb7. This confirms the first launch completed through runtime initialization and frontend startup; restart and font-favorite persistence were verified in the next user test. Correct outline rendering/export, project-state persistence, and the on-disk data location still need acceptance. The screenshot still displays the generic Documents-folder caption, so that wording must not be treated as runtime evidence of where this preview stores projects.

## Windows trial — font favorites and restart passed

On October 24, 2026, the user explicitly confirmed that favorite fonts work, then closed the program and relaunched it. The favorites remained saved and appeared at the top of the font selection list. This passes application relaunch, favorite-font persistence across a real restart, and Favorites placement. The user is continuing with the remaining tests. No claim is made yet about outline appearance, project save/reopen, Undo/Redo, export matching, or storage-location inspection.

## Windows trial — outline smoothness remains below the reference editor

On October 24, 2026, the user reported that the fork's outline is better than default Koharu but retains visible slight blockiness, particularly with larger text. Confirmed settings: Contour Stroke, Outside, Round. Smaller text/resolutions are acceptable to the user, but the implementation has not met their desired dedicated-editor quality. Do not mark outline visual acceptance as passed.

Comparison evidence supplied by the user: AffinityPhoto2.png (Library ID libfile_e2ac0efc327c8191a497f057ef537fff) and Koharu.png (Library ID libfile_6c58c053156081918071c8cbff8eeeac). Both attached PNGs are 356 by 356 pixels and have partially transparent edge pixels. They are visual comparison examples, not a controlled identical-geometry renderer benchmark. Respect the user's observation even where the image difference is subtle.

The user requested an explanation first, rather than implementing another change. Code inspection established:

- The original Expansion method uses vector glyph emboldening/path expansion, not merely bitmap-mask enlargement. Its kurbo 0.13.1 expand_path dependency documents limitations when a large expansion meets tight curvature and certain winding configurations.
- Contour Stroke constructs an explicit outline around merged glyph boundaries. The union currently flattens curves at 0.02 layout pixels, before later transforms; this is very fine at ordinary scale and is not established as the cause of the reported whole-pixel appearance.
- The same Vello renderer ultimately rasterizes the result with area antialiasing. Its stroke/arc approximation also has finite tolerances. Round controls join geometry, not antialiasing quality.
- PNG export uses RasterOptions::default(), whose supersampling factor is 1. Supersampling up to 4 exists in the rasterizer but is not selected by this export path. Good single-resolution area antialiasing can still be excellent; the absence of supersampling is a candidate quality lever, not proof of the dominant cause.
- Affinity's official documentation describes antialiasing coverage maps and blend gamma, and Adobe documents multiple text-antialiasing styles. Their exact proprietary stroke algorithms and internal sample counts are not known; do not invent a claim that they always use a particular supersampling factor.

No rendering code was changed for this explanatory response. Isolating the dominant cause would require a controlled same-font, same-transform export comparison rather than assigning all remaining roughness to curve flattening.

## Filled contour outline correction — October 24, 2026

- Replaced Contour text preparation's GPU stroke command with an ordinary nonzero fill of a cleaned polygon offset region, using the existing pinned i_overlay 9.1.0 dependency.
- Retained merged glyph regions as rings. Outside/Center/Inside use positive and negative offsets and boolean differences. Opaque Outside retains underlap beneath the letters to avoid antialias seams; clear/translucent Outside uses an outside-only region.
- Preserved Round/Sharp/Bevel. Round arcs use a roughly 0.01 layout-pixel chord target within the library's angular limits; Sharp translates the existing miter ratio of 4 into the library's minimum-angle representation.
- Bounds come from the finished border. Preparation remains in retained native text nodes; canvas/PNG/layer crops/PSD use the existing shared FillPath command. No frontend controls, IPC schema, prepared format, dependencies, or canvas WASM source changed. The Expansion option remains selectable.
- All six focused text renderer tests passed, including 0/96/255 text alpha, fractional placement, holes, merged overlap boundaries, all placements and joins, large inside widths, an outer-edge antialiasing guard at sizes 64 and 300, and actual prepared-packet round-trip.
- Rust formatting passed. The private frontend compiled and TypeScript passed; its 48 exported files were generated from the existing source and existing canvas WASM.
- A controlled DejaVu Sans comparison used size 300, width 24.5, Outside/Round, and the real vector compositor. Both the old stroke and new filled region were already antialiased on this Linux software adapter. This fixture does not reproduce the supplied CCWildWords/Windows failure and does not establish Affinity parity. The new approach preserved partial coverage and had similar warm rendering time in this limited debug test. 4x supersampling was substantially more expensive here. Do not project those timings onto the user's RTX 5080.
- The exact CCWildWords font/project and the original 3840x2160 image bytes were not available locally. The supplied image copies earlier in the chat were 2048x1152. Claude's exact 3.2%/91% measurements and proposed root cause remain unverified by this test.
- The private optimized Windows x64 build passed with static CRT and the production frontend protocol. The original runtime files were retained in the new portable. All 243 file entries passed size/SHA256 verification; all 10 PE files are complete x64, and the new executable has no separate MSVC runtime imports. No Windows GUI launch was performed here.
- Delivered Koharu-Typesetting-Preview-2-Outline-Fix-Portable.zip, 235369432 bytes, SHA256 a52d238456861e94927e933acb43827b5fe366a3201660aa3c3c818272053cbe. Executable SHA256 2c2e71330f9ac79f3f0d2deb13f47206bf9bec6dbe3a26c6f02cd2e56b38842e. The beginner guide supports updating only the executable in the existing preview folder to keep the same PreviewData path, with an executable backup for rollback.
- A rounded-corner fixture at radius 24.5 showed mean absolute coverage error decrease from 0.119453 to 0.004831 against a densely sampled continuous-shape reference. This is synthetic geometry evidence, not acceptance of the exact CCWildWords Windows case.
- The prior successful startup correction (explicit PreviewData/browser/editor profile), private namespace, credential separation, updater exclusion, and font favorites remain in the source unchanged. Original published preview downloads are preserved.

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

- [x] Successfully open the corrected private preview on Windows 11. User confirmation and the Projects-page screenshot establish the first successful launch.
- [x] Confirm favorites persist across a real restart. User closed/relaunched the program and verified the saved favorites at the top of the font selection list.
- [ ] Test the previously problematic font and all outline positions/joins. Partial trial: Contour Stroke / Outside / Round improves on default Koharu but still has user-reported blockiness; visual quality is not accepted yet.
- [ ] Save/reopen a trial project; verify settings and Undo/Redo.
- [ ] Compare the final desktop canvas, PNG and exported layer pixels.

The Linux build environment did not perform a Windows GUI launch. The user has now confirmed the initial launch, application relaunch, and favorite-font persistence on their PC; the unchecked items remain part of their ongoing trial.

## Remaining additions — not started

- [ ] **3. Perspective Transform:** independent corners, true perspective, guides/live preview, saved editable transforms, reset/apply/cancel and undo/redo.
- [ ] **4. Rich text:** selected-range font, weight, italic, size and color; continuous layout; saved runs and undo/redo.
- [ ] **5. Mesh Warp:** curved grid/handles, added divisions, multiple-point selection, smooth/sharp nodes, live preview and saved editable warps.

Do not start these additions until the user explicitly resumes the project.

## Preview 2 verification

- Both focused private-profile tests passed, including three copied-executable child processes and persistence across reopening.
- Nonzero contour-union/counter geometry, antialiased-seam and clear/translucent-fill pixel tests, and real prepared packet round-trip passed (3 renderer tests).
- Rust formatting and diff whitespace checks passed. No frontend, IPC schema or prepared-command format changed in Preview 2.
- Final private Windows x64 optimized build passed; only missing SDK PDB debug-metadata warnings remain.
- All 10 runtime PE files are x64, have complete section data, and do not import a separate MSVC runtime.
- The clean installer directory lists exactly the expected 242 runtime files plus standard NSIS internals. Every runtime file matches staging. Two incomplete local extraction outputs were checked against the complete archive member streams; their size/SHA256 matched.
- The portable ZIP's 242 files match staging by size and SHA256.
- All 237 CEF runtime/notice members match the official minimal archive. Its SHA-1 matches the official CEF index.
- A temporary staging copy inflated the first installer candidate. It was not delivered. The final installer excludes dot-files, and the Windows build script rejects temporary staging files and preserves user-data staging folders.

Setup: 176,398,091 bytes. SHA256: 484090392d1272375996c29fd68aee5a56a8964e82a6b39b5e4de0fc0026f47f.

Portable: 235,452,654 bytes. SHA256: b86ccf58052b197fa23543e7d5fa6ac6029438302e012211bc61a215dd6a50ac.

## Preview 1 verification record

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

The original upload is unchanged. The frozen source and ongoing checkpoint include IMPLEMENTATION_HANDOFF.md and the exact patch. Build caches, credentials, model weights, transient TypeScript metadata and internal review screenshots are excluded. Saved original compiled downloads can be recovered without rebuilding the temporary toolchain. The reviewed startup-fix portable has passed initial Windows startup, relaunch, and favorite-font persistence; further acceptance is in progress and the remaining editing features stay on hold.
