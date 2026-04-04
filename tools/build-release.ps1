param(
  [switch]$SkipDockerPrep
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$llvmBin = Join-Path $repoRoot "tools\llvm-portable\clang+llvm-22.1.1-x86_64-pc-windows-msvc\bin"
$libclang = Join-Path $llvmBin "libclang.dll"

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

Push-Location $repoRoot
try {
  npm.cmd run tauri build
  if ($LASTEXITCODE -ne 0) {
    throw "Host Tauri release build failed."
  }

  & (Join-Path $PSScriptRoot "package-portable.ps1") -RepoRoot $repoRoot
  if ($LASTEXITCODE -ne 0) {
    throw "Portable packaging failed."
  }
} finally {
  Pop-Location
}

Write-Host "Release build completed."
