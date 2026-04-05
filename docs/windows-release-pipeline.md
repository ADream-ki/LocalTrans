# Windows Release Pipeline

## Goal

`LocalTrans` should build on GitHub Actions without requiring end users to install LLVM, libclang, or runtime DLLs manually.

## Current pipeline

The Windows CI pipeline is defined in:

- `.github/workflows/build-windows.yml`

It does the following:

1. Checks out the repo with recursive submodules so `submodules/loci-refactor` is available.
2. Installs LLVM on the runner and exports `LIBCLANG_PATH`.
3. Installs frontend dependencies.
4. Prepares the bundled MT runtime into `src-tauri/resources/mt-runtime`.
5. Builds the Tauri Windows release.
6. Packages a portable bundle with runtime DLLs and bundled resources.
7. Runs smoke tests against the built `localtrans.exe`.
8. Runs smoke tests against the extracted portable bundle.
9. Generates `SHA256SUMS.txt` for downloadable artifacts.
10. Uploads build artifacts.
11. Publishes a GitHub Release automatically when the ref is a `v*` tag.

## Submodule requirement

The pinned `submodules/loci-refactor` entry must use a CI-resolvable URL in `.gitmodules`.

Use a canonical HTTPS GitHub URL such as:

- `https://github.com/decade-afk/Loci.git`

Do not leave workstation-only SSH aliases such as `git@work:...` in `.gitmodules`, otherwise GitHub Actions cannot resolve the host and recursive checkout fails before the build even starts.

## Release artifacts

The workflow uploads:

- `NSIS` installer
- `MSI` installer
- portable zip bundle
- `SHA256SUMS.txt`

The portable zip is produced by:

- `tools/package-portable.ps1`

That package includes:

- `localtrans.exe`
- all runtime `.dll` files from `src-tauri/target/release`
- `resources/loci-plugins`
- `resources/mt-runtime`
- `portable-manifest.json`
- `README-portable.txt`

## Local release build

For a local host build on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\build-release.ps1
```

That script:

- validates `libclang.dll`
- sets `LIBCLANG_PATH`
- runs `npm run tauri build`
- creates the portable package

## Important note

`libclang.dll` is a build-time dependency, not an end-user runtime dependency.

End users who install via the generated `NSIS`/`MSI` package or run the portable zip do not need to install LLVM manually. The portable package already includes the runtime DLLs required by the built desktop app.

## Smoke validation

The CI pipeline now validates the built release executable directly:

- `localtrans.exe workflow-profiles`
- `localtrans.exe workflow-apply --profile-id meeting-low-latency`

The CI pipeline also validates the extracted portable bundle directly:

- `artifacts\portable\...\localtrans.exe workflow-profiles`
- `artifacts\portable\...\localtrans.exe workflow-apply --profile-id privacy-local-only`

This catches packaging regressions where the binary builds but the shipped portable directory is incomplete.
