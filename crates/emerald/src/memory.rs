//! Memory meter: per-tab and total memory from CEF's TaskManager API.
use crate::shell::Shell;
use cef::*;
use serde_json::{json, Value};

pub fn report(s: &Shell) -> Value {
    let Some(tm) = task_manager_get() else { return json!({"error": "task manager unavailable"}) };
    // The binding fills at most `ids.len()` entries, so size the vector first.
    let mut ids = vec![0i64; tm.tasks_count() + 16];
    tm.task_ids_list(Some(&mut ids));
    ids.retain(|id| *id != 0);
    let mut info_by_task = std::collections::HashMap::new();
    let mut total: i64 = 0;
    let mut by_type: std::collections::BTreeMap<String, i64> = Default::default();
    for id in &ids {
        if let Some((mem, ty, cpu)) = task_info(&tm, *id) {
            total += mem;
            *by_type.entry(ty.to_string()).or_default() += mem;
            info_by_task.insert(*id, (mem, String::new(), cpu));
        }
    }
    let mut tabs = vec![];
    for (tab, l) in &s.live {
        let Some(b) = &l.browser else { continue };
        let task = tm.task_id_for_browser_id(b.identifier());
        let (mem, _, cpu) = info_by_task.get(&task).cloned().unwrap_or((0, String::new(), 0.0));
        let t = s.session.tab(*tab);
        tabs.push(json!({"id": tab, "title": t.map(|t| t.title.clone()).unwrap_or_default(), "url": t.map(|t| t.url.clone()).unwrap_or_default(), "favicon": t.and_then(|t| t.favicon.clone()), "bytes": mem, "cpu": cpu, "task": task}));
    }
    tabs.sort_by_key(|t| -t["bytes"].as_i64().unwrap_or(0));
    let sleeping = s.session.tabs.len().saturating_sub(s.live.values().filter(|l| l.browser.is_some()).count());
    json!({ "total": total, "processes": ids.len(), "byType": by_type, "tabs": tabs, "sleeping": sleeping, "liveTabs": tabs.len() })
}

/// `TaskManager::task_info` in cef 156.7 fills a temporary copy and never returns it, so call
/// the C function directly. Returns (memory bytes, kind, cpu %).
fn task_info(tm: &TaskManager, id: i64) -> Option<(i64, &'static str, f64)> {
    use cef::sys::{_cef_task_info_t, cef_task_type_t as T};
    unsafe {
        let raw = tm.get_raw();
        let f = (*raw).get_task_info?;
        let mut info: _cef_task_info_t = std::mem::zeroed();
        info.size = std::mem::size_of::<_cef_task_info_t>();
        let ok = f(raw, id, &mut info);
        cef::sys::cef_string_utf16_clear(&mut info.title);
        if ok == 0 { return None; }
        let kind = match info.type_ {
            T::CEF_TASK_TYPE_BROWSER => "browser",
            T::CEF_TASK_TYPE_GPU => "gpu",
            T::CEF_TASK_TYPE_RENDERER => "renderer",
            T::CEF_TASK_TYPE_EXTENSION => "extension",
            T::CEF_TASK_TYPE_SERVICE_WORKER | T::CEF_TASK_TYPE_SHARED_WORKER | T::CEF_TASK_TYPE_DEDICATED_WORKER => "service_worker",
            T::CEF_TASK_TYPE_UTILITY | T::CEF_TASK_TYPE_ZYGOTE | T::CEF_TASK_TYPE_SANDBOX_HELPER => "utility",
            _ => "other",
        };
        Some((info.memory.max(0), kind, info.cpu_usage))
    }
}
