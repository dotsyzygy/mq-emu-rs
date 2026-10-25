<#
.SYNOPSIS
    Builds a plugin from plugins/ and copies its DLL into MacroQuest's plugins folder.

.DESCRIPTION
    Finds MacroQuest through MQ_ROOT: the environment variable if set, otherwise the
    [env] MQ_ROOT entry in your user-level Cargo config (~/.cargo/config.toml), the
    same place the build uses. MQ_BUILD_CONFIG picks build/bin/<config> (default
    "release"), matching mq-emu-sys.

.EXAMPLE
    ./scripts/deploy-plugin.ps1
    Builds MQ2RustPluginTest in release and copies it.

.EXAMPLE
    ./scripts/deploy-plugin.ps1 -Plugin MQ2Other -DebugBuild
    Builds plugins/MQ2Other in debug and copies it (with its .pdb).
#>
[CmdletBinding()]
param(
    # Folder name under plugins/, which is also the DLL name.
    [string]$Plugin = "MQ2RustPluginTest",
    # Build the debug profile instead of release.
    [switch]$DebugBuild,
    # Copy what was built last time without running cargo.
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

function Find-MqRoot {
    if ($env:MQ_ROOT) { return $env:MQ_ROOT }

    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
    foreach ($name in "config.toml", "config") {
        $config = Join-Path $cargoHome $name
        if (-not (Test-Path $config)) { continue }
        # Matches both `MQ_ROOT = "C:/..."` and `MQ_ROOT = { value = "C:/...", ... }`.
        $match = Select-String -Path $config -Pattern '^\s*MQ_ROOT\s*=\s*(\{[^}]*value\s*=\s*)?"([^"]+)"' | Select-Object -First 1
        if ($match) { return $match.Matches[0].Groups[2].Value }
    }
    return $null
}

$mqRoot = Find-MqRoot
if (-not $mqRoot) {
    throw "MQ_ROOT isn't set. Set it in your environment or under [env] in ~/.cargo/config.toml (see README.md)."
}
$mqConfig = if ($env:MQ_BUILD_CONFIG) { $env:MQ_BUILD_CONFIG } else { "release" }
$destination = Join-Path $mqRoot "build/bin/$mqConfig/plugins"
if (-not (Test-Path $destination)) {
    throw "$destination doesn't exist. Is MQ_ROOT ($mqRoot) a built MacroQuest checkout?"
}

$manifest = Join-Path $repo "plugins/$Plugin/Cargo.toml"
if (-not (Test-Path $manifest)) {
    throw "No plugin at plugins/$Plugin (looked for $manifest)."
}

$buildProfile = if ($DebugBuild) { "debug" } else { "release" }
if (-not $NoBuild) {
    $cargoArgs = @("build", "--manifest-path", $manifest)
    if (-not $DebugBuild) { $cargoArgs += "--release" }
    Write-Host "Building $Plugin ($buildProfile)..."
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed." }
}

$built = Join-Path $repo "target/i686-pc-windows-msvc/$buildProfile"
$dll = Join-Path $built "$Plugin.dll"
if (-not (Test-Path $dll)) {
    throw "$dll not found. Build it first (or drop -NoBuild)."
}

try {
    Copy-Item $dll $destination -Force
    $pdb = Join-Path $built "$Plugin.pdb"
    if (Test-Path $pdb) { Copy-Item $pdb $destination -Force }
}
catch [System.IO.IOException] {
    throw "Couldn't overwrite $Plugin.dll; it's probably loaded. Run '/plugin $Plugin unload' in game, then try again."
}

Write-Host "Copied $Plugin.dll to $destination"
Write-Host "In game: /plugin $Plugin   (then e.g. /rusttest selftest)"
