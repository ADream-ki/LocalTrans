param(
  [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot),
  [string]$PortableDir = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($PortableDir)) {
  $tauriConfigPath = Join-Path $RepoRoot "src-tauri\tauri.conf.json"
  $tauriConfig = Get-Content $tauriConfigPath -Raw | ConvertFrom-Json
  $PortableDir = Join-Path $RepoRoot ("artifacts\portable\{0}_{1}_x64_portable" -f $tauriConfig.productName, $tauriConfig.version)
}

if (!(Test-Path $PortableDir)) {
  throw "portable directory not found: $PortableDir"
}

$manifestPath = Join-Path $PortableDir "portable-manifest.json"
$exePath = Join-Path $PortableDir "localtrans.exe"
$pluginsDir = Join-Path $PortableDir "resources\loci-plugins"
$mtDir = Join-Path $PortableDir "resources\mt-runtime"

foreach ($path in @($manifestPath, $exePath, $pluginsDir, $mtDir)) {
  if (!(Test-Path $path)) {
    throw "portable package missing required path: $path"
  }
}

Push-Location $PortableDir
try {
  & $exePath workflow-profiles | Out-Host
  if ($LASTEXITCODE -ne 0) {
    throw "portable workflow-profiles check failed"
  }

  & $exePath workflow-apply --profile-id privacy-local-only | Out-Host
  if ($LASTEXITCODE -ne 0) {
    throw "portable workflow-apply check failed"
  }

  & $exePath config-set --key translationEngine --value nllb | Out-Host
  if ($LASTEXITCODE -ne 0) {
    throw "portable config-set check failed"
  }

  $policy = & $exePath loci-governance-snapshot | ConvertFrom-Json
  if ($LASTEXITCODE -ne 0) {
    throw "portable loci-governance-snapshot check failed"
  }
  if ($policy.statusMessage -like "*feature is not enabled*") {
    throw "portable package is missing loci-backend"
  }
} finally {
  Pop-Location
}

Write-Host "Portable package verified: $PortableDir"
