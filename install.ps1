<#
.SYNOPSIS
install.ps1 — install the tabstride CLI on Windows from GitHub Releases.

.DESCRIPTION
Downloads the latest (or pinned) tabstride release for Windows x64,
extracts tabstride.exe to a user-local directory, and adds it to PATH.

Usage:
  irm https://raw.githubusercontent.com/Tencent/TabStride/main/install.ps1 | iex

Environment overrides:
  $env:TABSTRIDE_REPO         GitHub owner/repo (default: Tencent/TabStride)
  $env:TABSTRIDE_VERSION      Pin CLI version (default: latest from version.json)
  $env:TABSTRIDE_INSTALL_DIR  Install directory (default: $HOME\.local\bin)
#>

#Requires -Version 5.1

$ErrorActionPreference = "Stop"

$Repo = if ($env:TABSTRIDE_REPO) { $env:TABSTRIDE_REPO } else { "Tencent/TabStride" }
$InstallDir = if ($env:TABSTRIDE_INSTALL_DIR) { $env:TABSTRIDE_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
$GitHub = "https://github.com/${Repo}"

function Write-Log {
    param([string]$Message)
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Die {
    param([string]$Message)
    Write-Host "error: $Message" -ForegroundColor Red
    exit 1
}

# ── Platform / architecture detection ─────────────────────────────────────────

function Get-PlatformTriple {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture

    switch ($arch) {
        "X64"  { $archId = "x64" }
        "Arm64" { $archId = "arm64" }
        default { Write-Die "unsupported architecture: $arch (x64 and ARM64 only)" }
    }

    $windowsArch = switch ($archId) {
        "x64"   { "x86_64-pc-windows-msvc" }
        "arm64" { "aarch64-pc-windows-msvc" }
    }

    return @{
        ArchId      = $archId
        TargetTriple = $windowsArch
    }
}

# ── Version resolution ────────────────────────────────────────────────────────

function Get-LatestVersion {
    $versionJsonUrl = "${GitHub}/releases/latest/download/version.json"
    Write-Log "fetching latest version from ${versionJsonUrl}"

    try {
        $version = (Invoke-RestMethod -Uri $versionJsonUrl).version
        if (-not $version) { Write-Die "could not parse version from version.json" }
        return $version
    }
    catch {
        Write-Die "failed to fetch version.json: $_"
    }
}

# ── PATH helpers ──────────────────────────────────────────────────────────────

function Add-ToUserPath {
    param([string]$Dir)

    $currentUserPath = [Environment]::GetEnvironmentVariable("PATH", "User") -split ";" | Where-Object { $_ }

    if ($currentUserPath -contains $Dir) {
        Write-Log "$Dir is already in your user PATH"
        return
    }

    $newUserPath = ($currentUserPath + $Dir) -join ";"
    [Environment]::SetEnvironmentVariable("PATH", $newUserPath, "User")
    Write-Log "added ${Dir} to user PATH"
}

function Add-ToSessionPath {
    param([string]$Dir)

    $pathEntries = $env:PATH -split ";" | Where-Object { $_ }
    if ($pathEntries -contains $Dir) {
        return
    }

    $env:PATH = "$Dir;$env:PATH"
}

# ── Git Bash (bash environment) PATH helper ──────────────────────────────────

function Add-ToBashProfile {
    param([string]$Dir)

    # Convert Windows path (e.g. C:\Users\foo\.local\bin) to Git-Bash Unix-style (/c/Users/foo/.local/bin)
    $unixPath = $Dir -replace '\\', '/'
    if ($unixPath -match '^([A-Z]):(.*)$') {
        $unixPath = '/' + $matches[1].ToLower() + $matches[2]
    }
    $bashRc = Join-Path $HOME ".bashrc"
    $exportLine = "export PATH=""${unixPath}:`$PATH""  # tabstride CLI"

    if (Test-Path $bashRc) {
        $content = Get-Content $bashRc -Raw -ErrorAction SilentlyContinue
        if ($content -match [regex]::Escape($unixPath)) {
            Write-Log "$unixPath is already in ~/.bashrc"
            return
        }
    }

    Add-Content $bashRc "`n$exportLine" -Encoding ASCII
    Write-Log "added ${unixPath} to ~/.bashrc"
}

# ── Main ──────────────────────────────────────────────────────────────────────

function Main {
    $platform = Get-PlatformTriple

    if ($env:TABSTRIDE_VERSION) {
        $version = $env:TABSTRIDE_VERSION -replace '^v', ''
        Write-Log "using pinned version ${version}"
    }
    else {
        $version = Get-LatestVersion
        Write-Log "latest version is ${version}"
    }

    $tag = "cli-v${version}"
    $archiveName = "tabstride-v${version}-$($platform.TargetTriple).zip"
    $downloadUrl = "${GitHub}/releases/download/${tag}/${archiveName}"

    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    try {
        $archivePath = Join-Path $tempDir $archiveName

        Write-Log "downloading ${downloadUrl}"
        Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -UseBasicParsing -ErrorAction Stop

        Write-Log "extracting ${archiveName}"
        Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force

        if (-not (Test-Path (Join-Path $tempDir "tabstride.exe"))) {
            Write-Die "tabstride.exe not found in archive"
        }

        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        }

        Copy-Item -Path (Join-Path $tempDir "tabstride.exe") -Destination (Join-Path $InstallDir "tabstride.exe") -Force

        Write-Log "installed tabstride to $InstallDir\tabstride.exe"

        # Add to session PATH (current shell)
        Add-ToSessionPath $InstallDir

        # Add to user PATH (persistent, for PowerShell / cmd)
        Add-ToUserPath $InstallDir

        # Add to Git Bash PATH (persistent, for bash-based shells / agents)
        Add-ToBashProfile $InstallDir

        # Verify
        $tabstridePath = Join-Path $InstallDir "tabstride.exe"
        if (Get-Command tabstride -ErrorAction SilentlyContinue) {
            & tabstride --version
        }
        else {
            Write-Log "verify install: & ""$tabstridePath"" --version"
        }

        Write-Log "done"
        Write-Host ""
        Write-Host "Open a new terminal (PowerShell / Git Bash) for PATH changes to take full effect."
    }
    finally {
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }
}

Main
