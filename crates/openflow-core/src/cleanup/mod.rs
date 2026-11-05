//! Native cleanup boundary types. Jac owns prompt construction, output policy,
//! token budgets, formatting, and acceptance; Rust holds only adapter inputs.


use serde::{Deserialize, Serialize};

/// Native facts supplied to Jac's cleanup bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupContext {
    pub app_bundle_id: Option<String>,
    pub window_context: Option<String>,
}

impl Default for CleanupContext {
    fn default() -> Self {
        Self {
            app_bundle_id: None,
            window_context: None,
        }
    }
}

/// A Jac-assembled chat turn passed unchanged to a native provider adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupMsg {
    pub role: &'static str,
    pub content: String,
}
