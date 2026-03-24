use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();

    // Keep portable target/{profile}/localtrans.exe usable without system Python:
    // copy src-tauri/resources/mt-runtime next to the built binary.
    if let Err(err) = copy_mt_runtime_to_target_profile() {
        println!("cargo:warning=failed to copy mt-runtime resources: {err}");
    }
}

fn copy_mt_runtime_to_target_profile() -> io::Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let src = manifest_dir.join("resources").join("mt-runtime");
    if !src.exists() {
        return Ok(());
    }
    let dst = manifest_dir
        .join("target")
        .join(profile)
        .join("resources")
        .join("mt-runtime");
    if dst.exists() {
        fs::remove_dir_all(&dst)?;
    }
    copy_dir_all(&src, &dst)
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(from, to)?;
        }
    }
    Ok(())
}
