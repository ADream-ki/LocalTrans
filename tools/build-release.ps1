param(
  [switch]$SkipDockerPrep,
  [string]$Bundles = "nsis",
  [switch]$SkipFrontendBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$llvmBin = Join-Path $repoRoot "tools\llvm-portable\clang+llvm-22.1.1-x86_64-pc-windows-msvc\bin"
$libclang = Join-Path $llvmBin "libclang.dll"

function Invoke-StepWithRetry {
  param(
    [scriptblock]$Action,
    [string]$Description,
    [int]$MaxAttempts = 3,
    [int]$DelaySeconds = 3
  )

  for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    & $Action
    if ($LASTEXITCODE -eq 0) {
      return
    }

    if ($attempt -ge $MaxAttempts) {
      throw "$Description failed after $MaxAttempts attempts."
    }

    Write-Warning "$Description failed on attempt $attempt/$MaxAttempts. Retrying in $DelaySeconds seconds..."
    Start-Sleep -Seconds $DelaySeconds
  }
}

if (-not $SkipDockerPrep) {
  & (Join-Path $PSScriptRoot "docker-build.ps1")
  if ($LASTEXITCODE -ne 0) {
    throw "Docker preparation failed."
  }
}

if (-not (Test-Path $libclang)) {
  throw "libclang.dll not found: $libclang"
}

$env:LIBCLANG_PATH = $llvmBin

Write-Host "Using LIBCLANG_PATH=$env:LIBCLANG_PATH"
Write-Host "Building Windows Tauri release on host..."
Write-Host "Requested bundles: $Bundles"

Push-Location $repoRoot
try {
  if ($SkipFrontendBuild) {
    $distDir = Join-Path $repoRoot "dist"
    if (-not (Test-Path $distDir)) {
      throw "dist directory not found. Run 'npm run build' first or omit -SkipFrontendBuild."
    }
    Write-Host "Skipping frontend build because -SkipFrontendBuild was specified."
  } else {
    Invoke-StepWithRetry -Description "Frontend build" -Action { npm.cmd run build }
  }

  Invoke-StepWithRetry -Description "Host Tauri release build" -MaxAttempts 2 -DelaySeconds 5 -Action {
    npm.cmd run tauri build -- --features loci-backend --bundles $Bundles --config src-tauri/tauri.release.conf.json
  }

  & (Join-Path $PSScriptRoot "package-portable.ps1") -RepoRoot $repoRoot
  if ($LASTEXITCODE -ne 0) {
    throw "Portable packaging failed."
  }
} finally {
  Pop-Location
}

Write-Host "Release build completed."
