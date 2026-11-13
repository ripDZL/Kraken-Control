//! Gen-2 worker thread (VID 0x19F5 boards, e.g. the 2025 A75 Ultra).
//! Same rules as device.rs: one request in flight, cross-process device
//! lock around every transaction, idle ping, read-back after writes.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use dd_proto::gen2::{self, cmd, func, notify, Action, Packet, Trigger};

use crate::hid::{self, HidDevice};
use crate::log;

/// Everything read from the board for one profile.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub name: &'static str,
    pub fw: String,
    pub profile: u8,
    pub func: Vec<u8>,
    pub triggers: Vec<Trigger>,
    /// Factory keymap (profile 0), 4 layers; layer 0 maps layout keys to matrix slots.
    pub factory: Vec<Vec<Action>>,
    /// User keymap, 4 layers.
    pub keymap: Vec<Vec<Action>>,
    /// Per-key colours (effect "Custom").
    pub colors: Vec<[u8; 3]>,
    /// Raw advanced-key tables (see `gen2::find_advanced`).
    pub mt: Vec<u8>,
    pub tgl: Vec<u8>,
    pub dks: Vec<u8>,
    /// Raw macro block (`gen2::parse_macros`); empty if the keyboard didn't answer the read.
    pub macros: Vec<u8>,
}

/// Writable advanced-key tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    Mt,
    Tgl,
    Dks,
}

impl Table {
    fn spec(self, profile: u8) -> (u8, u8, u16, usize) {
        match self {
            Table::Mt => (cmd::MT_READ, cmd::MT_WRITE, gen2::mt_base(profile), gen2::MT_LEN),
            Table::Tgl => (cmd::TGL_READ, cmd::TGL_WRITE, gen2::tgl_base(profile), gen2::TGL_LEN),
            Table::Dks => (cmd::DKS_READ, cmd::DKS_WRITE, gen2::dks_base(profile), gen2::DKS_LEN),
        }
    }
}

pub enum Cmd {
    Refresh,
    /// Writes the whole trigger table of `profile`, then verifies it.
    SaveTriggers { profile: u8, triggers: Vec<Trigger> },
    /// Read-modify-write of single fields of the settings block.
    SaveFunc { profile: u8, edits: Vec<(func::Field, u8)> },
    SelectProfile(u8),
    /// Writes one whole layer (128 entries), then verifies it.
    SaveKeymap { profile: u8, layer: usize, actions: Vec<Action> },
    /// Writes the per-key colour table, then verifies it.
    SaveColors { profile: u8, colors: Vec<[u8; 3]> },
    /// Writes a whole advanced-key table, then verifies it.
    SaveTable { profile: u8, table: Table, data: Vec<u8> },
    /// Writes the macro block (header + events, `gen2::encode_macros`), then verifies it.
    SaveMacros { profile: u8, data: Vec<u8> },
    /// Reads the colours the keyboard is showing (live view on the Lighting page).
    ReadLeds,
    /// Live key depth on / off: sets or clears the keyboard's debug bit
    /// (`func::DEBUG`) on the current profile, as the official Performance tab does.
    LiveDepth(bool),
}

