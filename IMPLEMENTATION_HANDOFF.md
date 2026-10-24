# Koharu Typesetting Preview 1: implementation and reproduction guide

Updated October 24, 2026. Baseline: the user's original `koharu-main.zip`, Koharu 0.83.5.

## Scope and hold status

This milestone implements **favorite fonts** and **contour-based text outlines**. The user requested a temporary hold after this delivery. **Do not begin Perspective Transform, rich text, or Mesh Warp unless the user explicitly resumes that work.** Those three additions remain unimplemented.

This document explains the implementation sufficiently for a developer or another AI to recreate it. The complete modified source is the definitive reference. `FEATURE_PROGRESS.md` records verification and outstanding user acceptance; `PREVIEW_INSTALL.md` explains how to try the build; `BUILD_PREVIEW.md` explains the Windows rebuild. All paths below are relative to the extracted source root.

The frozen source archive also contains `Koharu-Typesetting-Preview-1.patch` beside its `koharu-main` folder. That patch is the exact text-file difference against the original supplied archive, including tests, translations, preview packaging and this documentation. Generated WASM and temporary build files are excluded from the patch. Rebuild WASM from the Rust source.

## 1. Architecture that these changes preserve

The React/Next.js interface invokes typed Tauri commands. Rust owns application preferences, scene state, project changes and preparation for rendering. Typography belongs to `koharu-scene`; `koharu-renderer` shapes text and prepares portable drawing commands; `koharu-rasterizer` compiles those commands into Vello scenes. The browser presents the prepared frame through the `koharu-canvas` WASM module and WebGPU.

One prepared frame must feed the canvas, PNG output, layer crops and PSD cached pixels. Implementing an outline only with CSS, a browser canvas effect, or an export-only filter would break that invariant. The new stroke is therefore a portable prepared command understood by both native rendering and WASM.

Favorites are application preferences rather than project edits. Outline choices are text-layer typography and use the existing undoable project edit path.

## 2. Favorite fonts

### Native persistence and typed commands

Add the existing workspace `koharu-config` dependency to `crates/koharu-app/Cargo.toml`. Cargo updates the `koharu-app` dependency entry in `Cargo.lock`; this does not introduce a new third-party package or change dependency versions.

In `crates/koharu-app/src/commands/fonts.rs`, add this private preference type:

```rust
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct FontPreferences {
    favorites: BTreeSet<String>,
}
```

Load it with `koharu_config::load::<FontPreferences>("fonts")`. This uses the existing native configuration manager and its durable save behavior. Store stable font **family names**, not a font's current index in the dropdown. The set removes duplicates and gives deterministic returned data. A favorite can remain in preferences while that font is temporarily unavailable.

Add two commands with the existing `#[tauri::command]` and `#[specta::specta]` annotations:

| Rust command | Arguments | Result |
| --- | --- | --- |
| `get_font_favorites` | none | Complete `Vec<String>` of saved favorites |
| `set_font_favorite` | `family_name: String`, `favorite: bool` | Complete updated `Vec<String>` |

The setter trims the name and rejects an empty value, more than 4,096 bytes, or an embedded NUL. It obtains a native write guard, inserts/removes the name, collects the updated names, calls `preferences.save()?`, and returns the collected list only after saving succeeds.

**Important ownership detail:** `ConfigWrite::save` consumes its guard. Collect the return value before calling `save`; do not try to read the moved guard or implement rollback by accessing it afterward.

Register both commands in `crates/koharu-app/src/commands/mod.rs` beside the existing font commands. Run the actual Rust binding generator:

```sh
cargo run -p koharu-app --bin generate
```

It regenerates `packages/bridge/src/protocol.ts` with `getFontFavorites()` and `setFontFavorite(familyName, favorite)`. Change Rust definitions and regenerate; do not maintain handwritten parallel bridge declarations.

### Shared query and mutation state

In `packages/koharu/lib/queries.ts`, add `fontFavoritesKey = ['font-favorites'] as const` and `useFontFavorites(enabled = true)`.

The hook reads through `call(commands.getFontFavorites)`. Its setter calls `call(commands.setFontFavorite, family, favorite)` and replaces the cached list with the complete native result on success. All instances use mutation key `fontFavoritesKey` and scope `{ id: 'font-favorites' }`, so changes across selectors are serialized and use shared data. `useIsMutating` supplies a shared busy flag. Pending/error loading also disables favorite toggles, preventing a misleading edit when the saved state is unknown.

