use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::collections::HashMap;
use std::sync::Mutex;

/// 插件注册表（全局单例）
static PLUGINS: std::sync::LazyLock<Mutex<HashMap<String, PluginInfo>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    #[allow(dead_code)]
    pub path: String,
}

pub struct PlugsTool;

impl Tool for PlugsTool {
    fn name(&self) -> &'static str {
        "PLUGS"
    }

    fn description(&self) -> &'static str {
        "插件管理 / Plugin management. Params: action (list|enable|disable|add|remove), name (required for add/remove/enable/disable), path (required for add)"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "PLUGS",
            "Manage registered plugins: list, add, remove, enable, disable / 插件管理：注册、启用、禁用",
            &[
                ("action", "string", "Action: list, add, remove, enable, disable / 操作", true),
                ("name", "string", "Plugin name (required for add/remove/enable/disable) / 插件名", false),
                ("path", "string", "Plugin path (required for add) / 插件路径", false),
                ("version", "string", "Plugin version (for add, default 0.1.0) / 版本号", false),
                ("description", "string", "Plugin description / 描述", false),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let action = params.get("action").map(|s| s.as_str()).unwrap_or("list");

        match action {
            "list" => {
                let registry = PLUGINS.lock().unwrap();
                if registry.is_empty() {
                    return ToolResult::ok("没有已注册的插件 / No plugins registered");
                }
                let mut lines: Vec<String> = registry
                    .values()
                    .map(|p| {
                        let status = if p.enabled { "✅" } else { "⛔" };
                        format!(
                            "{status} {name} v{ver} — {desc}",
                            name = p.name,
                            ver = p.version,
                            desc = p.description
                        )
                    })
                    .collect();
                lines.sort();
                ToolResult::ok_with_data(
                    format!("已注册 {} 个插件 / {} plugin(s) registered", lines.len(), lines.len()),
                    lines.join("\n"),
                )
            }

            "add" => {
                let name = match params.get("name") {
                    Some(n) => n.clone(),
                    None => return ToolResult::err("缺少参数 / Missing param: name"),
                };
                let path = match params.get("path") {
                    Some(p) => p.clone(),
                    None => return ToolResult::err("缺少参数 / Missing param: path"),
                };
                let desc = params.get("description").cloned().unwrap_or_default();
                let ver = params
                    .get("version")
                    .cloned()
                    .unwrap_or_else(|| "0.1.0".into());

                let mut registry = PLUGINS.lock().unwrap();
                if registry.contains_key(&name) {
                    return ToolResult::err(format!(
                        "插件已存在 / Plugin already registered: {name}"
                    ));
                }
                registry.insert(
                    name.clone(),
                    PluginInfo {
                        name: name.clone(),
                        version: ver,
                        description: desc,
                        enabled: true,
                        path,
                    },
                );
                ToolResult::ok(format!("插件已注册 / Plugin registered: {name}"))
            }

            "remove" => {
                let name = match params.get("name") {
                    Some(n) => n.clone(),
                    None => return ToolResult::err("缺少参数 / Missing param: name"),
                };
                let mut registry = PLUGINS.lock().unwrap();
                if registry.remove(&name).is_some() {
                    ToolResult::ok(format!("插件已移除 / Plugin removed: {name}"))
                } else {
                    ToolResult::err(format!("插件不存在 / Plugin not found: {name}"))
                }
            }

            "enable" | "disable" => {
                let name = match params.get("name") {
                    Some(n) => n.clone(),
                    None => return ToolResult::err("缺少参数 / Missing param: name"),
                };
                let enable = action == "enable";
                let mut registry = PLUGINS.lock().unwrap();
                match registry.get_mut(&name) {
                    Some(plugin) => {
                        plugin.enabled = enable;
                        let status = if enable {
                            "启用 / enabled"
                        } else {
                            "禁用 / disabled"
                        };
                        ToolResult::ok(format!("插件已{status}: {name}"))
                    }
                    None => ToolResult::err(format!("插件不存在 / Plugin not found: {name}")),
                }
            }

            _ => ToolResult::err(format!(
                "未知操作 / Unknown action: {action} (use list|add|remove|enable|disable)"
            )),
        }
    }
}
