//! 集成测试：插件注册表（钩子/插件机制）的生命周期与失败隔离。
//!
//! SapNi 的 PLUGS 工具维护一个全局插件注册表（list / add / remove / enable /
//! disable）。这里验证钩子机制应有的不变量：
//!   - 注册后可在 list 中看到，且参数（版本/描述）被正确保存；
//!   - 重复注册同名插件被拒绝（幂等保护）；
//!   - 对未注册插件执行 remove / enable / disable 被拒绝（越权/不存在隔离）；
//!   - enable/disable 状态翻转正确；
//!   - 各用例使用唯一名，避免全局注册表在并行测试间相互污染。

use SapNi::Tools::{PLUGS::PlugsTool, Tool, ToolParams};
use std::sync::atomic::{AtomicU64, Ordering};

fn p(pairs: &[(&str, &str)]) -> ToolParams {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
fn uniq(prefix: &str) -> String {
    format!("{}_{}_{}", prefix, std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[test]
fn plugin_register_then_list_shows_it() {
    let tool = PlugsTool;
    let name = uniq("alpha");
    let add = tool.execute(&p(&[
        ("action", "add"),
        ("name", &name),
        ("path", "/tmp/alpha.plug"),
        ("version", "1.2.3"),
        ("description", "first plugin"),
    ]));
    assert!(add.success, "add: {}", add.message);

    let list = tool.execute(&p(&[("action", "list")]));
    assert!(list.success);
    let data = list.data.unwrap();
    assert!(data.contains(&name), "list should contain {name}:\n{data}");
    assert!(data.contains("1.2.3"), "version should be stored:\n{data}");
}

#[test]
fn plugin_duplicate_add_is_rejected() {
    let tool = PlugsTool;
    let name = uniq("dup");
    let first = tool.execute(&p(&[("action", "add"), ("name", &name), ("path", "/tmp/d.plug")]));
    assert!(first.success);
    let second = tool.execute(&p(&[("action", "add"), ("name", &name), ("path", "/tmp/d.plug")]));
    assert!(!second.success, "duplicate add must fail");
    assert!(second.message.contains("already") || second.message.contains("已存在"), "{}", second.message);
}

#[test]
fn plugin_remove_nonexistent_is_rejected() {
    let tool = PlugsTool;
    let ghost = uniq("ghost");
    let r = tool.execute(&p(&[("action", "remove"), ("name", &ghost)]));
    assert!(!r.success, "removing an unregistered plugin must fail");
    assert!(r.message.contains("not found") || r.message.contains("不存在"), "{}", r.message);
}

#[test]
fn plugin_enable_disable_state_transitions() {
    let tool = PlugsTool;
    let name = uniq("toggl");
    tool.execute(&p(&[("action", "add"), ("name", &name), ("path", "/tmp/t.plug")]));

    let dis = tool.execute(&p(&[("action", "disable"), ("name", &name)]));
    assert!(dis.success, "disable: {}", dis.message);
    assert!(dis.message.contains("disabled") || dis.message.contains("禁用"), "{}", dis.message);

    let en = tool.execute(&p(&[("action", "enable"), ("name", &name)]));
    assert!(en.success);
    assert!(en.message.contains("enabled") || en.message.contains("启用"), "{}", en.message);
}

#[test]
fn plugin_enable_nonexistent_is_rejected() {
    let tool = PlugsTool;
    let ghost = uniq("ghost_en");
    let r = tool.execute(&p(&[("action", "enable"), ("name", &ghost)]));
    assert!(!r.success, "enabling an unregistered plugin must fail (isolation)");
}

#[test]
fn plugin_remove_then_list_drops_it() {
    let tool = PlugsTool;
    let name = uniq("temp");
    tool.execute(&p(&[("action", "add"), ("name", &name), ("path", "/tmp/t.plug")]));
    let rm = tool.execute(&p(&[("action", "remove"), ("name", &name)]));
    assert!(rm.success, "remove: {}", rm.message);

    let list = tool.execute(&p(&[("action", "list")]));
    let data = list.data.unwrap_or_default();
    assert!(!data.contains(&name), "removed plugin must not appear in list:\n{data}");
}

#[test]
fn plugin_unknown_action_is_rejected() {
    let tool = PlugsTool;
    let r = tool.execute(&p(&[("action", "self_destruct")]));
    assert!(!r.success);
}