The hook is enabled when the picker is open. Existing font loading and preview queries stay with their existing owners.

### Picker grouping, virtualization and accessibility

Update `packages/koharu/components/controls/FontPicker.tsx`:

1. Read favorites through the shared hook. Normalize names using the picker's existing normalization convention before comparing them.
2. Keep the current search and font filters. Build both sections from the resulting filtered family list.
3. When matching favorites exist, emit a Favorites heading and those families first, followed by an All Fonts heading and the complete filtered alphabetical list. Favorites also remain available in All Fonts.
4. Represent the virtualized list as heading rows and family rows. Use 24-pixel heading estimates and the existing 38-pixel family rows. Use stable, section-specific keys such as `favorite:<normalized-name>` and `all:<normalized-name>` so duplicate appearances are distinct virtual rows.
5. Start the list at the top when favorites exist. Scroll to the top when asynchronous preference loading introduces that section. With no favorites, retain the existing selected-font positioning behavior.
6. Put selection and star actions in separate sibling buttons. Do not nest a star button inside the font-selection button. Give the star a localized accessible label identifying its family.
7. Clicking a star invokes only the favorite mutation. It must neither change the chosen font nor close the popover.
8. Focus the search input with `preventScroll` after toggling. A favorite row can disappear from the virtualized section when unstarred; returning focus to a stable input avoids losing keyboard focus with the removed DOM node.

Preserve the existing font previews and list sizing. Review a long family name at both normal and compact window widths.

### Localization and regression coverage

Add the favorite headings/action labels, along with outline labels described below, to every `packages/koharu/public/locales/<locale>/translation.json`: `ar`, `en-US`, `es-ES`, `fr-FR`, `ja-JP`, `ko-KR`, `pt-BR`, `ru-RU`, `tr-TR`, `wuu`, `zh-CN`, and `zh-TW`.

`packages/koharu/tests/components/font-picker.test.tsx` exercises starring without selection/closure, favorite ordering, removal, filtering, restoring saved backend data after remount, and stable keyboard focus. `tests/setup.ts` adds a `scrollTo` mock for the test DOM. The native configuration manager supplies real persistence; the picker tests mock native command results. An actual restart on Windows remains an acceptance check.

## 3. Contour-based outlines

### Durable typography and every consumer

In `crates/koharu-scene/src/components/layers.rs`, introduce revisioned, serializable, typed enums:

```rust
pub enum StrokeMethod { Expansion, Contour }
pub enum StrokePosition { Outside, Center, Inside }
pub enum StrokeJoin { Round, Miter, Bevel }
```

Their wire names use `#[serde(rename_all = "snake_case")]`. Export them through both `components.rs` and the crate root `lib.rs`.

Bump `Typography` from revision 1 to revision 2 and append these fields, each annotated with `#[revision(start = 2)]`:

```rust
pub stroke_method: Option<StrokeMethod>,
pub stroke_position: Option<StrokePosition>,
pub stroke_join: Option<StrokeJoin>,
```

Keep existing numeric color and width representations. No new string color format or extension-map convention is necessary.

Update **all** constructors, copies and external views, rather than only the main Inspector path:

| File | Required work |
| --- | --- |
| `crates/koharu-app/src/commands/project.rs` | Add typed fields to the application Typography DTO; initialize new text; carry fields into scene updates and back into project views; update fixtures. |
| `crates/koharu-app/src/commands/agent/host.rs` | Add optional Agent schema enums/fields and explicit mappings into scene enums. |
| `crates/koharu-pipeline/src/stages/detection.rs` | Initialize all three fields to `None` for detected text. |
| `crates/koharu-scene/src/tests.rs` | Update Typography fixtures. |
| `crates/koharu-renderer/src/renderer.rs` | Update fixtures and resolve the three options into rendering stroke options. |
| `packages/koharu/tests/components/editor-components.test.tsx` | Update the typed UI fixture. |

Search for every `Typography {` constructor and every `stroke_width` mapping to find omissions. Regenerate the TypeScript protocol after the Rust mappings are complete. The renderer resolves absent values to **Expansion**, **Outside**, and **Round**. Expansion remains the original selectable border method.

The Inspector continues using the existing project update command. The new fields therefore travel with the normal project patch, inverse and undo/redo path; no separate outline history stack is introduced. Windows save/reopen and Undo/Redo still need the user's acceptance trial.

### Extract real glyph curves

Extend `StrokeOptions` in `crates/koharu-renderer/src/text_renderer.rs` with method, position and join. Make `TextRenderer::render` return `Result<Option<Rect>>`: extraction can fail, and a contour may extend beyond layout bounds.

