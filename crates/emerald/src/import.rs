//! Importers (implemented in phase 3).
use crate::shell::Shell;
use anyhow::{bail, Result};
use serde_json::{json, Value};

pub fn detect() -> Value { json!([]) }
pub fn run(_s: &mut Shell, _a: &Value) -> Result<Value> { bail!("import not available yet") }
pub fn run_file(_s: &mut Shell, _a: &Value) -> Result<Value> { bail!("import not available yet") }
