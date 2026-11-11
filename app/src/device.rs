//! Device worker thread: owns the HID handle, runs one request at a time
//! (like the official driver: one packet in flight, wait for its reply).

use std::cell::Cell;
use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use dd_proto::knob::{self, KnobMap};
use dd_proto::packets::{self, CommonFlags, DepthFrame, Packet, Travel, TravelReadback, REPORT_ID};
use dd_proto::rtp::{self, MatchMode, RdtPair};
use dd_proto::{Identity, Keymap, SLOTS};

use crate::hid::{self, HidDevice};
use crate::log;

pub type Values = [f32; SLOTS];
type Pairs = (Vec<(u8, u8)>, Vec<RdtPair>);

#[derive(Clone, Debug)]
pub struct TravelSet {
    pub ap: Values,
    pub ds: Values,
    pub us: Values,
}

/// Everything readable from the keyboard.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub identity: Identity,
    pub travel: TravelSet,
    pub lw: Vec<(u8, u8)>,
    pub rdt: Vec<RdtPair>,
    /// Onboard profile slot count (firmware >= 81), if the query succeeded.
    pub slots: Option<u8>,
}

pub enum Cmd {
    /// Re-read everything.
    Refresh,
    SaveCommon(CommonFlags),
    /// Writes only the listed tables (an unchanged table is never rewritten
    /// from possibly stale values), then verifies all three by read-back.
    SaveTravel { set: Box<TravelSet>, kinds: Vec<Travel> },
    SaveKeymap(Box<Keymap>),
    SavePairs { last_win: bool, rdt: bool, lw: Vec<(u8, u8)>, rdt_pairs: Vec<RdtPair> },
    SetMatchMode(Option<MatchMode>),
    SetLwReplace(bool),
    SelectSlot { index: u8, count: u8 },
    SetSlotCount(u8),
    /// Gen-1 lighting packets (`dd_proto::led`); acknowledged, not readable.
    Lighting { packets: Vec<Packet>, what: &'static str },
    /// Live key depth on/off (official "Keystroke tracking").
    Tracking(bool),
    /// Gen-1 knob, all six events (acknowledged, not readable).
    SaveKnob(Box<KnobMap>),
}

pub enum Event {
    Connected(Identity),
    Disconnected(String),
    Snapshot(Box<Snapshot>),
    Travel(Box<TravelSet>),
    Pairs { lw: Vec<(u8, u8)>, rdt: Vec<RdtPair> },
    Flags(Identity),
    Slots(u8),
    Busy(bool),
    Done(String),
    Error(String),
    /// Live key depth in mm (only while tracking is on).
    Depth(Box<Values>),
    /// Tracking stopped by the worker (no depth replies).
    TrackingStopped(String),
}

pub struct Handle {
    tx: Sender<Cmd>,
    pub rx: Receiver<Event>,
}

impl Handle {
    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

pub fn spawn(on_event: impl Fn() + Send + 'static) -> Handle {
    let (tx, cmd_rx) = mpsc::channel();
    let (ev_tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("device".into())
        .spawn(move || Worker { tx: ev_tx, notify: Box::new(on_event), session: None, tracking: false, depth_misses: 0, last_depth: None }.run(cmd_rx))
        .expect("spawn device thread");
    Handle { tx, rx }
}

const REPLY_TIMEOUT_MS: u32 = 1000;
const IDLE_PING: Duration = Duration::from_secs(3);
const RECONNECT_EVERY: Duration = Duration::from_millis(1500);
/// Max wait for the cross-process device lock (the tray agent holds it briefly).
const LOCK_WAIT_MS: u32 = 3000;
/// Pause between depth frames (official driver: 20 ms after the last packet).
const DEPTH_PAUSE: Duration = Duration::from_millis(20);
/// Wait for one complete depth frame.
const DEPTH_FRAME_MS: u32 = 150;
/// Frames in a row without any depth packet before tracking gives up.
const DEPTH_MAX_MISSES: u8 = 5;

pub struct Session {
    dev: HidDevice,
    identity: Identity,
    /// Suppresses packet logging (idle pings).
    quiet: Cell<bool>,
}

/// Onboard slot state (firmware >= 81).
pub struct SlotState {
    pub current: u8,
    pub count: u8,
}

fn busy() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "keyboard busy (another Kraken Control process is using it)")
}

/// Opens the keyboard for one short transaction under the cross-process lock
/// (used by the tray agent, which does not keep the device open).
pub fn one_shot<T>(wait_ms: u32, f: impl FnOnce(&Session) -> io::Result<T>) -> io::Result<T> {
    let _lock = hid::lock(wait_ms).ok_or_else(busy)?;
    for info in hid::candidates() {
        let Ok(dev) = HidDevice::open(&info) else { continue };
        let mut s = Session::new(dev);
        if s.drain().is_err() {
            continue;
        }
        if let Ok(id) = s.identity() {
            if id.model.is_some() {
                s.identity = id;
                return f(&s);
            }
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "keyboard not found"))
}

impl Session {
    fn new(dev: HidDevice) -> Self {
        Session { dev, identity: dummy_identity(), quiet: Cell::new(false) }
    }

