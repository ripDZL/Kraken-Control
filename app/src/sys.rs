//! Small Win32 helpers: single instance, process names, tray-agent control,
//! start-with-Windows. Non-Windows builds get inert stubs (demo/screenshots).

#[cfg(any(windows, feature = "check-win"))]
mod imp {
    use std::io;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::System::Threading::{
        CreateMutexW, GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW, SetForegroundWindow, ShowWindow, GWL_EXSTYLE,
        GW_OWNER, SW_RESTORE, WM_CLOSE, WS_EX_TOOLWINDOW,
    };

    use super::{AGENT_CLASS, UI_TITLE};

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Lower-case file name of this executable (e.g. `kraken-control.exe`).
    pub fn exe_name() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
            .unwrap_or_else(|| "kraken-control.exe".into())
    }

    /// Lower-case image file name of a process, if it can be queried.
    pub fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().map(|s| s.to_lowercase())
        }
    }

    pub fn window_pid(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        pid
    }

    pub fn window_process(hwnd: HWND) -> Option<String> {
        match window_pid(hwnd) {
            0 => None,
            pid => process_name(pid),
        }
    }

    fn window_title(hwnd: HWND) -> String {
        unsafe {
            let n = GetWindowTextLengthW(hwnd);
            if n <= 0 {
                return String::new();
            }
            let mut buf = vec![0u16; n as usize + 1];
            let got = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..got.max(0) as usize])
        }
    }

    /// Creates a named mutex; `false` if another process already holds the name.
    /// The handle is intentionally kept for the life of the process.
    pub fn claim_instance(name: &str) -> bool {
        let n = wide(name);
        unsafe {
            let h = CreateMutexW(null(), 0, n.as_ptr());
            if h.is_null() {
                return true; // can't tell; don't block startup
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(h);
                return false;
            }
        }
        true
    }

    fn agent_hwnd() -> HWND {
        let class = wide(AGENT_CLASS);
        unsafe { FindWindowW(class.as_ptr(), null()) }
    }

    pub fn agent_running() -> bool {
        !agent_hwnd().is_null()
    }

    pub fn stop_agent() {
        let h = agent_hwnd();
        if !h.is_null() {
            unsafe { PostMessageW(h, WM_CLOSE, 0, 0) };
        }
    }

    pub fn spawn_self(args: &[&str]) -> io::Result<()> {
        std::process::Command::new(std::env::current_exe()?).args(args).spawn().map(|_| ())
    }

    struct Enum {
        own_exe: String,
        own_pid: u32,
        found: Option<HWND>,
        apps: Vec<String>,
        want_ui: bool,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lp: LPARAM) -> i32 {
        let e = &mut *(lp as *mut Enum);
        if e.want_ui {
            if window_title(hwnd) == UI_TITLE && window_process(hwnd).as_deref() == Some(e.own_exe.as_str()) {
                e.found = Some(hwnd);
                return 0;
            }
            return 1;
        }
        // App windows a user would recognise: visible, titled, unowned, not tool windows.
        if IsWindowVisible(hwnd) == 0
            || !GetWindow(hwnd, GW_OWNER).is_null()
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
            || GetWindowTextLengthW(hwnd) == 0
        {
            return 1;
        }
        let pid = window_pid(hwnd);
        if pid != e.own_pid {
            if let Some(name) = process_name(pid) {
                if name != e.own_exe && !e.apps.contains(&name) {
                    e.apps.push(name);
                }
            }
        }
        1
    }

    fn enumerate(want_ui: bool) -> Enum {
        let mut e = Enum { own_exe: exe_name(), own_pid: unsafe { GetCurrentProcessId() }, found: None, apps: Vec::new(), want_ui };
        unsafe { EnumWindows(Some(enum_proc), &mut e as *mut Enum as LPARAM) };
        e
    }

    /// Main window of a running Kraken Control UI (title + our exe name).
    pub fn find_ui_window() -> Option<isize> {
        enumerate(true).found.map(|h| h as isize)
    }

    /// Explorer "Save as" (`save`) or "Open" dialog for profile files, modal to
    /// the Kraken window. Blocks until closed: call it from a worker thread.
    pub fn file_dialog(save: bool, default_name: &str) -> Option<std::path::PathBuf> {
        use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        use windows_sys::Win32::UI::Controls::Dialogs::{
            GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT,
            OFN_PATHMUSTEXIST, OPENFILENAMEW,
        };
        // Shell extensions in the dialog expect COM on this thread.
        unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as _) };
        let filter: Vec<u16> = "Kraken Control profile (*.json)\0*.json\0All files (*.*)\0*.*\0\0".encode_utf16().collect();
        let ext = wide("json");
        let mut buf = [0u16; 1024];
        for (d, c) in buf.iter_mut().zip(default_name.encode_utf16().take(1000)) {
            *d = c;
        }
        let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = find_ui_window().map_or(null_mut(), |h| h as HWND);
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = buf.as_mut_ptr();
        ofn.nMaxFile = buf.len() as u32;
        ofn.lpstrDefExt = ext.as_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST };
        // SAFETY: every pointer in `ofn` outlives the call; `buf` has nMaxFile elements.
        let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
        if ok == 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len])))
    }

    pub fn bring_to_front(hwnd: isize) {
        let h = hwnd as HWND;
        unsafe {
            if IsIconic(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            }
            SetForegroundWindow(h);
        }
    }

    /// Exe names of apps with a visible top-level window, sorted.
    pub fn running_apps() -> Vec<String> {
        let mut apps = enumerate(false).apps;
        apps.sort();
        apps
    }

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const RUN_VALUE: &str = "KrakenControl";
    /// Names used before the rename to Kraken Control (v0.3 and earlier).
    const LEGACY_RUN_VALUE: &str = "DeerControl";
    const LEGACY_AGENT_CLASS: &str = "DeerControlAgent";

    fn run_command() -> io::Result<String> {
        Ok(format!("\"{}\" --agent", std::env::current_exe()?.display()))
    }

    /// True if the Run entry exists and points at this executable.
    pub fn autostart_enabled() -> bool {
        let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc != 0 {
            return false;
        }
        let len = (size as usize / 2).saturating_sub(1);
        let cmd = String::from_utf16_lossy(&buf[..len]);
        run_command().is_ok_and(|want| cmd.eq_ignore_ascii_case(&want))
    }

    fn run_value_exists(name: &str) -> bool {
        let (key, value) = (wide(RUN_KEY), wide(name));
        let rc = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), null_mut())
        };
        rc == 0
    }

    /// One-time cleanup after the rename: closes an old "Deer Control" tray
    /// agent and moves its start-with-Windows entry. Returns notes for the log.
    pub fn migrate_legacy() -> Vec<String> {
        let mut notes = Vec::new();
        let class = wide(LEGACY_AGENT_CLASS);
        let old = unsafe { FindWindowW(class.as_ptr(), null()) };
        if !old.is_null() {
            unsafe { PostMessageW(old, WM_CLOSE, 0, 0) };
            std::thread::sleep(std::time::Duration::from_millis(500)); // let it release its files
            notes.push("closed old Deer Control tray agent".to_string());
        }
        if run_value_exists(LEGACY_RUN_VALUE) {
            let value = (wide(RUN_KEY), wide(LEGACY_RUN_VALUE));
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, value.0.as_ptr(), value.1.as_ptr()) };
            let res = set_autostart(true);
            notes.push(format!("moved start-with-Windows entry to Kraken Control: {res:?}"));
        }
        notes
    }

    pub fn set_autostart(on: bool) -> io::Result<()> {
        let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
        let rc = unsafe {
            if on {
                let data = wide(&run_command()?);
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            } else {
                match RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) {
                    2 => 0, // ERROR_FILE_NOT_FOUND: already off
                    rc => rc,
                }
            }
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc as i32))
        }
    }
}

