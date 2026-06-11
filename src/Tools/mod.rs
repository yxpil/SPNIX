/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
    pub data: Option<String>,
}

impl ToolResult {
    pub fn ok(msg: impl Into<String>) -> Self {
        Self {
            success: true,
            message: msg.into(),
            data: None,
        }
    }

    pub fn ok_with_data(msg: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            success: true,
            message: msg.into(),
            data: Some(data.into()),
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            message: msg.into(),
            data: None,
        }
    }
}

/// try! 宏替代 ? — 从 Result<T, ToolResult> 提前返回错误
#[macro_export]
macro_rules! try_tool {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return e,
        }
    };
}

/// 工具参数（键值对）
pub type ToolParams = std::collections::HashMap<String, String>;

/// OpenAI 兼容的工具 schema
pub type ToolSchema = serde_json::Value;

/// 辅助：用简单参数列表构建 OpenAI function schema
pub fn make_schema(
    name: &str,
    desc: &str,
    params: &[(&str, &str, &str, bool)], // (name, type, description, required)
) -> ToolSchema {
    let properties: serde_json::Map<String, serde_json::Value> = params
        .iter()
        .map(|(n, t, d, _)| {
            (
                n.to_string(),
                serde_json::json!({"type": t, "description": d}),
            )
        })
        .collect();
    let required: Vec<String> = params
        .iter()
        .filter(|(_, _, _, req)| *req)
        .map(|(n, _, _, _)| n.to_string())
        .collect();

    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": desc,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            }
        }
    })
}

/// 所有工具必须实现的 trait
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// 返回 OpenAI 兼容的 function schema
    fn schema(&self) -> ToolSchema;
    fn execute(&self, params: &ToolParams) -> ToolResult;
}

// 重新导出各工具
#[allow(non_snake_case)]
pub mod CHECK;
#[allow(non_snake_case)]
pub mod DEL;
#[allow(non_snake_case)]
pub mod EDIT;
#[allow(non_snake_case)]
pub mod EXEC;
#[allow(non_snake_case)]
pub mod LS;
#[allow(non_snake_case)]
pub mod PLUGS;
#[allow(non_snake_case)]
pub mod Todo;
#[allow(non_snake_case)]
pub mod WRITE;
