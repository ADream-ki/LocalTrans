param(
  [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot),
  [string]$ReleaseDir = "",
  [string]$OutputDir = "",
  [switch]$SkipZip
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($ReleaseDir)) {
  $ReleaseDir = Join-Path $RepoRoot "src-tauri\target\release"
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $RepoRoot "artifacts\portable"
}

$tauriConfigPath = Join-Path $RepoRoot "src-tauri\tauri.conf.json"
if (!(Test-Path $tauriConfigPath)) {
  throw "tauri config not found: $tauriConfigPath"
}

$tauriConfig = Get-Content $tauriConfigPath -Raw | ConvertFrom-Json
$productName = [string]$tauriConfig.productName
$version = [string]$tauriConfig.version

if ([string]::IsNullOrWhiteSpace($productName) -or [string]::IsNullOrWhiteSpace($version)) {
  throw "productName/version missing in tauri.conf.json"
}

$exePath = Join-Path $ReleaseDir "$productName.exe"
if (!(Test-Path $exePath)) {
  throw "release executable not found: $exePath"
}

$stagingRoot = Join-Path $OutputDir "${productName}_${version}_x64_portable"
$resourcesSource = Join-Path $RepoRoot "src-tauri\resources"
$zipPath = Join-Path $OutputDir "${productName}_${version}_x64_portable.zip"

if (Test-Path $stagingRoot) {
  Remove-Item -LiteralPath $stagingRoot -Recurse -Force
}
if (Test-Path $zipPath) {
  Remove-Item -LiteralPath $zipPath -Force
}

New-Item -ItemType Directory -Path $stagingRoot -Force | Out-Null

Copy-Item -LiteralPath $exePath -Destination $stagingRoot -Force

$runtimeDlls = Get-ChildItem -Path $ReleaseDir -Filter *.dll -File
foreach ($dll in $runtimeDlls) {
  Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $stagingRoot $dll.Name) -Force
}

if (Test-Path $resourcesSource) {
  Copy-Item -LiteralPath $resourcesSource -Destination (Join-Path $stagingRoot "resources") -Recurse -Force
}

$manifest = [ordered]@{
  productName = $productName
  version = $version
  executable = "$productName.exe"
  runtimeDlls = @($runtimeDlls | ForEach-Object { $_.Name })
  includes = @(
    "resources\loci-plugins",
    "resources\mt-runtime"
  )
  generatedAtUtc = [DateTime]::UtcNow.ToString("o")
}

$manifestPath = Join-Path $stagingRoot "portable-manifest.json"
$manifest | ConvertTo-Json -Depth 4 | Set-Content -Path $manifestPath -Encoding UTF8

if (-not $SkipZip) {
  New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
  Compress-Archive -Path (Join-Path $stagingRoot "*") -DestinationPath $zipPath -CompressionLevel Optimal
}

Write-Host "Portable staging directory: $stagingRoot"
if (-not $SkipZip) {
  Write-Host "Portable zip: $zipPath"
}