For a visible, positive-width Contour stroke:

1. Iterate the already shaped `LayoutRun`: its lines, glyph IDs, actual fallback font for each glyph, advances and offsets. Do not reshape the string a second time.
2. Use each glyph font's Skrifa reference and `outline_glyphs().get(GlyphId::new(...))`.
3. Draw with `DrawSettings::unhinted(Size::new(layout.font_size), glyph.font.location())`. This preserves the current variable-font location.
4. Implement `ContourPen` for `OutlinePen`, collecting move, line, quadratic, cubic and close operations into a `kurbo::BezPath`. This supports actual font curves instead of tracing a raster mask.
5. Match existing synthetic styling. The small synthetic-bold expansion matches Vello's synthetic fill weight; it is not the new border algorithm. Apply synthetic skew, the font-to-screen Y flip, baseline, accumulated pen advances, glyph offsets, padding and baseline shift in the same coordinate system as the fill.
6. Normalize each glyph path's overall winding before combining fallback faces. Opposite dominant winding between fonts must not cancel the clipping region. Reverse all subpaths together when needed, preserving the relative winding of holes.
7. Draw the regular glyph fill with hinting disabled for this mode so fill and unhinted contour agree. Keep the original editable text and font resources.
8. Append a portable stroke command using those curves. The stroke is drawn over the fill for Center/Inside placement.

If there is no drawable outline, retain the fill and return no additional contour bounds. Keep the Expansion branch's existing dilated border followed by ordinary fill behavior.

### Portable prepared command and packet version

In `crates/koharu-rasterizer/src/prepared.rs`:

- Add `PreparedSceneCommand::StrokePath(PreparedStrokePath)`.
- Add `PreparedStrokePath { path: PreparedPath, width: f64, position: StrokePosition, join: StrokeJoin }`.
- Keep independent serializable rasterizer position/join enums so portable display data does not depend on the scene crate.
- Validate the prepared path and require finite, strictly positive width.
- Set `PREPARED_FRAME_MANIFEST_VERSION` to **5**. The separate resource-format version remains **3**.
- Export the new types from `crates/koharu-rasterizer/src/lib.rs`.

The `PreparedPath` retains its fill rule, affine transform, RGBA color and curve elements. Both native and browser consumers must understand the new command, so rebuilding Rust alone is insufficient: rebuild `koharu-canvas` WASM and then the frontend.

### Vello alignment and transparent compositing

In `crates/koharu-rasterizer/src/frame.rs`, dispatch the new command to `draw_stroke`. Compile the portable curve elements into a `BezPath`, convert position/join to Vello types, and use a miter limit of **4.0**.

The width control is the visible width for one-sided strokes:

| Position | Vello centered stroke width | Additional operation |
| --- | --- | --- |
| Center | `w` | Draw normally. |
| Inside | `2w` | Clip to the original filled contour. |
| Outside | `2w` | Isolate the stroke, remove its inward half, then composite the isolated result. |

For Outside, push an isolated normal layer bounded by the contour's inflated bounds, draw the stroke, then use an inner `Compose::DestOut` layer to remove the original filled contour. Pop both layers. The removal must operate inside the isolated stroke group: using destination-out directly on the main scene would erase the text or unrelated artwork underneath.

Honor `PreparedPath.fill` for clipping/removal, including NonZero and EvenOdd. Ordinary recorded text contours use NonZero. This is especially important for counters/holes and reusable prepared drawing data.

For Inside, push a clip layer with that same fill rule, draw the doubled stroke, and pop it. Center is an ordinary conventional stroke. Round/Miter/Bevel map directly to Vello joins; the UI calls Miter **Sharp**.

### Bounds, caching and exports

Calculate actual contour bounds and expand them by `w` for Outside, `w/2` for Center, and zero for Inside. Apply the conservative miter multiplier where relevant, transform the bounds, and return them. In `render_descriptor`, union these with the existing layout bounds when setting `local_bounds`. Keep the existing Expansion padding path.

Otherwise an outline may look correct on an unrestricted canvas but be cut off in a cropped layer or export. Do not enlarge or reshape the original layout box to simulate an outline.

The same prepared frame reaches the canvas, PNG, crops and PSD cached pixels. This milestone **does not add a separately editable Photoshop stroke effect to PSD**. Text stays editable in Koharu; the shared cached visual result includes its stroke.

### Inspector controls

