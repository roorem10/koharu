# Try isolated Koharu Typesetting Preview 2

This Windows 11 x64 preview adds favorite fonts and contour text outlines. Preview 2 corrects overlapping contour boundaries, improves opaque Outside outlines, and isolates its application data from official Koharu. It starts with its own blank project list and default preferences. Your original Koharu can remain installed.

## Recommended: portable ZIP

1. Download **Koharu-Typesetting-Preview-2-Portable.zip**.
2. In File Explorer, right-click the ZIP and choose **Extract All**.
3. Choose a new writable folder on any drive, for example `D:\Koharu-Preview` or a new folder in Documents. Keep it separate from your original Koharu installation. Do not run the app from inside the ZIP.
4. Open the extracted **Koharu-Typesetting-Preview-2** folder and double-click **koharu.exe**. Keep all the bundled files together.
5. The app creates **PreviewData** beside koharu.exe. A blank project list is expected; your official projects have not been moved or deleted.

No Rust, Bun or developer tools are needed. The bundle includes the editor and CEF runtime. Model weights and inference packages can download when needed and are stored separately in PreviewData/packages. This may use additional disk space even if official Koharu has already downloaded the same resources.

## Installer option

1. Download **Koharu-Typesetting-Preview-2-Setup.exe**.
2. Follow the installer and keep its separate default destination, **KoharuTypesettingPreview**, under your Windows account's Programs folder.
3. Open **Koharu Typesetting Preview** from the Start menu.

The installer has its own shortcuts and uninstall entry. Its uninstaller removes only the installed program files and preserves PreviewData, which can contain your trial projects. The preview is unsigned and Windows may display an unknown publisher. Sentry reporting is disabled, and this preview does not register the official updater.

## What stays separate

- Settings and favorites: PreviewData/settings/config.toml.
- Automatically saved projects: PreviewData/projects.
- Downloaded models and native inference packages: PreviewData/packages.
- Browser storage: PreviewData/browser.
- Application and browser logs: PreviewData/logs.
- Window position/size: PreviewData/settings/window-state.json.
- Optional API keys and sign-in: separate preview entries in the Windows credential store; official credentials are not imported or changed. These protected entries do not travel with the portable folder.

No launcher or Windows environment-variable changes are needed. If PreviewData cannot be written, startup stops instead of using official Koharu's directories. Windows fonts and drivers are used normally; this is application-data isolation, not a Windows sandbox. The window-state plugin may create a separate preview-named AppData directory during initialization. Explicit imports read the file you choose, and exports write to the destination you choose.

## Optional: test a copy of an existing project

Start with a new trial project if possible. To test existing work:

1. Close both versions of Koharu.
2. In File Explorer, open **Documents**, then **Koharu**. If Windows redirects Documents to OneDrive, use that actual Documents folder.
3. Copy the **entire .khrproj project folder** you want to test. Use Copy, not Cut.
4. Paste the copy into the preview's **PreviewData/projects** folder. Launch the preview and open that copied project from its library.

The preview automatically saves edits. New or updated text styles use a newer format that official Koharu 0.83.5 cannot read. Keep the trial copy in the preview; retain your original project for the official program. The preview performs no automatic project or settings migration.

## First test

Create a small new project with one or two pages. The preview saves edits automatically in its own PreviewData/projects folder. New text starts with Contour stroke; an old copied text box can retain Expansion until changed.

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

Build and package checks are recorded in FEATURE_PROGRESS.md. Actual Windows launch and editing behavior have not been verified here and remain the purpose of your trial. PREVIEW2_IMPLEMENTATION.md explains the exact isolation and outline corrections.

The saved source includes both the original implementation guide and the Preview 2 addendum. Perspective Transform, rich text and Mesh Warp remain on hold. The older Preview 1 downloads are retained for reference and use shared data paths; use the Preview 2 downloads for this trial.