pub enum Event {
    Connected { name: &'static str, pid: u16 },
    Unsupported(String),
    Disconnected(String),
    Snapshot(Box<Snapshot>),
    Triggers(Vec<Trigger>),
    Func(Vec<u8>),
    Keymap { layer: usize, actions: Vec<Action> },
    Colors(Vec<[u8; 3]>),
    Table { table: Table, data: Vec<u8> },
    /// The macro block as read back after a write.
    Macros(Vec<u8>),
    /// Live colours (128 × RGB by matrix slot); empty = the keyboard didn't answer.
    Leds(Vec<u8>),
    /// Key travel reports received since the last event (key, 0.01 mm).
    Travel(Vec<(Action, u16)>),
    Busy(bool),
    Done(String),
    Error(String),
}

pub struct Handle {
    tx: Sender<Cmd>,
    pub rx: Receiver<Event>,
    /// Signalled by the worker when it has finished (after clearing the debug bit).
    done: Receiver<()>,
}

impl Drop for Handle {
    /// Closing the channel stops the worker; give it a moment to switch live
    /// key depth off so the keyboard isn't left streaming travel reports.
    fn drop(&mut self) {
        drop(std::mem::replace(&mut self.tx, mpsc::channel().0));
        let _ = self.done.recv_timeout(Duration::from_millis(800));
    }
}

impl Handle {
    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

pub fn spawn(on_event: impl Fn() + Send + 'static) -> Handle {
    let (tx, cmd_rx) = mpsc::channel();
    let (ev_tx, rx) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    thread::Builder::new()
        .name("device-gen2".into())
        .spawn(move || {
            Worker { tx: ev_tx, notify: Box::new(on_event), session: None, reported_unsupported: false, debug_profile: None }.run(cmd_rx);
            let _ = done_tx.send(());
        })
        .expect("spawn gen2 thread");
    Handle { tx, rx, done }
}

const REPLY_TIMEOUT_MS: u32 = 1500; // official races each reply against 1.5 s
const IDLE_PING: Duration = Duration::from_secs(3);
const RECONNECT_EVERY: Duration = Duration::from_millis(1500);
const LOCK_WAIT_MS: u32 = 3000;

struct Session {
    dev: HidDevice,
    name: &'static str,
    profile: u8,
    quiet: bool,
    /// Profile switch announced by the keyboard (`A1 .. idx`).
    switched_to: Option<u8>,
    warned_echo: bool,
    /// Live key depth: travel reports collected while reading (None = off).
    travel: Option<Vec<(Action, u16)>>,
}

impl Session {
    fn log(&self, dir: &str, bytes: &[u8]) {
        if !self.quiet {
            log::packet(dir, bytes);
        }
    }

    /// Next input report without the leading report-ID byte. Unsolicited
    /// notifications are consumed here and never returned.
    fn read_payload(&mut self, timeout_ms: u32) -> io::Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
            let Some(r) = self.dev.read_report(left)? else { return Ok(None) };
            if r.len() < 2 {
                continue;
            }
            let p = r[1..].to_vec();
            // Travel reports come with every key movement: keep them out of the packet log.
            if p[0] != notify::CALIBRATION || self.travel.is_none() {
                self.log("<-", &p);
            }
            match p[0] {
                notify::PROFILE => {
                    if let Some(&i) = p.get(2) {
                        self.switched_to = Some(i);
                    }
                }
                notify::CALIBRATION => {
                    if let (Some(buf), Some(t)) = (self.travel.as_mut(), gen2::parse_travel(&p)) {
                        buf.push((t.key, t.travel));
                    }
                }
                notify::RESET => {}
                gen2::REPLY_OK if p[1] == 0xFA => {} // lighting changed on the keyboard
                _ => return Ok(Some(p)),
            }
            if left == 0 {
                return Ok(None);
            }
        }
    }

    fn drain(&mut self) -> io::Result<()> {
        while self.read_payload(5)?.is_some() {}
        Ok(())
    }