    pub fn slot_state(&self) -> io::Result<SlotState> {
        if !self.identity.has_onboard_profiles() {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "firmware has no onboard slots (needs 0.081+)"));
        }
        let count = self.slot_count().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no slot count reply"))?;
        Ok(SlotState { current: self.identity.profile_index, count })
    }

    pub fn select_slot(&self, index: u8, count: u8) -> io::Result<()> {
        self.request(&rtp::select_slot(index, count)).map(|_| ())
    }

    /// Next input payload with our report ID (report ID byte stripped).
    fn read_payload(&self, timeout_ms: u32) -> io::Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
            match self.dev.read_report(left)? {
                None => return Ok(None),
                Some(r) if r.len() > 1 && r[0] == REPORT_ID => {
                    if !self.quiet.get() {
                        log::packet("<-", &r[1..]);
                    }
                    return Ok(Some(r[1..].to_vec()));
                }
                Some(_) if left == 0 => return Ok(None),
                Some(_) => continue,
            }
        }
    }

    /// Discards queued input reports (stale echoes would otherwise be taken
    /// as the reply to the next request).
    fn drain(&self) -> io::Result<()> {
        while self.read_payload(5)?.is_some() {}
        Ok(())
    }

    fn write(&self, p: &Packet) -> io::Result<()> {
        if !self.quiet.get() {
            log::packet("->", p);
        }
        self.dev.write_report(REPORT_ID, p, REPLY_TIMEOUT_MS)
    }

    /// Sends `p` and waits for its acknowledgement, skipping unrelated reports.
    fn request(&self, p: &Packet) -> io::Result<Vec<u8>> {
        self.write(p)?;
        let (b0, b1) = packets::expected_reply(p);
        let deadline = Instant::now() + Duration::from_millis(REPLY_TIMEOUT_MS as u64);
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
            match self.read_payload(left.max(1))? {
                // A late live-depth packet (FD 06) is never the ack for another FD command.
                Some(r) if r.len() > 1 && r[0] == 0xFD && r[1] == 0x06 && p[1] != 0x03 => continue,
                Some(r) if r.first() == Some(&b0) && b1.is_none_or(|b| r.get(1) == Some(&b)) => return Ok(r),
                Some(_) => continue,
                None => break,
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, format!("no reply to {:02X} {:02X}", p[0], p[1])))
    }

    fn identity(&self) -> io::Result<Identity> {
        let r = self.request(&packets::identity())?;
        Identity::parse(&r).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad identity reply"))
    }

    fn pull(&self, kind: Travel) -> io::Result<Values> {
        self.drain()?;
        self.write(&packets::travel_pull(kind))?;
        let mut rb = TravelReadback::default();
        let deadline = Instant::now() + Duration::from_millis(2000);
        while !rb.is_complete() && Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
            match self.read_payload(left.max(1))? {
                Some(r) => {
                    rb.feed(kind, &r);
                }
                None => break,
            }
        }
        rb.mm().ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, format!("incomplete {kind:?} read-back")))
    }

    fn pull_all(&self) -> io::Result<TravelSet> {
        Ok(TravelSet { ap: self.pull(Travel::Ap)?, ds: self.pull(Travel::Ds)?, us: self.pull(Travel::Us)? })
    }

    /// LW/RDT pair tables. Pairs naming slots that don't exist on the
    /// board are dropped (and logged) rather than shown or written back.
    fn pull_pairs(&self) -> io::Result<Pairs> {
        let valid: Vec<u8> = self.identity.model.map(|m| m.layout().iter().map(|k| k.slot).collect()).unwrap_or_default();
        let ok = |s: u8| valid.contains(&s);
        self.drain()?;
        let r = self.request(&rtp::pull_lw())?;
        let mut lw = rtp::parse_lw(&r).unwrap_or_default();
        if lw.iter().any(|&(m, t)| !ok(m) || !ok(t)) {
            log::line(format!("LW read-back has unknown slots, ignored: {r:02x?}"));
            lw.retain(|&(m, t)| ok(m) && ok(t));
        }
        let r = self.request(&rtp::pull_rdt())?;
        let mut rdt = rtp::parse_rdt(&r).unwrap_or_default();
        if rdt.iter().any(|p| !ok(p.main) || !ok(p.trigger)) {
            log::line(format!("RDT read-back has unknown slots, ignored: {r:02x?}"));
            rdt.retain(|p| ok(p.main) && ok(p.trigger));
        }
        Ok((lw, rdt))
    }

    fn slot_count(&self) -> Option<u8> {
        if !self.identity.has_onboard_profiles() {
            return None;
        }
        self.request(&rtp::slot_count(0)).ok().and_then(|r| rtp::parse_slot_count(&r))
    }

    fn snapshot(&mut self) -> io::Result<Snapshot> {
        self.identity = self.identity()?;
        let travel = self.pull_all()?;
        let (lw, rdt) = self.pull_pairs()?;
        let slots = self.slot_count();
        Ok(Snapshot { identity: self.identity.clone(), travel, lw, rdt, slots })
    }

    /// Requests one live depth frame (`FD 03 01` -> `FD 06` x5). `None` = no depth packet came.
    fn depth_frame(&self) -> io::Result<Option<Values>> {
        self.write(&packets::tracking(true))?;
        let mut frame = DepthFrame::default();
        let deadline = Instant::now() + Duration::from_millis(DEPTH_FRAME_MS as u64);
        while !frame.is_complete() && Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
            match self.read_payload(left.max(1))? {
                Some(r) => {
                    frame.feed(&r);
                }
                None => break,
            }
        }
        Ok(frame.any().then(|| frame.mm()))
    }

    fn write_all(&self, pkts: &[Packet]) -> io::Result<()> {
        self.drain()?;
        for (i, p) in pkts.iter().enumerate() {
            self.request(p).map_err(|e| io::Error::new(e.kind(), format!("packet {}/{}: {e}", i + 1, pkts.len())))?;
        }
        Ok(())
    }
}

