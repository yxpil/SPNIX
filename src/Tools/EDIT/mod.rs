use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::fs;
use std::path::Path;

pub struct EditTool;

impl Tool for EditTool {
    fn name(&self) -> &'static str {
        "EDIT"
    }

    fn description(&self) -> &'static str {
        "编辑文件 / Edit file.  mode=replace: old+new 全文查找替换 / find & replace  \
         mode=line: action=replace|insert_before|insert_after|delete, line=N, content=...  \
         Params: path, mode(replace|line), old, new, line, end_line, content, action"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "EDIT",
            "Edit files: find & replace text, line-level insert/delete/replace / 编辑文件：查找替换、行级编辑",
            &[
                ("path", "string", "File path to edit / 文件路径", true),
                ("mode", "string", "Edit mode: replace or line / 编辑模式", true),
                ("old", "string", "Text to find (replace mode) / 被替换文本", false),
                ("new", "string", "Replacement text (replace mode) / 替换文本", false),
                ("action", "string", "Line action: replace, insert_before, insert_after, delete / 行操作", false),
                ("line", "string", "Line number (1-indexed) / 行号", false),
                ("end_line", "string", "End line for range delete / 结束行号", false),
                ("content", "string", "Content for line operations / 行操作内容", false),
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
            return ToolResult::err(format!("文件不存在 / File not found: {path}"));
        }
        if !target.is_file() {
            return ToolResult::err(format!("不是文件 / Not a file: {path}"));
        }

        let original = match fs::read_to_string(target) {
            Ok(s) => s,
            Err(e) => return ToolResult::err(format!("读取失败 / Read failed: {e}")),
        };

        let mode = params.get("mode").map(|s| s.as_str()).unwrap_or("replace");

        match mode {
            "replace" => edit_replace(path, &original, params),
            "line" => edit_line(path, &original, params),
            _ => ToolResult::err(format!(
                "未知模式 / Unknown mode: {mode} (use replace|line)"
            )),
        }
    }
}

fn edit_replace(path: &str, original: &str, params: &ToolParams) -> ToolResult {
    let old = match params.get("old") {
        Some(o) => o,
        None => return ToolResult::err("缺少参数 / Missing param: old"),
    };
    let new = params.get("new").map(|s| s.as_str()).unwrap_or("");

    let count = original.matches(old).count();
    if count == 0 {
        return ToolResult::err(format!("未找到匹配文本 / Pattern not found: {old}"));
    }

    let modified = original.replace(old, new);
    fs::write(path, &modified)
        .map(|_| {
            ToolResult::ok(format!(
                "替换成功 / Replaced {count} occurrence(s) in {path}"
            ))
        })
        .unwrap_or_else(|e| ToolResult::err(format!("写入失败 / Write failed: {e}")))
}

