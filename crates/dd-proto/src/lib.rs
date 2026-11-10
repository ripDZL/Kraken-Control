//! DrunkDeer keyboard wire protocol.
//!
//! Scope (v0.1): gen-1 firmware with the "new high precision" dialect —
//! A75 Master (type bytes `0b 05 04`) and A75 Ultra (`0b 04 04`).
//! Every format here is taken from the official web driver
//! (beta.drunkdeer-antler.com, `index.exKUY8vl.js`); see
//! `docs/official-driver-a75-master-ultra.md` in the C# fork for line refs.
//!
//! `gen2`: second-generation boards (VID 0x19F5, e.g. 2025 A75 Ultra), from
//! the drunkdeer.keybord.net.cn driver.
//!
//! This crate is pure (no I/O). Transport lives in the app crate.

pub mod catalog;
pub mod gen2;
pub mod keymap;
pub mod led;
pub mod model;
pub mod packets;
pub mod pattern;
pub mod rtp;

pub use keymap::{KeyAction, Keymap, Layer, SLOTS};
pub use model::{Identity, KeyDef, Model};
