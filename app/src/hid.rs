//! Minimal Win32 HID transport (SetupAPI + overlapped ReadFile/WriteFile).
//! No C dependencies. On non-Windows builds a stub reports "unsupported"
//! (the `check-win` feature type-checks the real code on any host).

use std::io;

#[derive(Clone, Debug)]
#[allow(dead_code)] // usage: diagnostics
pub struct DeviceInfo {
    pub path: Vec<u16>,
    pub vid: u16,
    pub pid: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: usize,
    pub output_len: usize,
}

impl DeviceInfo {
    pub fn path_lossy(&self) -> String {
        String::from_utf16_lossy(&self.path[..self.path.len().saturating_sub(1)])
    }
}

#[cfg(any(windows, feature = "check-win"))]
mod imp {
    use super::DeviceInfo;
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::ptr::{null, null_mut};

    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
        SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData, HidP_GetCaps,
        HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::Foundation::WAIT_ABANDONED;
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, ReleaseMutex, WaitForSingleObject};
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    fn open_path(path: &[u16], access: u32, overlapped: bool) -> io::Result<Handle> {
        let flags = if overlapped { FILE_FLAG_OVERLAPPED } else { 0 };
        let h = unsafe {
            CreateFileW(path.as_ptr(), access, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, flags, null_mut())
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Handle(h))
    }

    /// Lists all present HID interfaces with their top-level collection caps.
    pub fn enumerate() -> Vec<DeviceInfo> {
        let mut out = Vec::new();
        unsafe {
            let mut guid: GUID = zeroed();
            HidD_GetHidGuid(&mut guid);
            let set = SetupDiGetClassDevsW(&guid, null(), null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
            if set == INVALID_HANDLE_VALUE as isize {
                return out;
            }
            let mut index = 0u32;
            loop {
                let mut iface: SP_DEVICE_INTERFACE_DATA = zeroed();
                iface.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
                if SetupDiEnumDeviceInterfaces(set, null(), &guid, index, &mut iface) == 0 {
                    break;
                }
                index += 1;
                if let Some(path) = interface_path(set, &iface) {
                    if let Some(info) = query(&path) {
                        out.push(info);
                    }
                }
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    unsafe fn interface_path(set: HDEVINFO, iface: &SP_DEVICE_INTERFACE_DATA) -> Option<Vec<u16>> {
        let mut required = 0u32;
        SetupDiGetDeviceInterfaceDetailW(set, iface, null_mut(), 0, &mut required, null_mut());
        if required == 0 {
            return None;
        }
        // u64 backing store keeps the struct aligned.
        let mut buf = vec![0u64; (required as usize).div_ceil(8)];
        let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        if SetupDiGetDeviceInterfaceDetailW(set, iface, detail, required, null_mut(), null_mut()) == 0 {
            return None;
        }
        let start = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
        let max = (required as usize - std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath)) / 2;
        let mut path = Vec::with_capacity(max);
        for i in 0..max {
            let c = *start.add(i);
            path.push(c);
            if c == 0 {
                break;
            }
        }
        if path.last() != Some(&0) {
            path.push(0);
        }
        Some(path)
    }

    /// Opens with zero access (works even for keyboard collections) to read
    /// VID/PID and collection caps.
    fn query(path: &[u16]) -> Option<DeviceInfo> {
        let h = open_path(path, 0, false).ok()?;
        unsafe {
            let mut attr: HIDD_ATTRIBUTES = zeroed();
            attr.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
            if HidD_GetAttributes(h.0, &mut attr) == 0 {
                return None;
            }
            let mut pp: PHIDP_PREPARSED_DATA = 0;
            if HidD_GetPreparsedData(h.0, &mut pp) == 0 {
                return None;
            }
            let mut caps: HIDP_CAPS = zeroed();
            let status = HidP_GetCaps(pp, &mut caps);
            HidD_FreePreparsedData(pp);
            if status != HIDP_STATUS_SUCCESS {
                return None;
            }
            Some(DeviceInfo {
                path: path.to_vec(),
                vid: attr.VendorID,
                pid: attr.ProductID,
                usage_page: caps.UsagePage,
                usage: caps.Usage,
                input_len: caps.InputReportByteLength as usize,
                output_len: caps.OutputReportByteLength as usize,
            })
        }
    }

    /// Cross-process lock around keyboard transactions: the UI and the tray
    /// agent each open the device, and the protocol allows one request in flight.
    pub struct HidLock(HANDLE);

    impl Drop for HidLock {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { ReleaseMutex(self.0) };
            }
        }
    }

    static MUTEX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

    pub fn lock(timeout_ms: u32) -> Option<HidLock> {
        let h = *MUTEX.get_or_init(|| {
            let name: Vec<u16> = "Local\\KrakenControlHid".encode_utf16().chain(Some(0)).collect();
            unsafe { CreateMutexW(null(), 0, name.as_ptr()) as usize }
        }) as HANDLE;
        if h.is_null() {
            return Some(HidLock(null_mut())); // no mutex: run unlocked rather than not at all
        }
        match unsafe { WaitForSingleObject(h, timeout_ms) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Some(HidLock(h)),
            _ => None,
        }
    }

    pub struct HidDevice {
        handle: Handle,
        event: Handle,
        pub info: DeviceInfo,
    }

    // The handles are plain kernel handles; the device is used from one thread at a time.
    unsafe impl Send for HidDevice {}

    impl HidDevice {
        pub fn open(info: &DeviceInfo) -> io::Result<Self> {
            let handle = open_path(&info.path, GENERIC_READ | GENERIC_WRITE, true)?;
            let ev = unsafe { CreateEventW(null(), 1, 0, null()) };
            if ev.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { handle, event: Handle(ev), info: info.clone() })
        }

        /// Runs one overlapped transfer. `Ok(None)` = timed out (I/O cancelled).
        fn transfer(&self, write: bool, buf: &mut [u8], timeout_ms: u32) -> io::Result<Option<usize>> {
            unsafe {
                let mut ov: OVERLAPPED = zeroed();
                ov.hEvent = self.event.0;
                let mut n = 0u32;
                let ok = if write {
                    WriteFile(self.handle.0, buf.as_ptr(), buf.len() as u32, null_mut(), &mut ov)
                } else {
                    ReadFile(self.handle.0, buf.as_mut_ptr(), buf.len() as u32, null_mut(), &mut ov)
                };
                if ok == 0 {
                    let err = GetLastError();
                    if err != ERROR_IO_PENDING {
                        return Err(io::Error::from_raw_os_error(err as i32));
                    }
                    match WaitForSingleObject(self.event.0, timeout_ms) {
                        WAIT_OBJECT_0 => {}
                        WAIT_TIMEOUT => {
                            CancelIoEx(self.handle.0, &ov);
                            // Wait for the cancellation so `buf`/`ov` outlive the I/O.
                            GetOverlappedResult(self.handle.0, &ov, &mut n, 1);
                            return Ok(None);
                        }
                        _ => return Err(io::Error::last_os_error()),
                    }
                }
                if GetOverlappedResult(self.handle.0, &ov, &mut n, 0) == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(n as usize))
            }
        }

        /// Writes one output report: `[report_id, payload..]`, padded to the
        /// collection's output report length.
        pub fn write_report(&self, report_id: u8, payload: &[u8], timeout_ms: u32) -> io::Result<()> {
            let len = self.info.output_len.max(payload.len() + 1);
            let mut buf = vec![0u8; len];
            buf[0] = report_id;
            buf[1..1 + payload.len()].copy_from_slice(payload);
            match self.transfer(true, &mut buf, timeout_ms)? {
                Some(_) => Ok(()),
                None => Err(io::Error::new(io::ErrorKind::TimedOut, "HID write timed out")),
            }
        }

        /// Reads one input report (including the report ID byte). `None` on timeout.
        pub fn read_report(&self, timeout_ms: u32) -> io::Result<Option<Vec<u8>>> {
            let mut buf = vec![0u8; self.info.input_len.max(65)];
            Ok(self.transfer(false, &mut buf, timeout_ms)?.map(|n| {
                buf.truncate(n);
                buf
            }))
        }
    }
}

