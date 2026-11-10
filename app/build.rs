//! Windows (MSVC): builds a compiled resource file (.res) with the app icon
//! and version info, and hands it to the linker. Written directly in the
//! documented .res format, so no rc.exe / extra crates are needed.

use std::env;
use std::fs;
use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
const LANG_EN_US: u16 = 0x0409;

fn main() {
    println!("cargo:rerun-if-changed=res/kraken.ico");
    println!("cargo:rerun-if-changed=build.rs");
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || target_env != "msvc" {
        return;
    }
    let ico = fs::read("res/kraken.ico").expect("res/kraken.ico");
    let res = build_res(&ico, &env::var("CARGO_PKG_VERSION").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("kraken.res");
    fs::write(&out, res).expect("write .res");
    // link.exe accepts .res inputs directly (it runs cvtres itself).
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

fn u16le(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn u32le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn pad4(v: &mut Vec<u8>) {
    v.resize(v.len().next_multiple_of(4), 0);
}

fn wstr(v: &mut Vec<u8>, s: &str) {
    for c in s.encode_utf16().chain(Some(0)) {
        u16le(v, c);
    }
}

/// One RESOURCEHEADER + data (ordinal type and name).
fn resource(out: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
    u32le(out, data.len() as u32);
    u32le(out, 32); // header size
    u16le(out, 0xFFFF);
    u16le(out, kind);
    u16le(out, 0xFFFF);
    u16le(out, id);
    u32le(out, 0); // data version
    u16le(out, flags);
    u16le(out, if kind == 0 { 0 } else { LANG_EN_US });
    u32le(out, 0); // version
    u32le(out, 0); // characteristics
    out.extend_from_slice(data);
    pad4(out);
}

fn build_res(ico: &[u8], version: &str) -> Vec<u8> {
    let rd16 = |o: usize| u16::from_le_bytes([ico[o], ico[o + 1]]);
    let rd32 = |o: usize| u32::from_le_bytes([ico[o], ico[o + 1], ico[o + 2], ico[o + 3]]);
    assert_eq!((rd16(0), rd16(2)), (0, 1), "not an .ico");
    let count = rd16(4) as usize;

    let mut out = Vec::new();
    resource(&mut out, 0, 0, 0, &[]); // mandatory empty first entry

    // RT_ICON per image; RT_GROUP_ICON = ICONDIR with 14-byte entries (id instead of offset).
    let mut group = Vec::new();
    u16le(&mut group, 0);
    u16le(&mut group, 1);
    u16le(&mut group, count as u16);
    for i in 0..count {
        let e = 6 + 16 * i;
        let size = rd32(e + 8) as usize;
        let offset = rd32(e + 12) as usize;
        resource(&mut out, RT_ICON, i as u16 + 1, 0x1010, &ico[offset..offset + size]);
        group.extend_from_slice(&ico[e..e + 12]);
        u16le(&mut group, i as u16 + 1);
    }
    resource(&mut out, RT_GROUP_ICON, 1, 0x1030, &group);
    resource(&mut out, RT_VERSION, 1, 0x0030, &version_info(version));
    out
}

/// VS_VERSIONINFO node: header, key, aligned value, aligned children.
fn node(key: &str, kind: u16, value: &[u8], value_len: u16, children: &[Vec<u8>]) -> Vec<u8> {
    let mut v = vec![0, 0];
    u16le(&mut v, value_len);
    u16le(&mut v, kind);
    wstr(&mut v, key);
    pad4(&mut v);
    v.extend_from_slice(value);
    for c in children {
        pad4(&mut v);
        v.extend_from_slice(c);
    }
    let len = v.len() as u16;
    v[..2].copy_from_slice(&len.to_le_bytes());
    v
}

fn string(key: &str, value: &str) -> Vec<u8> {
    let mut val = Vec::new();
    wstr(&mut val, value);
    node(key, 1, &val, (val.len() / 2) as u16, &[])
}

fn version_info(version: &str) -> Vec<u8> {
    let mut parts = version.split(['.', '-']).map(|p| p.parse::<u16>().unwrap_or(0));
    let (a, b, c) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let ms = ((a as u32) << 16) | b as u32;
    let ls = (c as u32) << 16;

    let mut fixed = Vec::new();
    for x in [
        0xFEEF04BD, // signature
        0x0001_0000,
        ms,
        ls, // file version
        ms,
        ls,          // product version
        0x3F,        // flags mask
        0,           // flags
        0x0004_0004, // VOS_NT_WINDOWS32
        1,           // VFT_APP
        0,
        0,
        0,
    ] {
        u32le(&mut fixed, x);
    }

    let table = node(
        "040904B0",
        1,
        &[],
        0,
        &[
            string("CompanyName", "Kraken Control"),
            string("FileDescription", "Kraken Control"),
            string("FileVersion", version),
            string("InternalName", "kraken-control"),
            string("OriginalFilename", "kraken-control.exe"),
            string("ProductName", "Kraken Control"),
            string("ProductVersion", version),
        ],
    );
    let strings = node("StringFileInfo", 1, &[], 0, &[table]);
    let mut translation = Vec::new();
    u16le(&mut translation, LANG_EN_US);
    u16le(&mut translation, 1200); // Unicode
    let var = node("Translation", 0, &translation, 4, &[]);
    let vars = node("VarFileInfo", 1, &[], 0, &[var]);
    node("VS_VERSION_INFO", 0, &fixed, fixed.len() as u16, &[strings, vars])
}
