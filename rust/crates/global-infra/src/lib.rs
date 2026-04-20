// PR #883: `best_effort!`, `unrecoverable!` are #[macro_export] — they live
// at the crate root regardless of mod visibility, so their modules stay
// private. `panic_hook::install()` is the only non-macro export.
mod best_effort;
pub mod clock;
pub mod html;
pub mod ids;
pub mod json_error;
pub mod json_file;
mod panic_hook;
pub mod paths;
pub mod retry;
mod test_support;
mod unrecoverable;
pub mod uuid;

pub use json_error::SharedError;
pub use json_file::{load_json_file, sanitize_path_id, save_json_file};
pub use panic_hook::install as install_panic_hook;
#[doc(hidden)]
pub use test_support::{EnvVarGuard, PROCESS_ENV_LOCK};
