# Install Booharu

Booharu is a personal Windows fork of [Koharu](https://github.com/koharu-rs/koharu). It keeps its application data separate from official Koharu, so both can stay installed. It starts with its own blank project list and default preferences.

## Portable ZIP

1. Download **Booharu-<version>-Portable.zip**.
2. In File Explorer, right-click the ZIP and choose **Extract All**.
3. Choose a writable folder, for example `D:\Booharu`. Do not run the app from inside the ZIP.
4. Open the extracted **Booharu** folder and double-click **koharu.exe**. Keep all the bundled files together.
5. The app creates **PreviewData** beside koharu.exe for its settings, projects, logs and downloaded models.

To update, extract the new ZIP and move the old folder's **PreviewData** into the new **Booharu** folder before the first start.

## Installer

1. Download **Booharu-<version>-Setup.exe**.
2. Follow the installer. The default destination is **Booharu** under your Windows account's Programs folder.
3. Open **Booharu** from the Start menu.

The uninstaller removes only the installed program files and keeps PreviewData, which can contain your projects.

Booharu is unsigned, so Windows may warn about an unknown publisher; choose **More info**, then **Run anyway**. Sentry reporting and the official updater are turned off.

## What stays separate

- Settings and favorites: PreviewData/settings/config.toml.
- Automatically saved projects: PreviewData/projects.
- Downloaded models and native inference packages: PreviewData/packages.
- Browser storage: PreviewData/browser.
- Application and browser logs: PreviewData/logs.
- Optional API keys and sign-in: separate entries in the Windows credential store. These do not travel with the portable folder.

If PreviewData cannot be written, startup stops instead of using official Koharu's directories.

Projects saved by Booharu can use text-style fields that official Koharu 0.83.5 cannot read. To try an existing project, copy its whole .khrproj folder from Documents\Koharu into PreviewData/projects and keep the original for official Koharu.
