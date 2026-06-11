use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::fs;
use std::path::Path;

pub struct WriteTool;

impl Tool for WriteTool {
    fn name(&self) -> &'static str {
        "WRITE"
    }

    fn description(&self) -> &'static str {
        "文件操作 / File operations.  \
         action=write(默认)|copy|move  \
         write: path, content (自动创建父目录)  \
         copy: source, target  \
         move: source, target"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "WRITE",
            "Write/create files, copy files/dirs, move/rename files / 写入、复制、移动文件",
            &[
                ("action", "string", "Action: write, copy, move / 操作类型", true),
                ("path", "string", "Target file path for write action / 写入目标路径", false),
                ("content", "string", "File content for write action / 写入内容", false),
                ("source", "string", "Source path for copy/move / 源路径", false),
                ("target", "string", "Destination path for copy/move / 目标路径", false),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let action = params.get("action").map(|s| s.as_str()).unwrap_or("write");
        match action {
            "write" => write_file(params),
            "copy" => copy_file(params),
            "move" => move_file(params),
            _ => ToolResult::err(format!(
                "未知操作 / Unknown action: {action} (use write|copy|move)"
            )),
        }
    }
}

fn write_file(params: &ToolParams) -> ToolResult {
    let path = try_tool!(get(params, "path"));
    let content = try_tool!(get(params, "content"));
    let target = Path::new(&path);

    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult::err(format!("创建目录失败 / Failed to create dirs: {e}"));
            }
        }
    }

    match fs::write(target, &content) {
        Ok(_) => ToolResult::ok(format!(
            "写入成功 / Written: {path} ({} chars)",
            content.len()
        )),
        Err(e) => ToolResult::err(format!("写入失败 / Write failed: {e}")),
    }
}

fn copy_file(params: &ToolParams) -> ToolResult {
    let source = try_tool!(get(params, "source"));
    let target = try_tool!(get(params, "target"));
    let src = Path::new(&source);
    let dst = Path::new(&target);

    if !src.exists() {
        return ToolResult::err(format!("源文件不存在 / Source not found: {source}"));
    }
    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult::err(format!("创建目录失败 / Failed to create dirs: {e}"));
            }
        }
    }

    if src.is_dir() {
        match copy_dir_recursive(src, dst) {
            Ok(_) => ToolResult::ok(format!("目录复制成功 / Dir copied: {source} → {target}")),
            Err(e) => e,
        }
    } else {
        match fs::copy(src, dst) {
            Ok(_) => ToolResult::ok(format!("复制成功 / Copied: {source} → {target}")),
            Err(e) => ToolResult::err(format!("复制失败 / Copy failed: {e}")),
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), ToolResult> {
    if let Err(e) = fs::create_dir_all(dst) {
        return Err(ToolResult::err(format!(
            "创建目录失败 / Failed to create dirs: {e}"
        )));
    }

    let entries = match fs::read_dir(src) {
        Ok(e) => e,
        Err(e) => {
            return Err(ToolResult::err(format!(
                "读取目录失败 / Read dir failed: {e}"
            )));
        }
    };

    for entry in entries.flatten() {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if let Err(e) = fs::copy(&src_path, &dst_path) {
            return Err(ToolResult::err(format!("复制失败 / Copy failed: {e}")));
        }
    }
    Ok(())
}

fn move_file(params: &ToolParams) -> ToolResult {
    let source = try_tool!(get(params, "source"));
    let target = try_tool!(get(params, "target"));
    let src = Path::new(&source);
    let dst = Path::new(&target);

    if !src.exists() {
        return ToolResult::err(format!("源文件不存在 / Source not found: {source}"));
    }
    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult::err(format!("创建目录失败 / Failed to create dirs: {e}"));
            }
        }
    }

    match fs::rename(src, dst) {
        Ok(_) => ToolResult::ok(format!("移动成功 / Moved: {source} → {target}")),
        Err(e) => ToolResult::err(format!("移动失败 / Move failed: {e}")),
    }
}

fn get(params: &ToolParams, key: &str) -> Result<String, ToolResult> {
    params
        .get(key)
        .cloned()
        .ok_or_else(|| ToolResult::err(format!("缺少参数 / Missing param: {key}")))
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
    fn test_write() {
        let tool = WriteTool;
        let tmp = "/tmp/sapni_write_test.txt";
        let r = tool.execute(&p(&[
            ("action", "write"),
            ("path", tmp),
            ("content", "hello sapni"),
        ]));
        assert!(r.success);
        let content = std::fs::read_to_string(tmp).unwrap();
        assert_eq!(content, "hello sapni");
        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn test_copy() {
        let tool = WriteTool;
        let src = "/tmp/sapni_copy_src.txt";
        let dst = "/tmp/sapni_copy_dst.txt";
        std::fs::write(src, "copy me").unwrap();
        let r = tool.execute(&p(&[("action", "copy"), ("source", src), ("target", dst)]));
        assert!(r.success);
        assert_eq!(std::fs::read_to_string(dst).unwrap(), "copy me");
        std::fs::remove_file(src).ok();
        std::fs::remove_file(dst).ok();
    }

    #[test]
    fn test_move() {
        let tool = WriteTool;
        let src = "/tmp/sapni_move_src.txt";
        let dst = "/tmp/sapni_move_dst.txt";
        std::fs::write(src, "move me").unwrap();
        let r = tool.execute(&p(&[("action", "move"), ("source", src), ("target", dst)]));
        assert!(r.success);
        assert!(std::fs::metadata(dst).is_ok());
        assert!(std::fs::metadata(src).is_err());
        std::fs::remove_file(dst).ok();
    }
}
