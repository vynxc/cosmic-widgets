//! Reactive data and optional Extism logic runtime.

mod logic;
mod provider;

#[cfg(feature = "extism-runtime")]
pub use logic::ExtismEngine;
pub use logic::{LogicError, LogicLimits, WidgetLogicEngine, WidgetLogicInstance};
pub use provider::{DataPatch, ProviderRegistry};
