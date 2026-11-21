# Install Booharu

Booharu is a personal Windows fork of [Koharu](https://github.com/koharu-rs/koharu). Booharu 0.0.5 is built on Koharu 0.89.0. It keeps its settings, projects and window apart from official Koharu, so both can stay installed and run at the same time.

## Portable ZIP

1. Download **Booharu-<version>-Portable.zip**.
2. In File Explorer, right-click the ZIP and choose **Extract All**.
3. Choose a writable folder, for example `D:\Booharu`. Do not run the app from inside the ZIP.
4. Open the extracted **Booharu** folder and double-click **Booharu.exe**. Keep all the bundled files together.
5. The app creates **projects** and **store** folders beside Booharu.exe for your projects and the models it downloads.

To update, extract the new ZIP and move the old folder's **projects** and **store** folders into the new **Booharu** folder before the first start.

## Installer

1. Download **Booharu-<version>-Setup.exe**.
2. Follow the installer. The default destination is **Booharu** under your Windows account's Programs folder.
3. Open **Booharu** from the Start menu.

The uninstaller removes only the installed program files and keeps the **projects** and **store** folders.

Booharu is unsigned, so Windows may warn about an unknown publisher; choose **More info**, then **Run anyway**. Sentry reporting and Koharu's updater are turned off.

## Where Booharu keeps things

- Projects: the **projects** folder beside Booharu.exe.
- Downloaded models and the GPU libraries that run them: the **store** folder beside Booharu.exe. The first start downloads the GPU libraries and each model downloads the first time it runs, so both need a network connection and several gigabytes of free space.
- If Booharu can't write to the folder it's in, such as a folder under Program Files, the **projects** and **store** folders go in `%LOCALAPPDATA%\Booharu` instead, never in official Koharu's folders.
- Settings: `%USERPROFILE%\.booharu\config.toml` (official Koharu uses `.koharu`).
- API keys and sign-ins: `%USERPROFILE%\.booharu\secrets.age`, locked with a key kept in the Windows credential store under **booharu**. These do not travel with the portable folder.
- Logs: `%LOCALAPPDATA%\Booharu\logs`. Browser storage: `%LOCALAPPDATA%\Booharu\cef`.

## Agents in other programs

While Booharu runs, an MCP client on the same computer can work on the open project at `http://127.0.0.1:47821/mcp`. Official Koharu keeps `127.0.0.1:7878`, so both can run at once. To use another port, from 1024 to 65535, set it in `%USERPROFILE%\.booharu\config.toml` and restart Booharu, which reads it only at start-up, for example:

```toml
[booharu-mcp]
port = 47822
```

A port outside that range is read as 47821. If the port is taken, or `config.toml` isn't valid TOML, Booharu starts without the MCP server. Agents connected over MCP can't change this setting.

## Projects from Booharu 0.0.4 and earlier

Booharu 0.0.5 uses Koharu's project format (`.khr`), the same from Koharu 0.85 through 0.89. It cannot open `.khrproj` projects made by Booharu 0.0.4 or earlier, or by Koharu 0.83 and earlier. Booharu 0.0.4 and earlier kept everything in a **PreviewData** folder beside the program; installing 0.0.5 leaves that folder alone.

To finish an old chapter, keep the old version: install 0.0.5 into a different folder, or use the portable ZIP. The installer warns before it replaces an older Booharu.
