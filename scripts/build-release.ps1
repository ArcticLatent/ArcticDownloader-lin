param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Repository = "ArcticLatent/Arctic-Helper",
    [string]$Tag = "",
    [string]$OutputDir = "dist",
    [string]$AssetName = "Arctic-ComfyUI-Helper.exe"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $Tag) {
    $Tag = "v$Version"
}

if (-not $Repository) {
    throw "Repository is required (for download URL generation)."
}

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

$cargo = "cargo"
$tauriManifest = Join-Path $root "src-tauri\Cargo.toml"
if (-not (Test-Path $tauriManifest)) {
    throw "Missing Tauri manifest at $tauriManifest"
}

Write-Host "Building release binary..."
& $cargo build --release --manifest-path $tauriManifest
if ($LASTEXITCODE -ne 0) {
    throw "cargo build --release --manifest-path src-tauri/Cargo.toml failed"
}

$binary = Join-Path $root "src-tauri\target\release\$AssetName"
if (-not (Test-Path $binary)) {
    throw "Expected binary not found at $binary"
}

$distDir = Join-Path $root $OutputDir
New-Item -ItemType Directory -Path $distDir -Force | Out-Null

$assetPath = Join-Path $distDir $AssetName
Copy-Item -Path $binary -Destination $assetPath -Force
if (-not (Test-Path $assetPath)) {
    throw "Release asset not found at $assetPath"
}

$sha = (Get-FileHash -Path $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
$downloadUrl = "https://github.com/$Repository/releases/download/$Tag/$AssetName"

$manifest = [ordered]@{
    version      = $Version
    download_url = $downloadUrl
    sha256       = $sha
    notes        = "Optional release notes"
}

$manifestJson = $manifest | ConvertTo-Json -Depth 4
$manifestPath = Join-Path $root "update.json"
$manifestDistPath = Join-Path $root "$OutputDir\update.json"
$manifestJson | Set-Content -Path $manifestPath -Encoding utf8
$manifestJson | Set-Content -Path $manifestDistPath -Encoding utf8

Write-Host "Asset: $assetPath"
Write-Host "SHA256: $sha"
Write-Host "Manifest: $manifestPath"

if ($env:GITHUB_OUTPUT) {
    "asset_path=$assetPath" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "manifest_path=$manifestDistPath" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "sha256=$sha" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "version=$Version" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
}