    fn request(&mut self, p: &Packet) -> io::Result<Vec<u8>> {
        self.log("->", p);
        self.dev.write_report(gen2::REPORT_ID, p, REPLY_TIMEOUT_MS)?;
        let r = self
            .read_payload(REPLY_TIMEOUT_MS)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, format!("no reply to {:02X} {:02X}", p[0], p[1])))?;
        if r[0] != gen2::REPLY_OK {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("reply {:02x?} to {:02X} {:02X}", &r[..8], p[0], p[1])));
        }
        if r[1] != p[1] && !self.warned_echo {
            // The official driver takes any next report; note it once so the log shows the real format.
            self.warned_echo = true;
            log::line(format!("gen2: reply byte 1 {:02X} != command {:02X} (accepted)", r[1], p[1]));
        }
        Ok(r)
    }

    fn read_region(&mut self, c: u8, base: u16, len: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        for p in gen2::read_region(c, base, len) {
            let want = p[4] as usize;
            let r = self.request(&p)?;
            let data = r.get(gen2::DATA..gen2::DATA + want).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "short reply"))?;
            out.extend_from_slice(data);
        }
        Ok(out)
    }

    fn write_region(&mut self, c: u8, base: u16, data: &[u8]) -> io::Result<()> {
        for p in gen2::write_region(c, base, data) {
            self.request(&p)?;
        }
        Ok(())
    }

    fn profile(&mut self) -> io::Result<u8> {
        let r = self.request(&gen2::base())?;
        let p = r.get(gen2::DATA).copied().unwrap_or(0);
        Ok(if p >= gen2::PROFILES { 0 } else { p })
    }

    fn triggers(&mut self, profile: u8) -> io::Result<Vec<Trigger>> {
        Ok(gen2::parse_triggers(&self.read_region(cmd::TRIGGER_READ, gen2::trigger_base(profile), gen2::KEYS * 8)?))
    }

    fn func(&mut self, profile: u8) -> io::Result<Vec<u8>> {
        self.read_region(cmd::FUNC_READ, gen2::func_base(profile), gen2::FUNC_LEN)
    }

    /// Sets or clears the debug bit of `profile` (read-modify-write, like the
    /// official `setDebugModeKey`), then checks it. Returns whether it changed.
    fn set_debug(&mut self, profile: u8, on: bool) -> io::Result<bool> {
        let mut block = self.func(profile)?;
        if func::get(&block, func::DEBUG) == on as u8 {
            return Ok(false);
        }
        func::set(&mut block, func::DEBUG, on as u8);
        self.write_region(cmd::FUNC_WRITE, gen2::func_base(profile), &block)?;
        let back = self.func(profile)?;
        if func::get(&back, func::DEBUG) != on as u8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "debug bit read back differently"));
        }
        Ok(true)
    }

    fn snapshot(&mut self) -> io::Result<Snapshot> {
        let fw = gen2::parse_fw(&self.request(&gen2::info())?).unwrap_or_default();
        self.profile = self.profile()?;
        let profile = self.profile;
        let func = self.func(profile)?;
        let triggers = self.triggers(profile)?;
        let mut factory = Vec::with_capacity(gen2::LAYERS);
        let mut keymap = Vec::with_capacity(gen2::LAYERS);
        for layer in 0..gen2::LAYERS {
            factory.push(gen2::parse_keymap(&self.read_region(cmd::KEYMAP_DEFAULT, gen2::keymap_base(0, layer), gen2::KEYS * 3)?));
            keymap.push(self.keymap(profile, layer)?);
        }
        let colors = self.colors(profile)?;
        let mt = self.read_region(cmd::MT_READ, gen2::mt_base(profile), gen2::MT_LEN)?;
        let tgl = self.read_region(cmd::TGL_READ, gen2::tgl_base(profile), gen2::TGL_LEN)?;
        let dks = self.read_region(cmd::DKS_READ, gen2::dks_base(profile), gen2::DKS_LEN)?;
        // The official driver reads the macro block on connect too; a keyboard that
        // doesn't answer it still gets everything else.
        let macros = match self.read_region(cmd::MACRO_READ, gen2::macro_base(profile), gen2::MACRO_LEN) {
            Ok(m) => m,
            Err(e) if is_disconnect(&e) => return Err(e),
            Err(e) => {
                log::line(format!("gen2: macro block not readable ({e}); macros are off"));
                let _ = self.drain();
                Vec::new()
            }
        };
        Ok(Snapshot { name: self.name, fw, profile, func, triggers, factory, keymap, colors, mt, tgl, dks, macros })
    }

    fn keymap(&mut self, profile: u8, layer: usize) -> io::Result<Vec<Action>> {
        Ok(gen2::parse_keymap(&self.read_region(cmd::KEYMAP_READ, gen2::keymap_base(profile, layer), gen2::KEYS * 3)?))
    }

    fn colors(&mut self, profile: u8) -> io::Result<Vec<[u8; 3]>> {
        Ok(gen2::parse_colors(&self.read_region(cmd::COLORS_READ, gen2::colors_base(profile), gen2::KEYS * 3)?))
    }
}

