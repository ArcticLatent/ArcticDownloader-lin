param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Repository = $env:GITHUB_REPOSITORY,
    [string]$Tag = "",
    [string]$OutputDir = "dist"
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

$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    throw "cargo.exe not found at $cargo"
}

Write-Host "Building release binary..."
& $cargo build --release
if ($LASTEXITCODE -ne 0) {
    throw "cargo build --release failed"
}

$binary = Join-Path $root "target\release\arctic-downloader.exe"
if (-not (Test-Path $binary)) {
    throw "Expected binary not found at $binary"
}

$isccCandidates = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
)
$iscc = $isccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    throw "Inno Setup compiler (ISCC.exe) not found. Install Inno Setup 6."
}

New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

Write-Host "Compiling installer with Inno Setup..."
& $iscc "/DAppVersion=$Version" "/DSourceExe=$binary" "/O$OutputDir" "/FArcticDownloader-setup" "installer\ArcticDownloader.iss"
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed"
}

$installer = Join-Path $root "$OutputDir\ArcticDownloader-setup.exe"
if (-not (Test-Path $installer)) {
    throw "Installer not found at $installer"
}

$sha = (Get-FileHash -Path $installer -Algorithm SHA256).Hash.ToLowerInvariant()
$downloadUrl = "https://github.com/$Repository/releases/download/$Tag/ArcticDownloader-setup.exe"

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

Write-Host "Installer: $installer"
Write-Host "SHA256: $sha"
Write-Host "Manifest: $manifestPath"

if ($env:GITHUB_OUTPUT) {
    "installer_path=$installer" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "manifest_path=$manifestDistPath" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "sha256=$sha" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "version=$Version" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
}