#[cfg(not(any(windows, feature = "check-win")))]
mod imp {
    use super::DeviceInfo;
    use std::io;

    pub fn enumerate() -> Vec<DeviceInfo> {
        Vec::new()
    }

    #[allow(dead_code)]
    pub struct HidDevice {
        pub info: DeviceInfo,
    }

    pub struct HidLock;

    pub fn lock(_: u32) -> Option<HidLock> {
        Some(HidLock)
    }

    impl HidDevice {
        pub fn open(_: &DeviceInfo) -> io::Result<Self> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "HID transport is Windows-only"))
        }
        pub fn write_report(&self, _: u8, _: &[u8], _: u32) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "HID transport is Windows-only"))
        }
        pub fn read_report(&self, _: u32) -> io::Result<Option<Vec<u8>>> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "HID transport is Windows-only"))
        }
    }
}

pub use imp::{enumerate, lock, HidDevice};

/// DrunkDeer vendor ID (gen-1 firmware).
pub const DRUNKDEER_VID: u16 = 0x352D;
/// Vendor-defined usage page carrying the config protocol.
pub const CONFIG_USAGE_PAGE: u16 = 0xFF00;

/// Config-interface candidates: DrunkDeer VID, vendor page, 64-byte output reports.
pub fn candidates() -> Vec<DeviceInfo> {
    enumerate()
        .into_iter()
        .filter(|d| d.vid == DRUNKDEER_VID && d.usage_page == CONFIG_USAGE_PAGE && d.output_len >= 64)
        .collect()
}

/// Gen-2 config interface (VID 0x19F5): the official driver opens the
/// collection with usage page 0x0001 / usage 0 (wired) and 0xFF00 / 1 (dongles,
/// not supported here). 64-byte reports without a report ID (65 with the 0 byte).
pub fn candidates_gen2() -> Vec<DeviceInfo> {
    enumerate()
        .into_iter()
        .filter(|d| d.vid == dd_proto::gen2::VID && d.usage_page == 0x0001 && d.usage == 0 && d.output_len >= 65)
        .collect()
}

#[allow(dead_code)]
pub fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "no HID transport")
}
