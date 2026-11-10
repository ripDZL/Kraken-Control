//! Memory use: a counting allocator for Kraken's own heap, plus what the OS
//! reports for the whole process (graphics driver included).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// System allocator that keeps a running total (two relaxed atomics per call).
pub struct Counting;

static HEAP: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grow(n: usize) {
    let now = HEAP.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(now, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            grow(l.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            grow(l.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        HEAP.fetch_sub(l.size(), Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new_size) };
        if !q.is_null() {
            if new_size >= l.size() {
                grow(new_size - l.size());
            } else {
                HEAP.fetch_sub(l.size() - new_size, Relaxed);
            }
        }
        q
    }
}

/// Bytes Kraken itself has allocated right now, and the most it ever had.
pub fn heap() -> (usize, usize) {
    (HEAP.load(Relaxed), PEAK.load(Relaxed))
}

/// Whole process: (private bytes, working set). Private bytes is what Process
/// Explorer shows; Task Manager's "Memory" is the private part of the working set.
#[cfg(any(windows, feature = "check-win"))]
pub fn process() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    // SAFETY: `c` is a valid, correctly sized PROCESS_MEMORY_COUNTERS_EX.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb) };
    (ok != 0).then_some((c.PrivateUsage as u64, c.WorkingSetSize as u64))
}

/// Linux (demo / screenshots): anonymous resident memory and total resident memory.
#[cfg(not(any(windows, feature = "check-win")))]
pub fn process() -> Option<(u64, u64)> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb = |key: &str| -> Option<u64> {
        let line = s.lines().find(|l| l.starts_with(key))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok().map(|v| v * 1024)
    };
    Some((kb("RssAnon:")?, kb("VmRSS:")?))
}

pub fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// One line for the log and the Troubleshooting card.
pub fn summary(font_atlas: [usize; 2]) -> String {
    let (heap, peak) = heap();
    let os = process().map_or(String::new(), |(private, ws)| format!("private {:.1} MB, working set {:.1} MB, ", mb(private), mb(ws)));
    let renderer = if crate::soft::active() {
        format!("software renderer {:.1} ms/frame", crate::soft::last_frame_ms())
    } else {
        "GPU renderer".to_string()
    };
    format!(
        "{os}Kraken heap {:.1} MB (peak {:.1} MB), font atlas {}x{}, {renderer}",
        mb(heap as u64),
        mb(peak as u64),
        font_atlas[0],
        font_atlas[1]
    )
}