#[cfg(not(any(windows, feature = "check-win")))]
mod imp {
    use std::io;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "Windows only")
    }
    pub fn exe_name() -> String {
        "kraken-control".into()
    }
    pub fn migrate_legacy() -> Vec<String> {
        Vec::new()
    }
    pub fn claim_instance(_: &str) -> bool {
        true
    }
    pub fn agent_running() -> bool {
        false
    }
    pub fn stop_agent() {}
    pub fn spawn_self(_: &[&str]) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn find_ui_window() -> Option<isize> {
        None
    }
    /// Demo / screenshots: a fixed file in the settings folder instead of a dialog.
    pub fn file_dialog(_save: bool, _default_name: &str) -> Option<std::path::PathBuf> {
        Some(crate::store::data_dir().join("kraken-profile.json"))
    }
    pub fn bring_to_front(_: isize) {}
    pub fn running_apps() -> Vec<String> {
        vec!["example-game.exe".into(), "discord.exe".into()]
    }
    pub fn autostart_enabled() -> bool {
        false
    }
    pub fn set_autostart(_: bool) -> io::Result<()> {
        Err(unsupported())
    }
}

/// Window class of the tray agent's hidden window (also how it is found).
pub const AGENT_CLASS: &str = "KrakenControlAgent";
/// Title of the UI window (single-instance focus).
pub const UI_TITLE: &str = "Kraken Control";

#[allow(unused_imports)]
pub use imp::*;