In `packages/koharu/components/editor/Inspector.tsx`, add localized method, position and corner selectors, reusing existing width/color controls. Default nullable values to expansion/outside/round. Disable the position and corner selectors in Expansion mode.

Explicitly render the localized selected labels inside `SelectValue`. Merely giving menu items translated labels left the closed select showing raw enum values during review. Remove the fixed `h-48` typography section height so the added controls remain reachable at compact sizes.

## 4. Private preview and installation changes

These supporting changes make a safe, reviewable trial build; they are separate from the two editing features.

### Build and runtime reporting

Automatic approval review blocked an earlier unsanitized frontend build because Sentry could transmit source maps/build data. The provided private path prevents that disclosure:

| File | Private preview behavior |
| --- | --- |
| `packages/koharu/next.config.ts` | When `NEXT_PUBLIC_KOHARU_PREVIEW=1`, return the plain application configuration **before dynamically importing** `withSentryConfig`. Disabling uploads after constructing the plugin is insufficient for this path. |
| `packages/koharu/instrumentation-client.ts` | Skip Sentry initialization and router capture in preview mode. |
| `packages/koharu/app/global-error.tsx` | Skip Sentry exception capture in preview mode. |
| `crates/koharu/src/sentry.rs` | Return `Option<ClientInitGuard>`; return `None` when compile-time `KOHARU_PRIVATE_PREVIEW=1`. |
| `packages/koharu/app/providers.tsx` | Hide the official Updater in preview mode. |

Normal upstream behavior remains available when preview flags are unset. Every build for this delivery sets both preview flags, sets `NEXT_TELEMETRY_DISABLED=1`, and removes `SENTRY_*` and `NEXT_PUBLIC_SENTRY_*` environment variables. Do not reuse the earlier unsanitized pipeline.

### Windows build and installer

`preview/tauri.preview.conf.json` supplies the separate identifier `rs.koharu.typesetting-preview`, product name and disabled updater artifacts. It also sets `build.windows.staticVCRuntime=false`. The build explicitly uses Rust's `-C target-feature=+crt-static`; disabling Tauri's separate runtime override prevents it from requesting dynamic UCRT while cargo-xwin requests static UCRT.

`scripts/build-preview-windows.ps1` handles prerequisites, private environment, generation, WASM/frontend build, native release, runtime staging, notices and NSIS. **Direct Cargo release builds must include `--features tauri/custom-protocol`**; a release profile alone does not select Tauri's embedded production frontend.

`preview/installer.nsi` creates a per-user, unsigned setup in a separate default program folder. It stages only `.exe`, `.dll`, `.pak`, `.bin`, `.dat` and `.json` runtime files, copies `locales`/`swiftshader` when present, and excludes temporary dot-files. It includes the executable, CEF runtime/locales, licenses and install guide, creates Start menu entries and registers an uninstaller. Before packaging, `Assert-CompleteWindowsBinary` checks the MZ/PE signature, x64 machine value, section-table bounds and every raw section extent against the file length. This rejects incomplete extracted DLLs even if the downloaded archive hash was correct. The build generates an `UNINSTALL_INCLUDE` containing the exact installed file paths. The uninstaller deletes those files and uses nonrecursive directory removal, preserving unrelated files if an existing folder was chosen.

The registry string must be correctly quoted:

```nsis
WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "UninstallString" '"$INSTDIR\Uninstall.exe"'
```

Compile NSIS with `/WX` on Windows or `-WX` on Linux so string/packaging warnings fail before producing a trial installer. Include original MIT/Apache licenses and CEF `LICENSE.txt`/`CREDITS.html`, renamed to distinguish them in the installation folder. No model weights, credentials, SDK import libraries, PDBs or Rust build caches belong in the runtime package.

## 5. Reproduce from the original source

Use the exact original 0.83.5 archive as the starting point. Preserve a copy. Its SHA256 is `58bb374c356e8a67715f74ccfc9ba62a8ba4b58089f4ce1277a8ca4484271b8f`. The patch also adds this guide and progress/build documents.

1. Extract the original source into a new folder.
2. Copy the companion patch outside that folder. From the source root, run `git apply --check <path-to-patch>` and then `git apply <path-to-patch>`. A Git checkout is useful for inspection; `git apply` can also operate on the extracted files.
3. Alternatively, follow sections 2–4 manually, using the complete modified files as the exact reference. Update authoritative Rust owners before UI types.
4. Install the tool versions/prerequisites in `BUILD_PREVIEW.md`. Use the supplied lockfiles; do not silently downgrade dependencies to make an old toolchain compile them.
5. Regenerate typed bindings, rebuild WASM, then build the private frontend and native application. The provided PowerShell script performs these steps on Windows.
6. Run the focused checks below and inspect the actual UI.
7. Stage only runtime files, generate the exact uninstall list, build the installer with warnings treated as errors, extract it, and compare every staged file with its extracted counterpart.