struct Worker {
    tx: Sender<Event>,
    notify: Box<dyn Fn() + Send>,
    session: Option<Session>,
    /// Live key depth requested by the UI.
    tracking: bool,
    depth_misses: u8,
    last_depth: Option<Values>,
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
            let wait = if self.tracking && self.session.is_some() { DEPTH_PAUSE } else { Duration::from_millis(250) };
            match rx.recv_timeout(wait) {
                Ok(cmd) => {
                    if !self.locked(|w| w.handle(cmd)) {
                        self.emit(Event::Error(busy().to_string()));
                    }
                    last_io = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) if self.tracking && self.session.is_some() => {
                    self.locked(Self::depth);
                    last_io = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.session.is_some() && last_io.elapsed() >= IDLE_PING {
                        last_io = Instant::now();
                        self.locked(Self::ping);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Runs `f` holding the cross-process device lock; stale input reports
    /// (e.g. replies to the tray agent) are drained first. `false` = lock timeout.
    fn locked(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        let Some(_lock) = hid::lock(LOCK_WAIT_MS) else { return false };
        if let Some(s) = &self.session {
            let _ = s.drain();
        }
        f(self);
        true
    }

    /// Keep-alive. Also notices slot switches made on the keyboard (Fn keys)
    /// or by the tray agent. Not packet-logged unless the reply changes.
    fn ping(&mut self) {
        let Some(s) = self.session.as_mut() else { return };
        s.quiet.set(true);
        let res = s.identity();
        s.quiet.set(false);
        match res {
            Ok(id) => {
                if id != s.identity && log::packet_logging() {
                    log::line(format!("identity changed: {:?}", id));
                }
                let switched = id.profile_index != s.identity.profile_index && id.model.is_some();
                s.identity = id;
                if switched {
                    log::line(format!("onboard slot changed to {}", s.identity.profile_index + 1));
                    self.refresh_snapshot();
                }
            }
            Err(e) => self.lost(e),
        }
    }

    fn connect(&mut self) {
        for info in hid::candidates() {
            let dev = match HidDevice::open(&info) {
                Ok(d) => d,
                Err(e) => {
                    log::line(format!("open failed pid={:04x} out={} in={} {}: {e}", info.pid, info.output_len, info.input_len, info.path_lossy()));
                    continue;
                }
            };
            let mut s = Session::new(dev);
            if s.drain().is_err() {
                continue;
            }
            match s.identity() {
                Ok(id) => {
                    log::line(format!(
                        "connected pid={:04x} type={:02x?} fw={} model={:?} path={}",
                        info.pid,
                        id.type_bytes,
                        id.firmware_label,
                        id.model,
                        info.path_lossy()
                    ));
                    s.identity = id.clone();
                    self.session = Some(s);
                    self.emit(Event::Connected(id.clone()));
                    if id.model.is_some() {
                        self.refresh_snapshot();
                    }
                    return;
                }
                Err(e) => {
                    log::line(format!("no identity reply pid={:04x} {}: {e}", info.pid, info.path_lossy()));
                    continue;
                }
            }
        }
    }

    fn refresh_snapshot(&mut self) {
        let res = self.session.as_mut().map(|s| s.snapshot());
        match res {
            Some(Ok(snap)) => self.emit(Event::Snapshot(Box::new(snap))),
            Some(Err(e)) if is_disconnect(&e) => self.lost(e),
            Some(Err(e)) => {
                log::line(format!("read-back failed: {e}"));
                self.emit(Event::Error(format!("Read-back failed: {e}")))
            }
            None => {}
        }
    }

    fn lost(&mut self, e: io::Error) {
        log::line(format!("disconnected: {e}"));
        self.session = None;
        self.tracking = false;
        self.emit(Event::Disconnected(e.to_string()));
    }

    /// One live depth frame; emits only when something moved.
    fn depth(&mut self) {
        let Some(s) = self.session.as_ref() else { return };
        s.quiet.set(true);
        let res = s.depth_frame();
        s.quiet.set(false);
        match res {
            Ok(Some(v)) => {
                self.depth_misses = 0;
                if self.last_depth.as_ref() != Some(&v) {
                    self.last_depth = Some(v);
                    self.emit(Event::Depth(Box::new(v)));
                }
            }
            Ok(None) => {
                self.depth_misses += 1;
                if self.depth_misses >= DEPTH_MAX_MISSES {
                    self.tracking = false;
                    log::line("live key depth: no FD 06 replies, stopped");
                    self.emit(Event::TrackingStopped("The keyboard didn't send live key depth. Its firmware may not support it.".into()));
                }
            }
            Err(e) if is_disconnect(&e) => self.lost(e),
            Err(e) => {
                self.tracking = false;
                log::line(format!("live key depth stopped: {e}"));
                self.emit(Event::TrackingStopped(format!("Live key depth stopped: {e}")));
            }
        }
    }

    fn set_tracking(&mut self, on: bool) {
        if on == self.tracking {
            return;
        }
        self.tracking = on;
        self.depth_misses = 0;
        self.last_depth = None;
        log::line(if on { "live key depth on" } else { "live key depth off" });
        if !on {
            if let Some(s) = &self.session {
                // Official driver sends FD 03 00 when tracking is switched off.
                let _ = s.write(&packets::tracking(false));
                let _ = s.drain();
            }
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        if let Cmd::Tracking(on) = cmd {
            self.set_tracking(on && self.session.as_ref().is_some_and(|s| s.identity.model.is_some()));
            return;
        }
        if self.session.as_ref().is_none_or(|s| s.identity.model.is_none()) {
            self.emit(Event::Error("No supported keyboard connected".into()));
            return;
        }
        self.emit(Event::Busy(true));
        let result = self.run_cmd(cmd);
        self.emit(Event::Busy(false));
        match result {
            Ok(msg) => {
                log::line(&msg);
                self.emit(Event::Done(msg))
            }
            Err(e) if is_disconnect(&e) => self.lost(e),
            Err(e) => {
                log::line(format!("error: {e}"));
                self.emit(Event::Error(e.to_string()))
            }
        }
    }

    fn run_cmd(&mut self, cmd: Cmd) -> io::Result<String> {
        let s = self.session.as_mut().expect("checked");
        match cmd {
            Cmd::Refresh => {
                let snap = s.snapshot()?;
                self.emit(Event::Snapshot(Box::new(snap)));
                Ok("Read settings from keyboard".into())
            }
            Cmd::SaveCommon(f) => {
                s.drain()?;
                s.request(&packets::common(f))?;
                s.identity = s.identity()?;
                let id = s.identity.clone();
                self.emit(Event::Flags(id));
                Ok("Saved switches".into())
            }
            Cmd::SaveTravel { set: t, kinds } => {
                let mut pkts = Vec::new();
                for kind in &kinds {
                    let values = match kind {
                        Travel::Ap => &t.ap,
                        Travel::Ds => &t.ds,
                        Travel::Us => &t.us,
                    };
                    pkts.extend(packets::travel_write(*kind, values));
                }
                s.write_all(&pkts)?;
                let back = s.pull_all()?;
                let bad = mismatches(&t, &back);
                self.emit(Event::Travel(Box::new(back)));
                if bad == 0 {
                    Ok("Saved actuation (verified by read-back)".into())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, format!("Actuation read-back differs on {bad} values")))
                }
            }
            Cmd::SaveKeymap(km) => {
                s.write_all(&packets::remap_all(&km))?;
                Ok("Saved keymap (3 layers)".into())
            }
            Cmd::SavePairs { last_win, rdt, lw, rdt_pairs } => {
                // Official `highRtpSaveToKeyboard`: clear with the active mode, then the table.
                let mut pkts = vec![rtp::clear(last_win, rdt)];
                if last_win {
                    pkts.push(rtp::lw_pairs(&lw));
                }
                if rdt {
                    pkts.push(rtp::rdt_pairs(&rdt_pairs));
                }
                s.write_all(&pkts)?;
                let (lw_back, rdt_back) = s.pull_pairs()?;
                let same_lw = !last_win || same_pairs(&lw, &lw_back);
                self.emit(Event::Pairs { lw: lw_back, rdt: rdt_back });
                if same_lw {
                    Ok("Saved Last Win / RDT pairs".into())
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidData, "Last Win pairs read back differently — see log"))
                }
            }
            Cmd::SetMatchMode(m) => {
                s.drain()?;
                s.request(&rtp::match_mode(m))?;
                s.identity = s.identity()?;
                let id = s.identity.clone();
                self.emit(Event::Flags(id));
                Ok(format!("RT Match mode: {}", m.map_or("off", MatchMode::name)))
            }
            Cmd::SetLwReplace(on) => {
                s.drain()?;
                s.request(&packets::lw_replace(on))?;
                s.identity = s.identity()?;
                let id = s.identity.clone();
                self.emit(Event::Flags(id));
                Ok(format!("Last Win full replace {}", if on { "on" } else { "off" }))
            }
            Cmd::SelectSlot { index, count } => {
                s.drain()?;
                s.request(&rtp::select_slot(index, count))?;
                let snap = s.snapshot()?;
                self.emit(Event::Snapshot(Box::new(snap)));
                Ok(format!("Switched to onboard slot {}", index + 1))
            }
            Cmd::SetSlotCount(n) => {
                s.drain()?;
                let r = s.request(&rtp::slot_count(n))?;
                let got = rtp::parse_slot_count(&r).unwrap_or(n);
                self.emit(Event::Slots(got));
                Ok(format!("Onboard slots: {got}"))
            }
            Cmd::Lighting { packets, what } => {
                s.write_all(&packets)?;
                Ok(format!("{what} sent (acknowledged; lighting can't be read back)"))
            }
            Cmd::SaveKnob(map) => {
                s.write_all(&knob::packets(&map))?;
                Ok("Saved knob (acknowledged; the knob can't be read back)".into())
            }
            // Handled in `handle` (no busy/done messages).
            Cmd::Tracking(_) => Ok(String::new()),
        }
    }
}