struct Worker {
    tx: Sender<Event>,
    notify: Box<dyn Fn() + Send>,
    session: Option<Session>,
    reported_unsupported: bool,
    /// Profile whose debug bit Kraken set for live key depth.
    debug_profile: Option<u8>,
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
        (self.notify)();
    }

    fn run(mut self, rx: Receiver<Cmd>) {
        let mut last_try = Instant::now() - RECONNECT_EVERY;
        let mut last_io = Instant::now();
        loop {
            if self.session.is_none() && last_try.elapsed() >= RECONNECT_EVERY {
                last_try = Instant::now();
                self.locked(Self::connect);
                last_io = Instant::now();
            }
            // Live key depth: wake often to pick up travel reports.
            let live = self.session.as_ref().is_some_and(|s| s.travel.is_some());
            let wait = Duration::from_millis(if live { 10 } else { 250 });
            match rx.recv_timeout(wait) {
                Ok(c) => {
                    if !self.locked(|w| w.handle(c)) {
                        self.emit(Event::Error("keyboard busy (another Kraken Control process is using it)".into()));
                    }
                    last_io = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.session.is_some() && last_io.elapsed() >= IDLE_PING {
                        last_io = Instant::now();
                        self.locked(Self::ping);
                    } else if live {
                        self.locked(|_| {}); // `locked` drains, which collects travel reports
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    // UI closed: don't leave the keyboard streaming travel reports.
                    self.locked(|w| w.set_live_depth(false));
                    return;
                }
            }
            if let Some(buf) = self.session.as_mut().and_then(|s| s.travel.as_mut()).filter(|b| !b.is_empty()) {
                let list = std::mem::take(buf);
                self.emit(Event::Travel(list));
            }
        }
    }

    /// Profile changed (on the keyboard or from the UI) while live key depth is
    /// on: move the debug bit to the new profile.
    fn follow_profile(&mut self) {
        let moved = self.session.as_ref().is_some_and(|s| s.travel.is_some() && self.debug_profile != Some(s.profile));
        if moved {
            self.set_live_depth(true);
        }
    }

    /// Live key depth on / off. Errors are reported; the stream simply stays off.
    fn set_live_depth(&mut self, on: bool) {
        let Some(s) = self.session.as_mut() else {
            self.debug_profile = None;
            return;
        };
        let was = s.travel.is_some();
        let want = on.then_some(s.profile);
        let mut res = Ok(());
        if self.debug_profile != want {
            if let Some(p) = self.debug_profile.take() {
                res = s.set_debug(p, false).map(|_| ());
            }
            if let (Some(p), Ok(())) = (want, &res) {
                res = s.set_debug(p, true).map(|_| ());
                if res.is_ok() {
                    self.debug_profile = Some(p);
                }
            }
        }
        s.travel = (on && res.is_ok()).then(Vec::new);
        match res {
            Ok(()) if was != on => log::line(if on { "gen2: live key depth on" } else { "gen2: live key depth off" }),
            Ok(()) => {}
            Err(e) => self.fail(e, "Live key depth"),
        }
    }

    fn locked(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        let Some(_lock) = hid::lock(LOCK_WAIT_MS) else { return false };
        if let Some(s) = self.session.as_mut() {
            let _ = s.drain();
        }
        f(self);
        true
    }

    fn connect(&mut self) {
        for info in hid::candidates_gen2() {
            let Some(board) = gen2::board(info.pid) else {
                if !self.reported_unsupported {
                    self.reported_unsupported = true;
                    log::line(format!("gen2: unsupported board pid={:04x}", info.pid));
                    self.emit(Event::Unsupported(format!("DrunkDeer gen-2 board with PID {:04X} is not supported yet", info.pid)));
                }
                continue;
            };
            let dev = match HidDevice::open(&info) {
                Ok(d) => d,
                Err(e) => {
                    log::line(format!("gen2: open failed pid={:04x} out={} in={} {}: {e}", info.pid, info.output_len, info.input_len, info.path_lossy()));
                    continue;
                }
            };
            let mut s = Session { dev, name: board.name, profile: 0, quiet: false, switched_to: None, warned_echo: false, travel: None };
            if s.drain().is_err() {
                continue;
            }
            match s.request(&gen2::info()) {
                Ok(r) => {
                    log::line(format!(
                        "gen2: connected {} pid={:04x} fw={} out={} in={} path={}",
                        board.name,
                        info.pid,
                        gen2::parse_fw(&r).unwrap_or_default(),
                        info.output_len,
                        info.input_len,
                        info.path_lossy()
                    ));
                    if let Ok(r) = s.request(&gen2::public_info()) {
                        if let Some(t) = gen2::parse_public_type(&r) {
                            log::line(format!("gen2: public device type {t}"));
                        }
                    }
                    // A debug bit left on (Kraken or the official page closed during live
                    // key depth) keeps the keyboard sending travel reports: clear it.
                    if let Ok(p) = s.profile() {
                        if let Ok(true) = s.set_debug(p, false) {
                            log::line(format!("gen2: cleared a leftover debug bit on profile {}", p + 1));
                        }
                    }
                    self.session = Some(s);
                    self.emit(Event::Connected { name: board.name, pid: info.pid });
                    self.refresh();
                    return;
                }
                Err(e) => log::line(format!("gen2: no reply pid={:04x} {}: {e}", info.pid, info.path_lossy())),
            }
        }
    }

    fn refresh(&mut self) {
        let res = self.session.as_mut().map(Session::snapshot);
        match res {
            Some(Ok(snap)) => {
                self.follow_profile();
                self.emit(Event::Snapshot(Box::new(snap)))
            }
            Some(Err(e)) => self.fail(e, "Read failed"),
            None => {}
        }
    }

    /// Keep-alive; also notices profile switches made on the keyboard.
    fn ping(&mut self) {
        let Some(s) = self.session.as_mut() else { return };
        s.quiet = true;
        let res = s.profile();
        s.quiet = false;
        match res {
            Ok(p) => {
                let announced = s.switched_to.take();
                if p != s.profile || announced.is_some_and(|a| a != s.profile) {
                    log::line(format!("gen2: profile changed to {}", p + 1));
                    self.refresh();
                }
            }
            Err(e) => self.fail(e, "Ping failed"),
        }
    }

    /// Live colour read: polled a few times a second, so it stays out of the
    /// packet log and the busy state. A keyboard that doesn't answer ends the polling.
    fn read_leds(&mut self) {
        let Some(s) = self.session.as_mut() else { return };
        s.quiet = true;
        let start = std::time::Instant::now();
        let res = s.read_region(cmd::LED_READ, 0, gen2::LED_LEN);
        s.quiet = false;
        // Once per run: how long a read takes decides how smooth the live view can be.
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if res.is_ok() && !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::line(format!("gen2: live colours read in {} ms", start.elapsed().as_millis()));
        }
        match res {
            Ok(data) => self.emit(Event::Leds(data)),
            Err(e) if is_disconnect(&e) => self.fail(e, "Live colours"),
            Err(e) => {
                log::line(format!("gen2: live colours not readable ({e}); live view off"));
                if let Some(s) = self.session.as_mut() {
                    let _ = s.drain();
                }
                self.emit(Event::Leds(Vec::new()));
            }
        }
    }

    fn fail(&mut self, e: io::Error, what: &str) {
        if is_disconnect(&e) {
            log::line(format!("gen2: disconnected: {e}"));
            self.session = None;
            self.debug_profile = None;
            self.emit(Event::Disconnected(e.to_string()));
        } else {
            log::line(format!("gen2: {what}: {e}"));
            self.emit(Event::Error(format!("{what}: {e}")));
        }
    }

    fn handle(&mut self, c: Cmd) {
        if let Cmd::LiveDepth(on) = c {
            self.set_live_depth(on);
            return;
        }
        if self.session.is_none() {
            self.emit(Event::Error("No gen-2 keyboard connected".into()));
            return;
        }
        if matches!(c, Cmd::ReadLeds) {
            self.read_leds();
            return;
        }
        self.emit(Event::Busy(true));
        let res = self.run_cmd(c);
        self.follow_profile();
        self.emit(Event::Busy(false));
        match res {
            Ok(msg) => {
                log::line(format!("gen2: {msg}"));
                self.emit(Event::Done(msg));
            }
            Err(e) => self.fail(e, "Error"),
        }
    }

    fn run_cmd(&mut self, c: Cmd) -> io::Result<String> {
        let s = self.session.as_mut().expect("checked");
        match c {
            Cmd::ReadLeds | Cmd::LiveDepth(_) => Ok(String::new()), // handled in `handle` (no status, no busy state)
            Cmd::Refresh => {
                let snap = s.snapshot()?;
                self.emit(Event::Snapshot(Box::new(snap)));
                Ok("Read settings from keyboard".into())
            }
            Cmd::SaveTriggers { profile, triggers } => {
                if triggers.len() != gen2::KEYS {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "trigger table must have 128 entries"));
                }
                s.write_region(cmd::TRIGGER_WRITE, gen2::trigger_base(profile), &gen2::encode_triggers(&triggers))?;
                let back = s.triggers(profile)?;
                let bad = back.iter().zip(&triggers).filter(|(a, b)| a != b).count();
                self.emit(Event::Triggers(back));
                if bad == 0 {
                    Ok("Saved key travel (verified by read-back)".into())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, format!("Key travel read-back differs on {bad} keys")))
                }
            }
            Cmd::SaveFunc { profile, edits } => {
                let mut block = s.func(profile)?;
                for (field, v) in &edits {
                    func::set(&mut block, *field, *v);
                }
                s.write_region(cmd::FUNC_WRITE, gen2::func_base(profile), &block)?;
                let back = s.func(profile)?;
                let ok = edits.iter().all(|(f, v)| func::get(&back, *f) == *v);
                self.emit(Event::Func(back));
                if ok {
                    Ok("Saved settings".into())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, "Settings read back differently"))
                }
            }
            Cmd::SaveKeymap { profile, layer, actions } => {
                if actions.len() != gen2::KEYS || layer >= gen2::LAYERS {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad keymap layer"));
                }
                s.write_region(cmd::KEYMAP_WRITE, gen2::keymap_base(profile, layer), &gen2::encode_keymap(&actions))?;
                let back = s.keymap(profile, layer)?;
                let bad = back.iter().zip(&actions).filter(|(a, b)| a != b).count();
                self.emit(Event::Keymap { layer, actions: back });
                let name = ["Main", "FN1", "FN2", "FN3"][layer];
                if bad == 0 {
                    Ok(format!("Saved {name} layer (verified by read-back)"))
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, format!("{name} layer read-back differs on {bad} keys")))
                }
            }
            Cmd::SaveColors { profile, colors } => {
                if colors.len() != gen2::KEYS {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "colour table must have 128 entries"));
                }
                s.write_region(cmd::COLORS_WRITE, gen2::colors_base(profile), &gen2::encode_colors(&colors))?;
                let back = s.colors(profile)?;
                let same = back == colors;
                self.emit(Event::Colors(back));
                if same {
                    Ok("Saved key colours (verified by read-back)".into())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, "Key colours read back differently"))
                }
            }
            Cmd::SaveTable { profile, table, data } => {
                let (read, write, base, len) = table.spec(profile);
                if data.len() != len {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad table length"));
                }
                s.write_region(write, base, &data)?;
                let back = s.read_region(read, base, len)?;
                let same = back == data;
                self.emit(Event::Table { table, data: back });
                if same {
                    Ok(format!("Saved {table:?} table (verified by read-back)"))
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, format!("{table:?} table read back differently")))
                }
            }
            Cmd::SaveMacros { profile, data } => {
                if data.len() < gen2::MACRO_HEADER || data.len() > gen2::MACRO_LEN {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad macro block"));
                }
                let base = gen2::macro_base(profile);
                s.write_region(cmd::MACRO_WRITE, base, &data)?;
                let back = s.read_region(cmd::MACRO_READ, base, gen2::MACRO_LEN)?;
                let same = back[..data.len()] == data[..];
                self.emit(Event::Macros(back));
                if same {
                    Ok("Saved macros (verified by read-back)".into())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, "Macros read back differently"))
                }
            }
            Cmd::SelectProfile(i) => {
                s.request(&gen2::select_profile(i))?;
                std::thread::sleep(Duration::from_millis(100));
                let snap = s.snapshot()?;
                let got = snap.profile;
                self.emit(Event::Snapshot(Box::new(snap)));
                if got == i {
                    Ok(format!("Switched to profile {}", i + 1))
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, format!("Keyboard still reports profile {}", got + 1)))
                }
            }
        }
    }
}

fn is_disconnect(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(1167) | Some(31) | Some(6))
}
