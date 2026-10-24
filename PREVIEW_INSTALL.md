# Try Koharu Typesetting Preview 1

This preview adds **favorite fonts** and **contour-based text outlines** to the supplied Koharu 0.83.5 source. Perspective Transform, rich text and Mesh Warp are pending. The project will be on temporary hold after this delivery.

## Install on Windows 11, 64-bit

1. Download `Koharu-Typesetting-Preview-1-Setup.exe`.
2. Close your existing Koharu window.
3. Double-click the downloaded installer and follow its steps. The default destination is a separate **KoharuTypesettingPreview** folder under your Windows user account's Programs folder.
4. Open **Koharu Typesetting Preview** from the Start menu, or leave **Open Koharu Typesetting Preview** selected on the installer's last screen.

The installer includes the editor and CEF runtime. You do not need Rust, Bun, or other developer tools to try it. Model weights are not bundled.

This local preview is unsigned, so Windows may identify its publisher as unknown. Its uninstaller removes the preview program, not your Documents projects or native Koharu preferences.

Sentry error reporting and source-map uploads are disabled in this private preview.

## Portable backup

The portable ZIP contains the same application and is also saved before installer compression finishes.

1. Download `Koharu-Typesetting-Preview-1-Portable.zip`.
2. Right-click the ZIP in File Explorer and choose **Extract All**. Extract it into a normal folder, such as a new Koharu Preview folder in Documents.
3. Open the extracted **Koharu-Typesetting-Preview-1** folder and double-click **koharu.exe**. Keep all its files and the locales folder together. Do not run it from inside the ZIP.

## First test

Create a small new project with one or two pages. This avoids changing a working chapter while you try the preview.

**Favorite fonts**

1. Open a font selector and click the star beside a font.
2. The selected font should stay unchanged and the selector should remain open.
3. Check that the starred font appears in **Favorites** above **All Fonts**.
4. Select a different font, change pages, and restart the preview. Your favorites should remain available.
5. Click a filled star to remove a favorite. Search and font filters continue to work in both sections.

**Contour outlines**

1. Select a text box and enable its border with a contrasting color and a visible width.
2. In **Outline method**, choose **Contour stroke**.
3. Compare **Outside**, **Center** and **Inside**. Center and Inside intentionally cover part of the text fill.
4. Try **Round**, **Sharp** and **Bevel** corners. Sharp corners use a bounded miter to prevent extreme spikes.
5. Try the font that produced the rectangular protrusions in the original version. Compare it with **Expansion**, which retains the original border method.
6. Save the project, close and reopen it, and check the outline settings. Try Undo/Redo after changing a setting.
7. Export a PNG and compare its border with the canvas. Layer crops and PSD cached pixels use the same prepared rendering path. This does not add a separate editable Photoshop stroke effect.

Text remains editable in Koharu. Favorites are application preferences; outline settings belong to the text layer. Project files containing the new outline fields should be treated as preview projects when returning to older Koharu versions.

## Verification and recovery

The rebuilt Windows x64 application and setup compiled successfully. All 242 runtime files were verified against the installer; the portable files match staging by size and SHA256. Focused rendering, interaction, localization, TypeScript and lint checks passed. Detailed checks are recorded in FEATURE_PROGRESS.md. A Windows GUI launch has not been verified here and is the purpose of your trial.

FEATURE_PROGRESS.md tracks all five additions. RESUME_WORK.md records what to resume if the session is interrupted. The saved source checkpoint contains the modified source and these notes; it does not contain model weights, credentials, or build caches.