/// Same unordered pair set (the firmware may store both directions).
fn same_pairs(a: &[(u8, u8)], b: &[(u8, u8)]) -> bool {
    let norm = |v: &[(u8, u8)]| {
        let mut n: Vec<(u8, u8)> = v.iter().map(|&(x, y)| (x.min(y), x.max(y))).collect();
        n.sort();
        n.dedup();
        n
    };
    norm(a) == norm(b)
}

fn mismatches(a: &TravelSet, b: &TravelSet) -> usize {
    let cmp = |x: &Values, y: &Values, kind: Travel| {
        x.iter()
            .zip(y)
            .filter(|(p, q)| packets::mm_to_units(kind, **p) != packets::mm_to_units(kind, **q))
            .count()
    };
    cmp(&a.ap, &b.ap, Travel::Ap) + cmp(&a.ds, &b.ds, Travel::Ds) + cmp(&a.us, &b.us, Travel::Us)
}

fn is_disconnect(e: &io::Error) -> bool {
    // ERROR_DEVICE_NOT_CONNECTED (1167), ERROR_GEN_FAILURE (31), ERROR_INVALID_HANDLE (6)
    matches!(e.raw_os_error(), Some(1167) | Some(31) | Some(6))
}

fn dummy_identity() -> Identity {
    Identity {
        type_bytes: [0; 3],
        model: None,
        firmware: 0,
        firmware_label: String::new(),
        turbo: false,
        rapid_trigger: false,
        rdt: false,
        last_win: false,
        profile_index: 0,
        light_mode: 0,
        light_speed: 0,
        light_brightness: 0,
        rt_match: false,
        auto_match_mode: 0,
        lw_replace: false,
    }
}
