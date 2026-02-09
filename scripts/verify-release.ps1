param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [Parameter(Mandatory = $true)]
    [string]$Repository,
    [string]$OutputDir = "dist",
    [string]$InstallerName = "ArcticDownloader-setup.exe"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$dist = Join-Path $root $OutputDir
$installerPath = Join-Path $dist $InstallerName
$manifestPath = Join-Path $dist "update.json"

if (-not (Test-Path $installerPath)) {
    throw "Missing installer: $installerPath"
}

if (-not (Test-Path $manifestPath)) {
    throw "Missing manifest: $manifestPath"
}

$manifestRaw = Get-Content $manifestPath -Raw
try {
    $manifest = $manifestRaw | ConvertFrom-Json
} catch {
    throw "update.json is not valid JSON: $($_.Exception.Message)"
}

foreach ($field in @("version", "download_url", "sha256")) {
    $value = $manifest.$field
    if (-not $value -or [string]::IsNullOrWhiteSpace([string]$value)) {
        throw "update.json is missing required field: $field"
    }
}

if ($manifest.version -ne $Version) {
    throw "Manifest version '$($manifest.version)' does not match expected '$Version'"
}

$expectedUrl = "https://github.com/$Repository/releases/download/$Tag/$InstallerName"
if ($manifest.download_url -ne $expectedUrl) {
    throw "Manifest download_url '$($manifest.download_url)' does not match expected '$expectedUrl'"
}

$expectedSha = (Get-FileHash -Path $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$manifestSha = ([string]$manifest.sha256).ToLowerInvariant()
if ($manifestSha -ne $expectedSha) {
    throw "Manifest sha256 '$manifestSha' does not match installer sha256 '$expectedSha'"
}

Write-Host "Release artifacts verified:"
Write-Host "  Installer: $installerPath"
Write-Host "  Manifest:  $manifestPath"
Write-Host "  Version:   $Version"
Write-Host "  Tag:       $Tag"
Write-Host "  SHA256:    $expectedSha"
