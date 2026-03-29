//! Voice control — SQLite persistence, TTS synthesis, STT transcription, and voice agent.

pub(crate) mod db;
pub(crate) mod db_usage;
pub(crate) mod intent;
pub(crate) mod streaming;
pub(crate) mod stt;
pub(crate) mod tts;
