#[cfg(target_os = "windows")]
mod windows_impl {
    use std::fs;
    use std::ffi::OsStr;
    use std::iter;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    pub struct TempPathGuard {
        added_dir: Option<String>,
        marker_file: Option<PathBuf>,
    }

    impl TempPathGuard {
        pub fn register_current_exe_dir() -> Self {
            let exe_path = std::env::current_exe().ok();
            let exe_dir = exe_path
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .map(|p| p.display().to_string());
            let Some(exe_dir) = exe_dir else {
                return Self {
                    added_dir: None,
                    marker_file: None,
                };
            };
            let marker_file = match exe_path.as_ref() {
                Some(p) => write_active_marker(p.as_path()),
                None => None,
            };
            let _ = install_powershell_profile_shim();
            let _ = install_cmd_autorun_shim();

            if add_dir_to_user_path(&exe_dir).is_ok() {
                return Self {
                    added_dir: Some(exe_dir),
                    marker_file,
                };
            }

            Self {
                added_dir: None,
                marker_file,
            }
        }
    }

    impl Drop for TempPathGuard {
        fn drop(&mut self) {
            if let Some(dir) = self.added_dir.as_deref() {
                let _ = remove_dir_from_user_path(dir);
            }
            if let Some(marker) = self.marker_file.as_ref() {
                let _ = fs::remove_file(marker);
            }
        }
    }

    fn open_env_key(flags: u32) -> Result<RegKey, std::io::Error> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        hkcu.open_subkey_with_flags("Environment", flags)
    }

    fn normalize_path(p: &str) -> String {
        p.trim()
            .trim_matches('"')
            .trim_end_matches('\\')
            .trim_end_matches('/')
            .to_ascii_lowercase()
    }

    fn split_path(path: &str) -> Vec<String> {
        path.split(';')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()
    }

    fn join_path(items: &[String]) -> String {
        items.join(";")
    }

    fn get_user_path() -> Result<String, std::io::Error> {
        let key = open_env_key(KEY_READ)?;
        match key.get_value::<String, _>("Path") {
            Ok(v) => Ok(v),
            Err(_) => Ok(String::new()),
        }
    }

    fn set_user_path(path: &str) -> Result<(), std::io::Error> {
        let key = open_env_key(KEY_WRITE)?;
        key.set_value("Path", &path)?;
        broadcast_env_change();
        Ok(())
    }

    fn add_dir_to_user_path(dir: &str) -> Result<(), std::io::Error> {
        let current = get_user_path()?;
        let mut items = split_path(&current);
        let needle = normalize_path(dir);
        if items.iter().any(|x| normalize_path(x) == needle) {
            return Ok(());
        }
        items.push(dir.to_string());
        set_user_path(&join_path(&items))
    }

    fn remove_dir_from_user_path(dir: &str) -> Result<(), std::io::Error> {
        let current = get_user_path()?;
        let needle = normalize_path(dir);
        let filtered: Vec<String> = split_path(&current)
            .into_iter()
            .filter(|x| normalize_path(x) != needle)
            .collect();
        set_user_path(&join_path(&filtered))
    }

    fn broadcast_env_change() {
        let wide: Vec<u16> = OsStr::new("Environment")
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM::default(),
                wide.as_ptr() as LPARAM,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            );
        }
    }

    fn localtrans_runtime_dir() -> Option<PathBuf> {
        let home = std::env::var_os("USERPROFILE")?;
        let dir = PathBuf::from(home).join(".localtrans").join("runtime");
        if fs::create_dir_all(&dir).is_err() {
            return None;
        }
        Some(dir)
    }

    fn marker_file_path() -> Option<PathBuf> {
        Some(localtrans_runtime_dir()?.join("active_exe_path.txt"))
    }

    fn write_active_marker(exe_path: &Path) -> Option<PathBuf> {
        let marker = marker_file_path()?;
        if fs::write(&marker, exe_path.display().to_string()).is_ok() {
            Some(marker)
        } else {
            None
        }
    }

    fn install_powershell_profile_shim() -> Result<(), std::io::Error> {
        let home =
            std::env::var("USERPROFILE").map_err(|_| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let profile_paths = [
            PathBuf::from(&home)
                .join("Documents")
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
            PathBuf::from(&home)
                .join("Documents")
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ];
        for p in profile_paths {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)?;
            }
            let existing = fs::read_to_string(&p).unwrap_or_default();
            if existing.contains("# >>> localtrans shim >>>") {
                continue;
            }
            let shim = r#"
# >>> localtrans shim >>>
function localtrans {
  param([Parameter(ValueFromRemainingArguments = $true)] $Args)
  $marker = Join-Path $env:USERPROFILE ".localtrans\runtime\active_exe_path.txt"
  if (Test-Path $marker) {
    $exe = (Get-Content $marker -Raw).Trim()
    if ($exe -and (Test-Path $exe)) {
      & $exe @Args
      return
    }
  }
  $cmd = Get-Command localtrans.exe -CommandType Application -ErrorAction SilentlyContinue
  if ($cmd) { & $cmd.Source @Args; return }
  Write-Error "localtrans executable not found."
}
Set-Alias localtrans.exe localtrans -Scope Global
# <<< localtrans shim <<<
"#;
            let mut merged = existing;
            if !merged.ends_with('\n') {
                merged.push('\n');
            }
            merged.push_str(shim);
            fs::write(&p, merged)?;
        }
        Ok(())
    }

    fn install_cmd_autorun_shim() -> Result<(), std::io::Error> {
        let runtime_dir = localtrans_runtime_dir()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let macro_bat = runtime_dir.join("localtrans-doskey.cmd");
        let script = r#"@echo off
set "LT_MARKER=%USERPROFILE%\.localtrans\runtime\active_exe_path.txt"
if exist "%LT_MARKER%" (
  set /p LT_EXE=<"%LT_MARKER%"
  if not "%LT_EXE%"=="" (
    doskey localtrans="%LT_EXE%" $*
    doskey localtrans.exe="%LT_EXE%" $*
  )
)
"#;
        fs::write(&macro_bat, script)?;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (cmd_key, _) = hkcu.create_subkey("Software\\Microsoft\\Command Processor")?;
        let existing: String = cmd_key.get_value("AutoRun").unwrap_or_default();
        let call_stmt = format!("\"{}\"", macro_bat.display());
        if !existing.contains(&call_stmt) {
            let new_value = if existing.trim().is_empty() {
                call_stmt
            } else {
                format!("{existing} & {call_stmt}")
            };
            cmd_key.set_value("AutoRun", &new_value)?;
        }
        Ok(())
    }

    pub fn register_for_gui_mode() -> TempPathGuard {
        TempPathGuard::register_current_exe_dir()
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::register_for_gui_mode;

#[cfg(not(target_os = "windows"))]
pub struct TempPathGuard;

#[cfg(not(target_os = "windows"))]
pub fn register_for_gui_mode() -> TempPathGuard {
    TempPathGuard
}
