param(
    [string]$Version,
    [string]$Repository = "ArcticLatent/Arctic-Helper",
    [string]$AssetName = "Arctic-ComfyUI-Helper.exe",
    [string]$OutputDir = "dist",
    [switch]$SkipClean
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found in PATH."
    }
}

function Resolve-CargoExe {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        return "cargo"
    }
    $fallback = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $fallback) {
        return $fallback
    }
    throw "Cargo not found. Install Rust or add cargo to PATH."
}

function Update-CargoVersion([string]$Path, [string]$NewVersion) {
    $raw = Get-Content $Path -Raw
    $updated = [regex]::Replace($raw, '(?m)^version\s*=\s*".*?"\s*$', "version = `"$NewVersion`"", 1)
    if ($updated -eq $raw) {
        throw "Could not update version in $Path"
    }
    Set-Content -Path $Path -Value $updated -Encoding utf8
}

function Prompt-ReleaseNotes {
    Write-Host ""
    Write-Host "Paste release notes. End with a single line containing END"
    $lines = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $line = Read-Host
        if ($line -eq "END") {
            break
        }
        $lines.Add($line)
    }
    $text = ($lines -join [Environment]::NewLine).Trim()
    if ([string]::IsNullOrWhiteSpace($text)) {
        return "Release v$Version"
    }
    return $text
}

if (-not $Version) {
    $Version = Read-Host "Release version (example: 0.1.1)"
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version must be semantic version format: x.y.z"
}

$notes = Prompt-ReleaseNotes
$tag = "v$Version"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

$cargo = Resolve-CargoExe
Require-Command "gh"

Write-Host "Checking GitHub auth..."
& gh auth status | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "GitHub CLI is not authenticated. Run: gh auth login"
}

$tauriCargo = Join-Path $root "src-tauri\Cargo.toml"
$tauriConf = Join-Path $root "src-tauri\tauri.conf.json"

Write-Host "Updating versions to $Version ..."
Update-CargoVersion -Path $tauriCargo -NewVersion $Version
$conf = Get-Content $tauriConf -Raw | ConvertFrom-Json
$conf.version = $Version
$conf | ConvertTo-Json -Depth 20 | Set-Content -Path $tauriConf -Encoding utf8

if (-not $SkipClean) {
    Write-Host "Running clean build..."
    & $cargo clean --manifest-path .\src-tauri\Cargo.toml
    if ($LASTEXITCODE -ne 0) {
        throw "cargo clean failed"
    }
}

Write-Host "Building release binary (tauri, no bundle)..."
& $cargo tauri build --no-bundle
if ($LASTEXITCODE -ne 0) {
    throw "cargo tauri build --no-bundle failed"
}

$binary = Join-Path $root "src-tauri\target\release\$AssetName"
if (-not (Test-Path $binary)) {
    throw "Release binary not found: $binary"
}

$dist = Join-Path $root $OutputDir
New-Item -ItemType Directory -Path $dist -Force | Out-Null

$assetPath = Join-Path $dist $AssetName
Copy-Item -Path $binary -Destination $assetPath -Force

$sha = (Get-FileHash -Path $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
$shaPath = Join-Path $dist "$AssetName.sha256"
"$sha *$AssetName" | Set-Content -Path $shaPath -Encoding ascii

$downloadUrl = "https://github.com/$Repository/releases/download/$tag/$AssetName"
$updateManifest = [ordered]@{
    version      = $Version
    download_url = $downloadUrl
    sha256       = $sha
    notes        = $notes
}
$updatePath = Join-Path $dist "update.json"
$updateManifest | ConvertTo-Json -Depth 10 | Set-Content -Path $updatePath -Encoding utf8

$notesPath = Join-Path $dist "release-notes-$tag.md"
$notes | Set-Content -Path $notesPath -Encoding utf8

Write-Host "Publishing GitHub release $tag to $Repository ..."
& gh release view $tag --repo $Repository | Out-Null
if ($LASTEXITCODE -eq 0) {
    & gh release edit $tag --repo $Repository --title $tag --notes-file $notesPath
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to edit existing release $tag"
    }
    & gh release upload $tag $assetPath $shaPath $updatePath --repo $Repository --clobber
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to upload release artifacts"
    }
} else {
    & gh release create $tag $assetPath $shaPath $updatePath --repo $Repository --title $tag --notes-file $notesPath
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create release $tag"
    }
}

Write-Host ""
Write-Host "Release complete:"
Write-Host "  Repo:      $Repository"
Write-Host "  Tag:       $tag"
Write-Host "  Asset:     $assetPath"
Write-Host "  SHA256:    $sha"
Write-Host "  update.json: $updatePath"