For a manual private build, set preview/reporting isolation first. The essential build order is:

```sh
cargo run -p koharu-app --bin generate
bun run --filter @koharu/bridge build
bun run --filter @koharu/app build
cargo build -p koharu --release --features tauri/custom-protocol
```

Those commands assume the private environment and Tauri preview configuration from the supplied PowerShell script. Without those settings they are not the provided private preview build.

If the optional `wasm-opt` download is unavailable, build the same Rust canvas with wasm-pack 0.15.0 and `--no-opt`. Keep target `web`, output name `koharu_canvas`, `--no-pack`, and output directory `packages/bridge/src/wasm`. Skipping optional post-optimization does not remove the contour implementation. Rebuild the frontend afterward so it includes the resulting WASM.

The generated protocol is included in the exact text patch because it was produced by the actual generator. Generated WASM is included in the full source checkpoint for recovery convenience, while rebuilding remains the reproducible path. Exclude `target`, `node_modules`, `.next`, `out`, `dist`, `.review`, `*.tsbuildinfo` and `next-env.d.ts` from source deliveries.

### Linux cross-build notes

The trial was cross-built for `x86_64-pc-windows-msvc` with Rust 1.97.1, Bun 1.3.14, LLVM/clang-cl/lld 22.1.8, cargo-xwin 0.23.1, and the official CEF 154.0.34 Windows archive. The normal Windows rebuild script does not require reproducing the Linux sysroot.

For another Linux build:

- Prepare the Microsoft CRT/SDK through `cargo xwin env --target x86_64-pc-windows-msvc` before compilation. Preserve its cache until the build finishes.
- Verify the official CEF archive hash against its index entry. Prepare the layout as `download-cef` does: Release files at the CEF root, Resources entries alongside them, plus include/cmake/libcef_dll and notices. Its `archive.json` records the verified archive's type, name and SHA1. Set `CEF_PATH` to that prepared root.
- Supply LLVM 22's resource directory through `CL_FLAGS`, which cargo-xwin applies to target C/C++ flags. Do not set global CFLAGS/CXXFLAGS to that option: host GCC rejects it.
- A small local clang-cl wrapper handles POSIX absolute input paths by inserting `--` where needed. It also enables `/clang:-msse4.1` **only** for libwebp vendor translation units ending in `sse41.c`. Their runtime dispatcher checks CPU support. Do not broaden the whole application's CPU requirement with a global SSE4.1 flag.
- If necessary, stage CMake's `libuv` and LLVM's `libz3` dependencies. These are build-environment repairs, not application dependency changes.
- If the runtime supplies an HTTP proxy and xwin cannot use its default SOCKS configuration, use the supplied HTTPS proxy through `ALL_PROXY`; never put actual proxy credentials/URLs into project files or documentation.
- Apply the same private flags, preview JSON, static CRT flag and production protocol feature. Use `cargo xwin build -p koharu --release --target x86_64-pc-windows-msvc --features tauri/custom-protocol`.

Validate extracted binary lengths/PE sections as well as the archive hash. In this session, extraction left `libcef.dll` and `dxcompiler.dll` truncated even though the archive was intact. Re-reading those two complete members from the same verified minimal archive restored valid files. No dependency version or compiled application code changed. All 237 CEF runtime/notice files were subsequently matched by size and SHA256 against members of the verified minimal archive, whose SHA1 is `be54bbaf3222191dec59ffd9c9ec7e8a7f44a26d`. The initial portable checkpoint was replaced after all runtime PE files parsed and every portable member matched staging by size and SHA256.

Save the validated staged portable runtime immediately after a successful native build, before lengthy NSIS compression. This resumed build does so automatically. Temporary toolchains and native binaries were cleared during a prior usage-limit interruption; the saved source checkpoint preserved both implementations. The portable and final installer checkpoints prevent another interruption from requiring the completed executable to be rebuilt.

## 6. Verification and acceptance

