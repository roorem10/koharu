# Test the combined WO-00 preview

This build includes the outline alpha correction and all five fixes from the revised Windows patch 04. It keeps font favorites, the startup fix, and isolated preview data.

## Update your working preview

1. Close Koharu Typesetting Preview completely.
2. Open your working preview folder:

   `D:\KoharuStartupTest\Koharu-Typesetting-Preview-2-Startup-Fix`

3. Rename its existing `koharu.exe` to **`koharu-before-WO00.exe`**. Keep that backup. If the name already exists, choose another backup name.
4. Download **Koharu-WO00-Executable-Update.zip**. Right-click it and choose **Extract All** into a temporary folder in Downloads.
5. Copy the extracted **`koharu.exe`** into your working preview folder from step 2.
6. Run **`koharu.exe` from that working preview folder**.

Keep **PreviewData** in place. Keeping the same directory retains the preview's projects, favorite fonts, preferences, and stored project paths. The original Koharu installation remains separate. The canvas code is embedded in the executable; no extra files or developer tools are required.

For a fresh separate trial, extract **Koharu-Typesetting-Preview-2-WO00-Portable.zip** into a new folder and run the executable there. That new folder creates its own PreviewData.

## Tests

### 1. Outline quality

Use the same blue page and **CCWildWords Regular / weight 400 / size 300 / white outline 24.5 / Contour Stroke / Outside / Round**. Check the outer white curves at 100% zoom in the canvas and an original-size PNG export. Also try white or colored lettering over the page image and an Expansion outline.

Put the original exported PNG in a ZIP before attaching it, so it can be inspected at its original resolution.

### 2. Color picker

Drag the text color to black, release, and close the picker. The lettering should keep the chosen black. A drag whose end event is missed should still apply when the picker closes.

For cancellation, start dragging while holding the mouse button, then press Escape. An unfinished color choice should be discarded. Also check that choosing Transparent, entering a hex code, or using the eyedropper keeps the later choice when the picker closes.

### 3. Tool shortcuts after using controls

Adjust a sidebar slider, then press a bare tool key such as V or T. It should switch tools. Space, arrow keys, Delete, and modified shortcuts should stay with the focused slider/control. Typing in a text field should keep typing rather than switch tools.

### 4. Shortcut persistence

Open Settings, then Shortcuts. Change one tool key to an unused single letter. Close the program completely and reopen the same executable from the same folder. The chosen key should remain saved and work in the editor.

### 5. Tiny text areas

Export a page that previously failed with **“text inset leaves no layout area.”** Small positive-size balloon cells should now retain usable layout space. A genuinely zero-size cell remains invalid.

### 6. Multipage export

Export a project with at least five pages to PNG and CBZ; try PSD as well if you use it. Check page count, names, order, and the last page. The application now processes four pages at a time to reduce export memory use. This is a page-count bound, not a fixed memory limit.

If PNG or PSD export fails partway, pages already written can remain in the destination folder. A CBZ is published only after it finishes successfully.

### 7. Existing behavior

Confirm startup, favorite fonts after relaunch, and normal black lettering still work. Screenshots of the open color picker and the Shortcuts settings page after relaunch would complete the UI review; automated browser capture was blocked by this build host's socket restriction.

## Roll back

Close the preview. Remove or rename the new `koharu.exe`, then rename your backup to `koharu.exe`. Leave PreviewData in place.

## Scope

The withdrawn nested-detection change is excluded. WO-01 and the later features have not started. This is a private testing build; owner visual acceptance remains pending.
