# Builds the Windows packages in dist/: a portable zip, and with -Installer a per-machine NSIS installer.
#
# usage: pwsh tools/package_windows.ps1 [-Installer] [-Smoke]     (Windows PowerShell 5.1 or PowerShell 7)
#   dist/Tempester-<version>-windows-x64.zip         tempester.exe, tempest.ico, LICENSE, and plugins/ (the CLAP and
#                                                    VST3 bundles of Tempester Control, to copy by hand, README.txt)
#   dist/Tempester-<version>-windows-x64-setup.exe   with -Installer (needs makensis on the PATH, NSIS): app plus the
#       two plugins as optional components, for all users, administrator rights (see tools/windows/tempester.nsi)
#   $env:TEMPESTER_PLUGIN_BUNDLES=<folder> uses plugin bundles already built instead of `cargo xtask bundle`.
#   -Smoke: starts the built exe on the simulated machine (TEMPESTER_FAKE_MIDI=1) and checks it is still running
#           after a few seconds (a window opened, WebView2 present). Nothing reaches a MIDI port.
# The release exe is a GUI program (no console), so `--version` prints nothing there: the version is checked from the
# manifest and the file names. The exe itself has no embedded icon yet (needs a resource crate, awaiting approval);
# the window icon is set at run time and the installer shortcut uses tempest.ico.
# Every check ends the script with an error. Nothing DSI is ever packaged.
param(
    [switch]$Installer,
    [switch]$Smoke
)
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$Dist = if ($env:TEMPESTER_DIST) { $env:TEMPESTER_DIST } else { Join-Path $Root 'dist' }
$Icons = Join-Path $Root 'app/crates/tempester/icons'
$PluginName = 'Tempester Control'

$Match = Select-String -Path (Join-Path $Root 'app/Cargo.toml') -Pattern '^version = "(.*)"' | Select-Object -First 1
if (-not $Match) { throw 'no version in app/Cargo.toml' }
$Version = $Match.Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

function Assert-Package([string]$Path, [int64]$MinimumSize) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "package missing: $Path" }
    $Size = (Get-Item -LiteralPath $Path).Length
    if ($Size -lt $MinimumSize) { throw "package too small ($Size bytes < $MinimumSize): $Path" }
    if ((Split-Path -Leaf $Path) -notlike "*$Version*") { throw "version $Version not in the file name: $Path" }
    Write-Host "$Path ($Size bytes, version $Version)"
}

Push-Location (Join-Path $Root 'app')
try {
    cargo build --release --locked -p tempester | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally { Pop-Location }
$Exe = Join-Path $Root 'app/target/release/tempester.exe'
if (-not (Test-Path -LiteralPath $Exe)) { throw "no binary: $Exe" }

# The plugin is an isolated crate (own workspace, lock file and target/); --locked so that a package never resolves a
# dependency the lock file does not name.
$Bundles = $env:TEMPESTER_PLUGIN_BUNDLES
if (-not $Bundles) {
    Push-Location (Join-Path $Root 'app/crates/tempester-plugin')
    try {
        cargo xtask bundle tempester-plugin --release --locked | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'cargo xtask bundle failed' }
    } finally { Pop-Location }
    $Bundles = Join-Path $Root 'app/crates/tempester-plugin/target/bundled'
}
foreach ($Format in 'clap', 'vst3') {
    if (-not (Test-Path -LiteralPath (Join-Path $Bundles "$PluginName.$Format"))) { throw "no $Format bundle in $Bundles" }
}

$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ("tempester-package-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $Stage | Out-Null
try {
    Copy-Item -LiteralPath $Exe -Destination $Stage
    Copy-Item -LiteralPath (Join-Path $Icons 'tempest.ico') -Destination $Stage
    Copy-Item -LiteralPath (Join-Path $Root 'LICENSE') -Destination $Stage
    $StagedPlugins = Join-Path $Stage 'plugins'
    New-Item -ItemType Directory -Path $StagedPlugins | Out-Null
    foreach ($Format in 'clap', 'vst3') {
        Copy-Item -LiteralPath (Join-Path $Bundles "$PluginName.$Format") -Destination $StagedPlugins -Recurse
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'windows/plugins-readme.txt') -Destination (Join-Path $StagedPlugins 'README.txt')

    $Zip = Join-Path $Dist "Tempester-$Version-windows-x64.zip"
    if (Test-Path -LiteralPath $Zip) { Remove-Item -LiteralPath $Zip }
    # ZipFile, not Compress-Archive: Windows PowerShell 5.1's Compress-Archive writes backslashes in the entry names of
    # sub-folders (plugins/), which unzip tools other than Explorer reject.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::CreateFromDirectory($Stage, $Zip, [System.IO.Compression.CompressionLevel]::Optimal, $false)
    $Archive = [System.IO.Compression.ZipFile]::OpenRead($Zip)
    try { $Names = $Archive.Entries | ForEach-Object { $_.FullName } } finally { $Archive.Dispose() }
    foreach ($Needed in 'tempester.exe', 'tempest.ico', 'LICENSE', 'plugins/README.txt', "plugins/$PluginName.clap") {
        if ($Names -notcontains $Needed) { throw "$Needed missing from the zip" }
    }
    if (-not ($Names | Where-Object { $_ -like "plugins/$PluginName.vst3/Contents/*/$PluginName.vst3" })) {
        throw 'the VST3 binary is missing from the zip'
    }
    if ($Names | Where-Object { $_ -like '*\*' }) { throw 'backslash in a zip entry name' }
    Assert-Package $Zip 500000

    if ($Installer) {
        $Makensis = Get-Command makensis -ErrorAction SilentlyContinue
        if (-not $Makensis) { throw 'makensis not found (install NSIS)' }
        $Setup = Join-Path $Dist "Tempester-$Version-windows-x64-setup.exe"
        if (Test-Path -LiteralPath $Setup) { Remove-Item -LiteralPath $Setup }
        # The licence page: the notice (version 3 or any later version) then the GPL text, like licence_page in
        # tools/package_common.sh; written outside the stage so that the zip keeps the verbatim LICENSE.
        $LicencePage = Join-Path ([System.IO.Path]::GetTempPath()) ("tempester-licence-" + [System.Guid]::NewGuid() + ".txt")
        $Notice = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'licence-notice.txt'))
        [System.IO.File]::WriteAllText($LicencePage, $Notice + [System.IO.File]::ReadAllText((Join-Path $Root 'LICENSE')))
        try {
            & $Makensis.Source "/DVERSION=$Version" "/DSOURCE=$Stage" "/DPLUGINS=$StagedPlugins" "/DLICENSE=$LicencePage" "/DOUTFILE=$Setup" (Join-Path $PSScriptRoot 'windows/tempester.nsi') | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }
        } finally { Remove-Item -LiteralPath $LicencePage -ErrorAction SilentlyContinue }
        Assert-Package $Setup 500000
    }

    if ($Smoke) {
        $env:TEMPESTER_FAKE_MIDI = '1'
        $env:TEMPESTER_DATA_DIR = Join-Path $Stage 'smoke-data'
        $Process = Start-Process -FilePath (Join-Path $Stage 'tempester.exe') -PassThru
        try {
            Start-Sleep -Seconds 6
            if ($Process.HasExited) { throw "the app exited with code $($Process.ExitCode) before the window opened" }
            Write-Host 'smoke: the app stayed up for 6 s on the simulated machine'
        } finally {
            if (-not $Process.HasExited) { Stop-Process -Id $Process.Id -Force }
        }
    }
} finally {
    Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue
}
