//! Background tray agent (`kraken-control --agent`). No window, no GL, no
//! persistent device handle: a hidden message window, a tray icon with a
//! menu, and a foreground-window hook that switches onboard slots per app.
//! The keyboard is opened only for each switch (under the cross-process lock).

#[cfg(any(windows, feature = "check-win"))]
mod imp {
    use std::cell::RefCell;
    use std::io;
    use std::ptr::{null, null_mut};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::SystemTime;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
        DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, KillTimer, PostMessageW, PostQuitMessage,
        RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetTimer, TrackPopupMenu, TranslateMessage,
        EVENT_SYSTEM_FOREGROUND, HICON, LR_DEFAULTCOLOR, MF_CHECKED, MF_DEFAULT, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG,
        OBJID_WINDOW, SM_CXSMICON, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
    };

    use crate::store::AgentConfig;
    use crate::{device, log, sys};

    const WM_TRAY: u32 = WM_APP + 1;
    /// From the worker: wparam = current slot + 1 (0 = unknown).
    const WM_SLOT: u32 = WM_APP + 2;
    const TIMER_FG: usize = 1;
    /// Alt-Tab produces bursts of foreground events; act on the one that sticks.
    const FG_DEBOUNCE_MS: u32 = 300;
    /// Menu queries must not stall the tray for long.
    const MENU_LOCK_MS: u32 = 300;
    const SWITCH_LOCK_MS: u32 = 5000;

    const ID_OPEN: usize = 1;
    const ID_SLOT0: usize = 10;
    const ID_AUTO: usize = 20;
    const ID_AUTOSTART: usize = 21;
    const ID_EXIT: usize = 30;

    struct Job {
        slot: u8,
        why: String,
    }

    struct State {
        hwnd: HWND,
        icon: HICON,
        taskbar_created: u32,
        jobs: Sender<Job>,
        cfg: AgentConfig,
        cfg_stamp: Option<SystemTime>,
        /// Latest foreground window, acted on after the debounce timer.
        fg: HWND,
        last_exe: String,
        own_exe: String,
        slot: Option<u8>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    /// Short, non-reentrant access. Modal calls (menus) must happen outside.
    fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
        STATE.with(|cell| cell.try_borrow_mut().ok().and_then(|mut s| s.as_mut().map(f)))
    }

    pub fn run() -> i32 {
        if !sys::claim_instance("Local\\KrakenControlAgent") {
            return 0; // already running
        }
        unsafe {
            // Crisp tray icon / menu at any scale (fails harmlessly on old builds).
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let hinst = GetModuleHandleW(null());
            let class = sys::wide(sys::AGENT_CLASS);
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
            };
            if RegisterClassW(&wc) == 0 {
                log::line(format!("agent: RegisterClass failed: {}", io::Error::last_os_error()));
                return 1;
            }
            let title = sys::wide("Kraken Control Agent");
            // Top-level (never shown) rather than message-only: only top-level
            // windows receive the TaskbarCreated broadcast after Explorer restarts.
            let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());
            if hwnd.is_null() {
                log::line(format!("agent: CreateWindow failed: {}", io::Error::last_os_error()));
                return 1;
            }
            let (tx, rx) = mpsc::channel();
            let worker_hwnd = hwnd as isize;
            std::thread::Builder::new().name("agent-worker".into()).spawn(move || worker(rx, worker_hwnd)).ok();

            let cfg = AgentConfig::load();
            STATE.with(|cell| {
                *cell.borrow_mut() = Some(State {
                    hwnd,
                    icon: make_icon(),
                    taskbar_created: RegisterWindowMessageW(sys::wide("TaskbarCreated").as_ptr()),
                    jobs: tx,
                    cfg,
                    cfg_stamp: AgentConfig::stamp(),
                    fg: null_mut(),
                    last_exe: String::new(),
                    own_exe: sys::exe_name(),
                    slot: None,
                })
            });
            with_state(|s| tray(s, NIM_ADD));
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if !hook.is_null() {
                UnhookWinEvent(hook);
            }
            if let Some(Some(icon)) = STATE.with(|cell| cell.borrow_mut().take().map(|s| (!s.icon.is_null()).then_some(s.icon))) {
                DestroyIcon(icon);
            }
        }
        log::line("agent exit");
        0
    }

    /// Opens the keyboard, switches to `job.slot` if needed. Requests are
    /// coalesced: only the latest target matters.
    fn worker(rx: Receiver<Job>, hwnd: isize) {
        while let Ok(mut job) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }
            let res = device::one_shot(SWITCH_LOCK_MS, |s| {
                let st = s.slot_state()?;
                if job.slot >= st.count {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("slot {} does not exist (keyboard has {})", job.slot + 1, st.count),
                    ));
                }
                if st.current != job.slot {
                    s.select_slot(job.slot, st.count)?;
                    log::line(format!("agent: {} -> slot {}", job.why, job.slot + 1));
                }
                Ok(job.slot)
            });
            match res {
                Ok(slot) => unsafe {
                    PostMessageW(hwnd as HWND, WM_SLOT, slot as usize + 1, 0);
                },
                Err(e) => log::line(format!("agent: switch for {} failed: {e}", job.why)),
            }
        }
    }

    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, hwnd: HWND, object: i32, _: i32, _: u32, _: u32) {
        if object != OBJID_WINDOW || hwnd.is_null() {
            return;
        }
        with_state(|s| {
            s.fg = hwnd;
            SetTimer(s.hwnd, TIMER_FG, FG_DEBOUNCE_MS, None);
        });
    }

    fn reload_config(s: &mut State) {
        let stamp = AgentConfig::stamp();
        if stamp != s.cfg_stamp {
            s.cfg = AgentConfig::load();
            s.cfg_stamp = stamp;
            s.last_exe.clear(); // re-evaluate the current app under new rules
        }
    }

    /// Debounced foreground change: pick the slot for the app now in front.
    fn foreground_settled(s: &mut State) {
        unsafe { KillTimer(s.hwnd, TIMER_FG) };
        reload_config(s);
        if !s.cfg.auto_switch {
            return;
        }
        let Some(exe) = sys::window_process(s.fg) else { return };
        if exe == s.last_exe {
            return;
        }
        s.last_exe = exe.clone();
        if exe == s.own_exe {
            return; // the Kraken Control UI itself never triggers a switch
        }
        if let Some(slot) = s.cfg.target_for(&exe) {
            let _ = s.jobs.send(Job { slot, why: exe });
        }
    }

    fn tip_text(slot: Option<u8>) -> String {
        match slot {
            Some(i) => format!("Kraken Control — slot {}", i + 1),
            None => "Kraken Control".into(),
        }
    }

    fn tray(s: &State, op: u32) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = s.hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY;
            nid.hIcon = s.icon;
            let tip: Vec<u16> = tip_text(s.slot).encode_utf16().take(nid.szTip.len() - 1).collect();
            nid.szTip[..tip.len()].copy_from_slice(&tip);
            Shell_NotifyIconW(op, &nid);
        }
    }

    fn open_ui() {
        match sys::find_ui_window() {
            Some(w) => sys::bring_to_front(w),
            None => {
                if let Err(e) = sys::spawn_self(&[]) {
                    log::line(format!("agent: could not start UI: {e}"));
                }
            }
        }
    }

    fn show_menu(hwnd: HWND) {
        // Gather everything first: the menu loop below re-enters wndproc.
        let auto = with_state(|s| {
            reload_config(s);
            s.cfg.auto_switch
        })
        .unwrap_or(false);
        let slots = device::one_shot(MENU_LOCK_MS, |s| s.slot_state()).ok();
        let autostart = sys::autostart_enabled();
        if let Some(st) = &slots {
            with_state(|s| s.slot = Some(st.current));
        }
        let chosen = unsafe {
            let menu = CreatePopupMenu();
            let add = |flags: u32, id: usize, text: &str| {
                let w = sys::wide(text);
                AppendMenuW(menu, flags, id, w.as_ptr());
            };
            add(MF_STRING | MF_DEFAULT, ID_OPEN, "Open Kraken Control");
            add(MF_SEPARATOR, 0, "");
            match &slots {
                Some(st) => {
                    for i in 0..st.count {
                        let check = if i == st.current { MF_CHECKED } else { 0 };
                        add(MF_STRING | check, ID_SLOT0 + i as usize, &format!("Slot {}", i + 1));
                    }
                }
                None => add(MF_STRING | MF_GRAYED, 0, "Keyboard not available"),
            }
            add(MF_SEPARATOR, 0, "");
            add(MF_STRING | if auto { MF_CHECKED } else { 0 }, ID_AUTO, "Switch slot by active app");
            add(MF_STRING | if autostart { MF_CHECKED } else { 0 }, ID_AUTOSTART, "Start with Windows");
            add(MF_SEPARATOR, 0, "");
            add(MF_STRING, ID_EXIT, "Exit");
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd); // otherwise the menu doesn't close on outside clicks
            let id = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            id as usize
        };
        match chosen {
            ID_OPEN => open_ui(),
            ID_AUTO => {
                let mut cfg = AgentConfig::load();
                cfg.auto_switch = !cfg.auto_switch;
                if let Err(e) = cfg.save() {
                    log::line(format!("agent: could not save agent.json: {e}"));
                }
            }
            ID_AUTOSTART => {
                if let Err(e) = sys::set_autostart(!autostart) {
                    log::line(format!("agent: start with Windows: {e}"));
                }
            }
            ID_EXIT => unsafe {
                DestroyWindow(hwnd);
            },
            id if (ID_SLOT0..ID_SLOT0 + 8).contains(&id) => {
                let _ = with_state(|s| s.jobs.send(Job { slot: (id - ID_SLOT0) as u8, why: "tray menu".into() }));
            }
            _ => {}
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_TRAY => {
                match lp as u32 {
                    WM_LBUTTONUP => open_ui(),
                    WM_RBUTTONUP => show_menu(hwnd),
                    _ => {}
                }
                0
            }
            WM_TIMER if wp == TIMER_FG => {
                with_state(foreground_settled);
                0
            }
            WM_SLOT => {
                with_state(|s| {
                    s.slot = wp.checked_sub(1).map(|v| v as u8);
                    tray(s, NIM_MODIFY);
                });
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                with_state(|s| tray(s, NIM_DELETE));
                PostQuitMessage(0);
                0
            }
            _ => {
                if with_state(|s| msg == s.taskbar_created && msg != 0) == Some(true) {
                    with_state(|s| tray(s, NIM_ADD)); // Explorer restarted
                    return 0;
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
    }

    /// Tray icon at the system small-icon size, from the embedded .ico.
    unsafe fn make_icon() -> HICON {
        let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 256);
        let data = crate::icon::best(size as u32);
        CreateIconFromResourceEx(data.as_ptr(), data.len() as u32, 1, 0x0003_0000, size, size, LR_DEFAULTCOLOR)
    }
}

#[cfg(not(any(windows, feature = "check-win")))]
mod imp {
    pub fn run() -> i32 {
        crate::log::line("agent: tray mode is Windows-only");
        1
    }
}

pub use imp::run;