The completed implementation checks from the prior session include native app/generator compilation and real binding generation; renderer checking; the focused native compositor pixel test; a real contour-glyph prepared-packet round-trip; 7 focused font/localization tests; an additional rerun of the 2 font tests after focus hardening; TypeScript; focused lint; Rust formatting; final WASM; and private production frontend builds. The private frontend rebuild passed again after restoration. The resumed Windows x64 optimized release build also passed; its only linker warning concerns unavailable Microsoft SDK PDB debug metadata, which does not require shipping those debugger files.

The UI review used actual FontPicker/Inspector components in a headless browser with a mocked native backend at 1100×780 and 900×600, including a long font name. It identified the selected-label and fixed-height issues described above. That review is not a claim of Windows desktop or final WebGPU acceptance. Temporary review screenshots/harnesses are not part of the delivery.

Focused reproduction commands, from the source root with dependencies available:

```sh
cargo check -p koharu-renderer
cargo test -p koharu-rasterizer --test compositor contour_stroke_alignment_preserves_transparency_and_background
cargo test -p koharu-renderer text_renderer::tests::rendered_text_survives_prepared_packet_round_trip
cargo fmt --all --check
bun run --filter @koharu/app test tests/components/font-picker.test.tsx tests/lib/localization.test.ts
bun x tsc --noEmit --project packages/koharu/tsconfig.json
```

Run the existing frontend linter on changed frontend files as well. Native tests use the normal debug profile and a usable GPU/software adapter where required. Full test suites, end-to-end tests and a Windows GUI launch were not claimed. Final release/package results are recorded in `FEATURE_PROGRESS.md`.

The compositor test protects Outside/Center/Inside pixel placement, transparent interiors, and preservation of artwork below an outside stroke. The renderer test protects real glyph contour recording and portable packet reconstruction. The picker tests protect interaction and shared saved-data behavior. These checks cover different boundaries; a green TypeScript check alone would not verify rendering or persistence.

Before accepting the milestone, the user should test actual Windows launch, favorite persistence across restart, the previously problematic font, each alignment/join, save/reopen, Undo/Redo and canvas/PNG comparison. Keep trial projects separate from files opened in older Koharu versions. PSD cached pixels share the rendering path, but editable Photoshop stroke effects are outside this milestone.

## 7. Files changed

Core font implementation: `crates/koharu-app/Cargo.toml`, `Cargo.lock`, `crates/koharu-app/src/commands/fonts.rs`, `crates/koharu-app/src/commands/mod.rs`, `packages/koharu/lib/queries.ts`, `packages/koharu/components/controls/FontPicker.tsx`.

Scene and command integration: `crates/koharu-scene/src/components/layers.rs`, `crates/koharu-scene/src/components.rs`, `crates/koharu-scene/src/lib.rs`, `crates/koharu-app/src/commands/project.rs`, `crates/koharu-app/src/commands/agent/host.rs`, `crates/koharu-pipeline/src/stages/detection.rs`, and generated `packages/bridge/src/protocol.ts`.

Rendering: `crates/koharu-renderer/src/renderer.rs`, `crates/koharu-renderer/src/text_renderer.rs`, `crates/koharu-rasterizer/src/prepared.rs`, `crates/koharu-rasterizer/src/frame.rs`, `crates/koharu-rasterizer/src/lib.rs`.

UI/locales: `packages/koharu/components/editor/Inspector.tsx` and the 12 locale files listed in section 2.

Tests/fixtures: `crates/koharu-rasterizer/tests/compositor.rs`, `crates/koharu-scene/src/tests.rs`, renderer/project fixtures in their implementation files, `packages/koharu/tests/components/font-picker.test.tsx`, `packages/koharu/tests/components/editor-components.test.tsx`, and `packages/koharu/tests/setup.ts`.

Private preview: `crates/koharu/src/sentry.rs`, `packages/koharu/next.config.ts`, `packages/koharu/instrumentation-client.ts`, `packages/koharu/app/global-error.tsx`, `packages/koharu/app/providers.tsx`, `preview/tauri.preview.conf.json`, `preview/installer.nsi`, `scripts/build-preview-windows.ps1`.

Documentation: `BUILD_PREVIEW.md`, `PREVIEW_INSTALL.md`, `FEATURE_PROGRESS.md`, `RESUME_WORK.md`, and this `IMPLEMENTATION_HANDOFF.md`.

## 8. Handoff rule

Deliver the first-two-features installer/portable build, source, this guide and the checklist. Then leave the project **on temporary hold**. Preserve the original upload and frozen Preview 1 source. A future developer should inspect the saved checklist and the user's latest instruction before changing anything; the unimplemented backlog is not authorization to start it.
