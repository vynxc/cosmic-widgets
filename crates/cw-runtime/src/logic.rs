use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hard resource ceilings for a widget logic instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicLimits {
    pub memory_pages: u32,
    pub fuel: u64,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

impl Default for LogicLimits {
    fn default() -> Self {
        Self {
            memory_pages: 256,
            fuel: 5_000_000,
            timeout_ms: 50,
            max_output_bytes: 1024 * 1024,
        }
    }
}

/// Failure contained to one widget logic instance.
#[derive(Debug, Error)]
pub enum LogicError {
    #[error("logic engine is unavailable in this build")]
    Unavailable,
    #[error("unable to compile widget logic: {0}")]
    Compile(String),
    #[error("widget function is missing: {0}")]
    MissingFunction(String),
    #[error("widget logic call failed: {0}")]
    Call(String),
    #[error("widget logic returned {actual} bytes, exceeding the {limit} byte limit")]
    OutputTooLarge { actual: usize, limit: usize },
}

/// Runtime-neutral factory for stateful widget logic.
pub trait WidgetLogicEngine {
    /// Compiles and creates a stateful plugin using host-supplied ceilings.
    ///
    /// # Errors
    ///
    /// Returns [`LogicError::Compile`] when the module or runtime configuration is invalid.
    fn instantiate(
        &self,
        wasm: &[u8],
        limits: LogicLimits,
    ) -> Result<Box<dyn WidgetLogicInstance>, LogicError>;
}

/// One stateful package instance. Calls are deliberately serialized through `&mut self`.
pub trait WidgetLogicInstance {
    /// Reports whether the module exports an application-level entrypoint.
    fn has_function(&self, function: &str) -> bool;
    /// Calls an entrypoint with a versioned CBOR message.
    ///
    /// # Errors
    ///
    /// Returns an error when the export is missing, traps, exceeds its limits, or
    /// emits a response larger than the configured ceiling.
    fn call(&mut self, function: &str, cbor: &[u8]) -> Result<Vec<u8>, LogicError>;
}

#[cfg(feature = "extism-runtime")]
/// Lazy Extism engine with WASI and built-in host access disabled.
pub struct ExtismEngine {
    cache_config: Option<PathBuf>,
}

#[cfg(feature = "extism-runtime")]
impl ExtismEngine {
    /// Creates an engine using an optional Wasmtime cache configuration file.
    pub fn new(cache_config: Option<PathBuf>) -> Self {
        Self { cache_config }
    }

    /// Uses a cache configuration only when the file exists.
    pub fn with_cache_if_present(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        Self::new(path.exists().then(|| path.to_path_buf()))
    }
}

#[cfg(feature = "extism-runtime")]
impl WidgetLogicEngine for ExtismEngine {
    fn instantiate(
        &self,
        wasm: &[u8],
        limits: LogicLimits,
    ) -> Result<Box<dyn WidgetLogicInstance>, LogicError> {
        let manifest = extism::Manifest::new([extism::Wasm::data(wasm)])
            .with_memory_max(limits.memory_pages)
            .with_timeout(Duration::from_millis(limits.timeout_ms));
        let mut builder = extism::PluginBuilder::new(manifest)
            .with_wasi(false)
            .with_fuel_limit(limits.fuel);
        if let Some(cache_config) = &self.cache_config {
            builder = builder.with_cache_config(cache_config);
        } else {
            builder = builder.with_cache_disabled();
        }
        let plugin = builder
            .build()
            .map_err(|error| LogicError::Compile(error.to_string()))?;
        Ok(Box::new(ExtismInstance {
            plugin,
            max_output_bytes: limits.max_output_bytes,
        }))
    }
}

#[cfg(feature = "extism-runtime")]
struct ExtismInstance {
    plugin: extism::Plugin,
    max_output_bytes: usize,
}

#[cfg(feature = "extism-runtime")]
impl WidgetLogicInstance for ExtismInstance {
    fn has_function(&self, function: &str) -> bool {
        self.plugin.function_exists(function)
    }

    fn call(&mut self, function: &str, cbor: &[u8]) -> Result<Vec<u8>, LogicError> {
        if !self.has_function(function) {
            return Err(LogicError::MissingFunction(function.into()));
        }
        let output = self
            .plugin
            .call::<&[u8], &[u8]>(function, cbor)
            .map_err(|error| LogicError::Call(error.to_string()))?;
        if output.len() > self.max_output_bytes {
            return Err(LogicError::OutputTooLarge {
                actual: output.len(),
                limit: self.max_output_bytes,
            });
        }
        Ok(output.to_vec())
    }
}
