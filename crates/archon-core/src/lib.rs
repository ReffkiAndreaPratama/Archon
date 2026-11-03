//! Archon archive core: engine abstraction, ZIP engine, security and filesystem
//! helpers shared by the desktop application and the CLI.

pub mod engine;
pub mod error;
pub mod fsutil;
pub mod model;
pub mod progress;
pub mod security;
pub mod zip_engine;

pub use engine::{ArchiveEngine, EngineRegistry, detect_format};
pub use error::{ArchonError, ArchonResult};
pub use model::*;
pub use progress::{OperationContext, Progress, ProgressPhase};
