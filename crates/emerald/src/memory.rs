//! Memory meter: per-tab and total memory from CEF's TaskManager API.
use crate::shell::Shell;
use cef::*;
use serde_json::{json, Value};

pub fn report(s: &Shell) -> Value {
    let Some(tm) = task_manager_get() else { return json!({"error": "task manager unavailable"}) };
    let mut ids = Vec::new();
    tm.task_ids_list(Some(&mut ids));
    let mut info_by_task = std::collections::HashMap::new();
    let mut total: i64 = 0;
    let mut by_type: std::collections::BTreeMap<String, i64> = Default::default();
    for id in &ids {
        let mut info = TaskInfo::default();
        if tm.task_info(*id, Some(&mut info)) != 0 {
            let mem = info.memory.max(0);
            total += mem;
            let ty = format!("{:?}", info.type_).to_lowercase();
            *by_type.entry(ty).or_default() += mem;
            info_by_task.insert(*id, (mem, crate::util::cs(&info.title.to_string()).to_string(), info.cpu_usage));
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
