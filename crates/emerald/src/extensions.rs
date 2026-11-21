//! Extension glue (expanded in phase 4).
use crate::shell::Shell;
use cef::*;
use emerald_core::session::Id;
use serde_json::{json, Value};

#[derive(Default)]
pub struct ExtState {
    pub pending_overlay: Option<(String, Value)>,
    pub pinned: Vec<String>,
}

impl ExtState {
    pub fn pinned_json(&self) -> Value { json!([]) }
    pub fn shutdown(&mut self) {}
}

pub fn on_page_loaded(_s: &mut Shell, _tab: Id, _b: &Browser, _url: &str) {}

pub fn list_json(_s: &Shell) -> Value { json!([]) }
pub fn action(_s: &mut Shell, _a: &Value) -> anyhow::Result<Value> { anyhow::bail!("extensions manager not available yet") }
pub fn open_popup(_s: &mut Shell, _id: &str, _x: f64, _y: f64) {}
