//! Process ID newtype.
//!
//! Wraps `u32` so the compiler can distinguish worker PIDs from arbitrary
//! integers in function signatures and struct fields. Serializes transparently
//! as a plain number so existing persisted state (pid_registry, health_store)
//! round-trips without migration.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pid(pub u32);

impl Pid {
    /// Construct a Pid from a raw u32. Use this at the OS boundary where the
    /// raw value comes from `std::process::id()` or `libc::fork()` output.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Raw u32 value for use with libc syscalls that require it.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Raw i32 value for `libc::kill` and similar syscalls that take signed pids.
    pub const fn as_i32(self) -> i32 {
        self.0 as i32
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<u32> for Pid {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<Pid> for u32 {
    fn from(p: Pid) -> Self {
        p.0
    }
}
