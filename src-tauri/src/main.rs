#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    suppress_windows_crash_dialogs();
    localtrans_lib::run()
}

#[cfg(target_os = "windows")]
fn suppress_windows_crash_dialogs() {
    // Avoid native crash popup dialogs in packaged builds. We still log failures
    // through tracing/panic hooks and let the process exit.
    const SEM_FAILCRITICALERRORS: u32 = 0x0001;
    const SEM_NOGPFAULTERRORBOX: u32 = 0x0002;
    const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;

    unsafe extern "system" {
        fn SetErrorMode(uMode: u32) -> u32;
    }

    let mode = SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX;
    unsafe {
        let _ = SetErrorMode(mode);
    }
}
