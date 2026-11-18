param([switch]$SkipDependencies)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

function Assert-CompleteWindowsBinary {
    param([string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    $reader = [System.IO.BinaryReader]::new($stream)
    try {
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { throw "Invalid Windows binary: $Path" }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        if ([UInt64]$peOffset + 24 -gt $stream.Length) { throw "Truncated Windows binary: $Path" }
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "Expected a Windows x64 binary: $Path" }
        $sectionCount = $reader.ReadUInt16()
        $stream.Position = $peOffset + 20
        $optionalSize = $reader.ReadUInt16()
        $sectionStart = [UInt64]$peOffset + 24 + $optionalSize
        if ($sectionStart + [UInt64]$sectionCount * 40 -gt $stream.Length) { throw "Truncated section table: $Path" }
        for ($index = 0; $index -lt $sectionCount; $index++) {
            $stream.Position = $sectionStart + $index * 40 + 16
            $rawSize = $reader.ReadUInt32()
            $rawOffset = $reader.ReadUInt32()
            if ($rawSize -gt 0 -and [UInt64]$rawOffset + [UInt64]$rawSize -gt $stream.Length) {
                throw "Incomplete extracted binary; restore it from the verified CEF archive: $Path"
            }
        }
    }
    finally { $reader.Dispose() }
}

foreach ($program in @('cargo', 'bun', 'cmake', 'ninja')) {
    if (-not (Get-Command $program -ErrorAction SilentlyContinue)) {
        throw "Missing $program. See BUILD_PREVIEW.md for the Windows build prerequisites."
    }
}
$makensis = Get-Command makensis -ErrorAction SilentlyContinue
if ($makensis) { $makensis = $makensis.Source }
else { $makensis = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe' }
if (-not (Test-Path $makensis)) { throw 'NSIS is missing. See BUILD_PREVIEW.md.' }

$env:NEXT_PUBLIC_KOHARU_FORK_VERSION = (Get-Content (Join-Path $root 'FORK_VERSION') -Raw).Trim()
if (-not $env:NEXT_PUBLIC_KOHARU_FORK_VERSION) { throw 'FORK_VERSION is empty.' }
# "Booharu 0.0.5" packages as Booharu-0.0.5; a work-order build such as "WO-03" as Booharu-WO-03.
$releaseVersion = $env:NEXT_PUBLIC_KOHARU_FORK_VERSION -replace '^Booharu\s+', ''
$packageName = 'Booharu-' + ($releaseVersion -replace '[^A-Za-z0-9._-]', '-')
# Windows file versions take four numbers; work-order builds have none.
$fileVersion = if ($releaseVersion -match '^\d+\.\d+\.\d+$') { "$releaseVersion.0" } else { '0.0.0.0' }

# CI passes the commit it builds; hand builds read it from git when there is a checkout.
if (-not $env:KOHARU_FORK_COMMIT -and (Test-Path (Join-Path $root '.git'))) {
    $commit = git -C $root rev-parse --short=8 HEAD 2>$null
    if ($LASTEXITCODE -eq 0 -and $commit) { $env:KOHARU_FORK_COMMIT = $commit.Trim() }
}
Remove-Item Env:NEXT_PUBLIC_KOHARU_FORK_COMMIT -ErrorAction SilentlyContinue
if ($env:KOHARU_FORK_COMMIT) {
    $env:NEXT_PUBLIC_KOHARU_FORK_COMMIT = $env:KOHARU_FORK_COMMIT.Substring(0, [Math]::Min(8, $env:KOHARU_FORK_COMMIT.Length))
}

# Booharu reports nothing to Koharu's Sentry: without SENTRY_DSN at build time the program
# has no address to send to, and without credentials the interface build uploads no source maps.
$env:NEXT_TELEMETRY_DISABLED = '1'
Get-ChildItem Env: | Where-Object { $_.Name -like 'SENTRY_*' -or $_.Name -like 'NEXT_PUBLIC_SENTRY_*' } |
    ForEach-Object { Remove-Item ("Env:" + $_.Name) }
# Koharu keeps release debug information for Sentry's stack traces, which Booharu never sends.
$env:CARGO_PROFILE_RELEASE_DEBUG = '0'
# Where the CEF build script downloads Chromium Embedded Framework, so this script knows
# where to take the runtime and its notices from. CI sets its own, outside target\.
if (-not $env:CEF_PATH) { $env:CEF_PATH = Join-Path $root 'target\cef' }
# The CEF build script (cef-dll-sys) names the CEF it uses after the build part of its own locked
# version: 154.5.0+154.0.34 is CEF 154.0.34. The workflow's CEF cache reads it the same way.
$cargoLock = Get-Content (Join-Path $root 'Cargo.lock') -Raw
$lockedCef = [regex]::Matches($cargoLock, '(?m)^name = "cef-dll-sys"\r?\nversion = "([^"]+)"')
if ($lockedCef.Count -ne 1) { throw 'Cannot read the locked cef-dll-sys version from Cargo.lock.' }
$cefVersion = $lockedCef[0].Groups[1].Value -replace '^[^+]*\+', ''

if (-not $SkipDependencies) {
    # Koharu's postinstall installs the Tauri CLI, which this build doesn't use: it calls Cargo directly.
    Invoke-Checked 'bun' @('install', '--frozen-lockfile', '--ignore-scripts')
}
Invoke-Checked 'bun' @('run', '--filter', '@koharu/app', 'build')
# Direct Cargo builds must opt into Tauri's bundled frontend protocol. --locked builds exactly what
# Cargo.lock holds, so the CEF read from it above is the one the program is built for: a lock that
# no longer fits Cargo.toml fails here instead of being rewritten for another CEF.
Invoke-Checked 'cargo' @('build', '-p', 'koharu', '--release', '--locked', '--features', 'tauri/custom-protocol')

$out = Join-Path $root 'dist'
$stage = Join-Path $out 'Booharu'
foreach ($userData in @('projects', 'store', 'PreviewData')) {
    if (Test-Path (Join-Path $stage $userData)) {
        throw "The staging folder contains user data ($userData). Move the whole dist\Booharu folder somewhere safe before rebuilding."
    }
}
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item (Join-Path $root 'target\release\Booharu.exe') -Destination $stage -Force

# The CEF runtime goes beside the program. Take it from the CEF distribution itself rather
# than from target\release, where a restored CI cache can leave an older copy or none.
# Find it where the CEF build script took it: CEF_PATH\<CEF version>\cef_windows_x86_64 when
# that version folder exists (the build script downloads CEF there), otherwise CEF_PATH itself.
$cefRoot = Join-Path $env:CEF_PATH $cefVersion
if (Test-Path $cefRoot) { $cefRoot = Join-Path $cefRoot 'cef_windows_x86_64' } else { $cefRoot = $env:CEF_PATH }
# The CEF build script also accepts a CEF older than the locked one; Booharu ships only the locked one.
$archiveJson = Join-Path $cefRoot 'archive.json'
if (-not (Test-Path $archiveJson)) { throw "Cannot find the CEF distribution: $archiveJson is missing." }
$archiveName = [string]((Get-Content $archiveJson -Raw | ConvertFrom-Json).name)
if ([regex]::Match($archiveName, '^cef_binary_([^+]+)').Groups[1].Value -ne $cefVersion) {
    throw "Cargo.lock locks CEF $cefVersion, but $cefRoot holds $archiveName. Remove that folder so the next build downloads CEF $cefVersion."
}
Write-Host "Staging CEF $cefVersion from $cefRoot"
$runtimeExtensions = @('.exe', '.dll', '.pak', '.bin', '.dat', '.json')
Get-ChildItem $cefRoot -File | Where-Object { $_.Extension -in $runtimeExtensions -and $_.Name -ne 'archive.json' } |
    Copy-Item -Destination $stage -Force
foreach ($directory in @('locales', 'swiftshader')) {
    $path = Join-Path $cefRoot $directory
    if (Test-Path $path) { Copy-Item $path -Destination $stage -Recurse -Force }
}
Copy-Item (Join-Path $root 'LICENSE-MIT'), (Join-Path $root 'LICENSE-APACHE'), (Join-Path $root 'PREVIEW_INSTALL.md') -Destination $stage -Force

# Cargo's CEF download keeps CREDITS.html but drops the archive's LICENSE.txt; fall back to the checked-in copy.
$cefLicense = Join-Path $cefRoot 'LICENSE.txt'
if (-not (Test-Path $cefLicense)) { $cefLicense = Join-Path $root 'preview\CEF-LICENSE.txt' }
Copy-Item $cefLicense -Destination (Join-Path $stage 'CEF-LICENSE.txt') -Force
Copy-Item (Join-Path $cefRoot 'CREDITS.html') -Destination (Join-Path $stage 'CEF-CREDITS.html') -Force

Get-ChildItem $stage -File | Where-Object { $_.Extension -in @('.exe', '.dll') } |
    ForEach-Object { Assert-CompleteWindowsBinary $_.FullName }
if (Get-ChildItem $stage -Recurse -File -Force | Where-Object { $_.Name.StartsWith('.') }) {
    throw 'Temporary dot-files are present in runtime staging. Remove them before packaging.'
}

# Remove only the files we install, even when an existing folder is chosen, so the uninstaller
# never touches the projects and store folders the program keeps beside itself.
$uninstallInclude = Join-Path $out 'uninstall-preview-files.nsh'
$removals = @()
Get-ChildItem $stage -Recurse -File | ForEach-Object {
    $relative = $_.FullName.Substring($stage.Length + 1).Replace('$', '$$')
    $removals += 'Delete "$INSTDIR\' + $relative + '"'
}
Get-ChildItem $stage -Recurse -Directory | Sort-Object { $_.FullName.Length } -Descending | ForEach-Object {
    $relative = $_.FullName.Substring($stage.Length + 1).Replace('$', '$$')
    $removals += 'RMDir "$INSTDIR\' + $relative + '"'
}
Set-Content -Path $uninstallInclude -Value $removals -Encoding utf8

Invoke-Checked $makensis @('/WX', "/DRUNTIME_DIR=$stage", "/DOUTPUT_DIR=$out", "/DSOURCE_DIR=$root", "/DUNINSTALL_INCLUDE=$uninstallInclude",
    "/DPACKAGE_NAME=$packageName", "/DPRODUCT_VERSION=$releaseVersion", "/DFILE_VERSION=$fileVersion", (Join-Path $root 'preview\installer.nsi'))
$installer = Join-Path $out "$packageName-Setup.exe"
$portable = Join-Path $out "$packageName-Portable.zip"
Compress-Archive -LiteralPath $stage -DestinationPath $portable -Force

# The two release downloads and their checksums, in sha256sum format.
$installer, $portable | ForEach-Object {
    "$((Get-FileHash $_ -Algorithm SHA256).Hash.ToLower())  $(Split-Path $_ -Leaf)"
} | Set-Content (Join-Path $out 'SHA256SUMS.txt') -Encoding ascii
Write-Host "Installer created: $installer"
Write-Host "Portable ZIP created: $portable"
