# Windows Release Pipeline

## Goal

`LocalTrans` should build on GitHub Actions without requiring end users to install LLVM, libclang, or runtime DLLs manually.

## Current pipeline

The Windows CI pipeline is defined in:

- `.github/workflows/build-windows.yml`

It does the following:

1. Runs `validate-windows` on branch pushes, pull requests, and manual dispatches.
2. Checks out the repo with recursive submodules so `submodules/loci-refactor` is available.
3. Installs LLVM on the runner and exports `LIBCLANG_PATH`.
4. Installs frontend dependencies.
5. Builds the frontend explicitly with `npm run build`.
6. Runs `cargo check` both with and without `loci-backend` so branch CI catches host/runtime integration regressions without waiting for release packaging.
7. Verifies the repository-bundled MT runtime in `src-tauri/resources/mt-runtime` by checking `python.exe`, `mt_translate.py`, and Argos package directories explicitly.
8. Runs `release-windows` only for `v*` tags.
9. Builds the Tauri Windows release with `loci-backend` enabled.
10. Packages a portable bundle with runtime DLLs and bundled resources.
11. Runs smoke tests against the built `localtrans.exe`.
12. Runs smoke tests against the extracted portable bundle.
13. Generates `SHA256SUMS.txt` for downloadable artifacts.
14. Uploads build artifacts.
15. Publishes a GitHub Release automatically when the ref is a `v*` tag.

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
- runs `npm run build`
- runs `npm run tauri build -- --features loci-backend --bundles nsis --config src-tauri/tauri.release.conf.json`
- creates the portable package

The local script defaults to `NSIS + portable` because that is the most reliable workstation release path.

If you also want MSI from a local machine and your Windows Installer environment is healthy, run:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\build-release.ps1 -Bundles "nsis,msi"
```

If your workstation policy intermittently blocks `esbuild` child-process creation, build the frontend first and then skip that step in the release script:

```powershell
npm run build
powershell -ExecutionPolicy Bypass -File .\tools\build-release.ps1 -SkipFrontendBuild
```

## Important note

`libclang.dll` is a build-time dependency, not an end-user runtime dependency.

End users who install via the generated `NSIS`/`MSI` package or run the portable zip do not need to install LLVM manually. The portable package already includes the runtime DLLs required by the built desktop app.

## Smoke validation

The CI pipeline now validates the built release executable directly:

- `localtrans.exe workflow-profiles`
- `localtrans.exe workflow-apply --profile-id meeting-low-latency`
- `localtrans.exe config-set --key translationEngine --value nllb`
- `localtrans.exe loci-governance-snapshot`

The CI pipeline also validates the extracted portable bundle directly:

- `artifacts\portable\...\localtrans.exe workflow-profiles`
- `artifacts\portable\...\localtrans.exe workflow-apply --profile-id privacy-local-only`
- `artifacts\portable\...\localtrans.exe config-set --key translationEngine --value nllb`
- `artifacts\portable\...\localtrans.exe loci-governance-snapshot`

This catches packaging regressions where the binary builds but the shipped portable directory is incomplete, or where the release accidentally omits the `loci-backend` feature.
