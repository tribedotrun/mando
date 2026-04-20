//! transport-http I/O tier.
//!
//! The HTTP transport currently keeps provider-facing work inside delegated
//! domain calls, so this tier is intentionally present as part of the crate
//! envelope even when light.