fn edit_line(path: &str, original: &str, params: &ToolParams) -> ToolResult {
    let action = params
        .get("action")
        .map(|s| s.as_str())
        .unwrap_or("replace");

    let lines: Vec<&str> = original.lines().collect();
    let total = lines.len();

    match action {
        "replace" => {
            let line_no = match parse_line_no(params, total) {
                Ok(n) => n,
                Err(e) => return e,
            };
            let content = match params.get("content") {
                Some(c) => c.as_str(),
                None => return ToolResult::err("缺少参数 / Missing param: content"),
            };
            let mut v: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            let old_line = v[line_no - 1].clone();
            v[line_no - 1] = content.to_string();
            let modified = v.join("\n");
            fs::write(path, &modified)
                .map(|_| {
                    ToolResult::ok(format!(
                        "替换第{line_no}行 / Replaced line {line_no}: {old_line} → {content}"
                    ))
                })
                .unwrap_or_else(|e| ToolResult::err(format!("写入失败 / Write failed: {e}")))
        }

        "insert_before" => {
            let line_no = match parse_line_no(params, total) {
                Ok(n) => n,
                Err(e) => return e,
            };
            let content = match params.get("content") {
                Some(c) => c.as_str(),
                None => return ToolResult::err("缺少参数 / Missing param: content"),
            };
            let mut v: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            v.insert(line_no - 1, content.to_string());
            let modified = v.join("\n");
            fs::write(path, &modified)
                .map(|_| {
                    ToolResult::ok(format!(
                        "在第{line_no}行前插入 / Inserted before line {line_no}: {content}"
                    ))
                })
                .unwrap_or_else(|e| ToolResult::err(format!("写入失败 / Write failed: {e}")))
        }

        "insert_after" => {
            let line_no = match parse_line_no(params, total) {
                Ok(n) => n,
                Err(e) => return e,
            };
            let content = match params.get("content") {
                Some(c) => c.as_str(),
                None => return ToolResult::err("缺少参数 / Missing param: content"),
            };
            let insert_at = if line_no >= total { total } else { line_no };
            let mut v: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            v.insert(insert_at, content.to_string());
            let modified = v.join("\n");
            fs::write(path, &modified)
                .map(|_| {
                    ToolResult::ok(format!(
                        "在第{line_no}行后插入 / Inserted after line {line_no}: {content}"
                    ))
                })
                .unwrap_or_else(|e| ToolResult::err(format!("写入失败 / Write failed: {e}")))
        }

        "delete" => {
            let line_no = match parse_line_no(params, total) {
                Ok(n) => n,
                Err(e) => return e,
            };
            let end_line = params
                .get("end_line")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(line_no)
                .min(total);

            if end_line < line_no {
                return ToolResult::err(format!(
                    "end_line({end_line}) 不能小于 line({line_no}) / end_line must be >= line"
                ));
            }

            let mut v: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            let deleted: Vec<String> = v.drain((line_no - 1)..end_line).collect();
            let modified = v.join("\n");

            let range_desc = if line_no == end_line {
                format!("{line_no}")
            } else {
                format!("{line_no}..{end_line}")
            };

            fs::write(path, &modified)
                .map(|_| {
                    ToolResult::ok(format!(
                        "删除第{range_desc}行 / Deleted line(s) {range_desc}: {}",
                        deleted.join(" | ")
                    ))
                })
                .unwrap_or_else(|e| ToolResult::err(format!("写入失败 / Write failed: {e}")))
        }

        _ => ToolResult::err(format!(
            "未知操作 / Unknown action: {action} (use replace|insert_before|insert_after|delete)"
        )),
    }
}

fn parse_line_no(params: &ToolParams, total: usize) -> Result<usize, ToolResult> {
    let val = match params.get("line") {
        Some(v) => v,
        None => {
            return Err(ToolResult::err(
                "缺少参数 / Missing param: line".to_string(),
            ));
        }
    };
    let n: usize = val
        .parse()
        .map_err(|_| ToolResult::err(format!("行号无效 / Invalid line number: {val}")))?;
    if n < 1 || n > total {
        return Err(ToolResult::err(format!(
            "行号超出范围 / Line out of range: {n} (total={total})"
        )));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(map: &[(&str, &str)]) -> ToolParams {
        map.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_line_replace() {
        let tmp = "/tmp/sapni_edit_test.txt";
        fs::write(tmp, "line1\nline2\nline3\n").unwrap();
        let tool = EditTool;
        let params = p(&[
            ("path", tmp),
            ("mode", "line"),
            ("action", "replace"),
            ("line", "2"),
            ("content", "NEW_LINE_2"),
        ]);
        let r = tool.execute(&params);
        assert!(r.success, "{}", r.message);
        let result = fs::read_to_string(tmp).unwrap();
        assert_eq!(result, "line1\nNEW_LINE_2\nline3");
        fs::remove_file(tmp).ok();
    }

    #[test]
    fn test_line_out_of_range() {
        let tmp = "/tmp/sapni_edit_test6.txt";
        fs::write(tmp, "A\nB\n").unwrap();
        let tool = EditTool;
        let params = p(&[
            ("path", tmp),
            ("mode", "line"),
            ("action", "replace"),
            ("line", "99"),
            ("content", "X"),
        ]);
        let r = tool.execute(&params);
        assert!(!r.success);
        fs::remove_file(tmp).ok();
    }

    #[test]
    fn test_replace_mode() {
        let tmp = "/tmp/sapni_edit_test7.txt";
        fs::write(tmp, "hello world\n").unwrap();
        let tool = EditTool;
        let params = p(&[
            ("path", tmp),
            ("mode", "replace"),
            ("old", "world"),
            ("new", "Rust"),
        ]);
        let r = tool.execute(&params);
        assert!(r.success, "{}", r.message);
        let result = fs::read_to_string(tmp).unwrap();
        assert_eq!(result, "hello Rust\n");
        fs::remove_file(tmp).ok();
    }
}
