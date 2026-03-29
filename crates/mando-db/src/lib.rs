//! Unified SQLite database for Mando — single `mando.db` file, all tables,
//! compile-time checked queries via sqlx.

pub mod caller;
pub mod pool;
pub mod queries;

pub use caller::SessionCaller;
pub use pool::Db;
