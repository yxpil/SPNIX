use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::fs;
use std::path::Path;

pub struct DelTool;

impl Tool for DelTool {
    fn name(&self) -> &'static str {
        "DEL"
    }

    fn description(&self) -> &'static str {
        "删除文件或目录 / Delete file or directory. Params: path (required)"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "DEL",
            "Delete a file or directory / 删除文件或目录",
            &[
                ("path", "string", "Path to delete / 要删除的路径", true),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let path = match params.get("path") {
            Some(p) => p,
            None => return ToolResult::err("缺少参数 / Missing param: path"),
        };

        let target = Path::new(path);

        if !target.exists() {
            return ToolResult::err(format!("路径不存在 / Path not found: {path}"));
        }

        let is_dir = target.is_dir();
        let result = if is_dir {
            fs::remove_dir_all(target)
        } else {
            fs::remove_file(target)
        };

        match result {
            Ok(_) => {
                let kind = if is_dir {
                    "目录 / dir"
                } else {
                    "文件 / file"
                };
                ToolResult::ok(format!("已删除 {kind}: {path}"))
            }
            Err(e) => ToolResult::err(format!("删除失败 / Delete failed: {e}")),
        }
    }
}
