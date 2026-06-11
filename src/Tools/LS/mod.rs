use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::fs;
use std::path::Path;

pub struct LsTool;

impl Tool for LsTool {
    fn name(&self) -> &'static str {
        "LS"
    }

    fn description(&self) -> &'static str {
        "文件浏览与搜索 / File browser & search.  \
         action=list_dir(默认)|read_file|file_info|find_files|search_in_files  \
         list_dir: path, glob?  \
         read_file: path, offset?, limit?  \
         file_info: path  \
         find_files: path, pattern(glob), max?  \
         search_in_files: path, pattern(regex), file_glob?, max?"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "LS",
            "List files, read file contents, find files by glob, search text in files / 浏览文件、读取内容、搜索文件",
            &[
                ("action", "string", "Action: list_dir, read_file, file_info, find_files, search_in_files / 操作类型", true),
                ("path", "string", "Directory or file path / 路径", true),
                ("glob", "string", "Glob filter for list_dir (e.g. *.rs) / 通配过滤", false),
                ("pattern", "string", "Search pattern: glob for find_files, substring for search_in_files / 搜索模式", false),
                ("file_glob", "string", "Filter files by glob in search_in_files (e.g. *.py) / 文件过滤", false),
                ("offset", "string", "Start line number for read_file (1-indexed) / 起始行号", false),
                ("limit", "string", "Max lines for read_file, max results for find/search / 最大数量", false),
                ("max", "string", "Max results for find_files/search_in_files / 最大结果数", false),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let action = params
            .get("action")
            .map(|s| s.as_str())
            .unwrap_or("list_dir");
        match action {
            "list_dir" => list_dir(params),
            "read_file" => read_file(params),
            "file_info" => file_info(params),
            "find_files" => find_files(params),
            "search_in_files" => search_in_files(params),
            _ => ToolResult::err(format!("未知操作 / Unknown action: {action}")),
        }
    }
}

fn list_dir(params: &ToolParams) -> ToolResult {
    let path = try_tool!(param(params, "path"));
    let target = Path::new(&path);
    if !target.exists() {
        return ToolResult::err(format!("路径不存在 / Path not found: {path}"));
    }
    if target.is_file() {
        return ToolResult::ok_with_data(format!("文件 / File: {path}"), path);
    }

    let glob_filter = params.get("glob");
    let mut lines = Vec::new();
    let entries = match fs::read_dir(target) {
        Ok(e) => e,
        Err(e) => return ToolResult::err(format!("读取失败 / Read error: {e}")),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(g) = glob_filter {
            if !simple_glob(g, &name) {
                continue;
            }
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let meta = entry.metadata().ok();
        let size_str = meta
            .filter(|_| !is_dir)
            .map(|m| format!(" {:>8}", human_size(m.len())))
            .unwrap_or_default();
        let prefix = if is_dir { "📁 " } else { "📄 " };
        lines.push(format!("{prefix}{name}{size_str}"));
    }
    lines.sort();
    let count = lines.len();
    ToolResult::ok_with_data(
        format!("列出 {count} 项 / Listed {count} items in {path}"),
        lines.join("\n"),
    )
}

fn read_file(params: &ToolParams) -> ToolResult {
    let path = try_tool!(param(params, "path"));
    let target = Path::new(&path);
    if !target.exists() {
        return ToolResult::err(format!("文件不存在 / File not found: {path}"));
    }
    if target.is_dir() {
        return ToolResult::err(format!("是目录不是文件 / Is a directory: {path}"));
    }
    let content = match fs::read_to_string(target) {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("读取失败 / Read failed: {e}")),
    };
    let offset: usize = params
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);
    let limit: usize = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if offset > total {
        return ToolResult::err(format!("行号超出范围 / Offset {offset} > total {total}"));
    }
    let end = (offset - 1 + limit).min(total);
    let mut output = String::new();
    for (i, line) in lines[(offset - 1)..end].iter().enumerate() {
        output.push_str(&format!("{:>6}|{}\n", offset + i, line));
    }
    ToolResult::ok_with_data(
        format!("读取 {path}: 行{offset}-{end}/{total} / Lines {offset}-{end}/{total}"),
        output,
    )
}

fn file_info(params: &ToolParams) -> ToolResult {
    let path = try_tool!(param(params, "path"));
    let target = Path::new(&path);
    if !target.exists() {
        return ToolResult::err(format!("不存在 / Not found: {path}"));
    }
    let meta = match target.metadata() {
        Ok(m) => m,
        Err(e) => return ToolResult::err(format!("无法读取 / Cannot read: {e}")),
    };
    let is_dir = meta.is_dir();
    let kind = if is_dir { "目录/dir" } else { "文件/file" };
    let size = meta.len();
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("");
    let line_count = if !is_dir {
        fs::read_to_string(target)
            .map(|c| c.lines().count())
            .unwrap_or(0)
    } else {
        0
    };
    let modified = meta
        .modified()
        .map(|t| format!("{t:?}"))
        .unwrap_or_else(|_| "?".into());

    let info = format!(
        "路径/path: {path}\n类型/type: {kind} (.{ext})\n大小/size: {} ({size})\n行数/lines: {line_count}\n修改/modified: {modified}",
        human_size(size)
    );
    ToolResult::ok_with_data(format!("文件信息 / File info: {path}"), info)
}

fn find_files(params: &ToolParams) -> ToolResult {
    let path = try_tool!(param(params, "path"));
    let pattern = try_tool!(param(params, "pattern"));
    let max: usize = params
        .get("max")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let mut results = Vec::new();
    walk_find(Path::new(&path), &pattern, &mut results, max);
    if results.is_empty() {
        ToolResult::ok(format!("无匹配 / No match: {pattern} in {path}"))
    } else {
        ToolResult::ok_with_data(
            format!("找到 {} 个文件 / Found {} files", results.len(), results.len()),
            results.join("\n"),
        )
    }
}

fn walk_find(dir: &Path, pattern: &str, results: &mut Vec<String>, max: usize) {
    if results.len() >= max {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if results.len() >= max {
            break;
        }
        let fp = entry.path();
        let name = fp.file_name().unwrap_or_default().to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if name != "node_modules"
                && name != ".git"
                && name != "target"
                && !name.starts_with('.')
            {
                walk_find(&fp, pattern, results, max);
            }
        } else if simple_glob(pattern, &name) {
            results.push(fp.to_string_lossy().to_string());
        }
    }
}

fn search_in_files(params: &ToolParams) -> ToolResult {
    let path = try_tool!(param(params, "path"));
    let pattern = try_tool!(param(params, "pattern"));
    let file_glob = params.get("file_glob");
    let max: usize = params.get("max").and_then(|s| s.parse().ok()).unwrap_or(50);
    let mut results = Vec::new();
    grep_walk(
        Path::new(&path),
        &pattern,
        file_glob.map(|s| s.as_str()),
        &mut results,
        max,
    );
    if results.is_empty() {
        ToolResult::ok(format!("无匹配 / No match: {pattern} in {path}"))
    } else {
        ToolResult::ok_with_data(
            format!("找到 {} 处匹配 / Found {} matches", results.len(), results.len()),
            results.join("\n"),
        )
    }
}

fn grep_walk(
    dir: &Path,
    pattern: &str,
    file_glob: Option<&str>,
    results: &mut Vec<String>,
    max: usize,
) {
    if results.len() >= max {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if results.len() >= max {
            break;
        }
        let fp = entry.path();
        let name = fp.file_name().unwrap_or_default().to_string_lossy();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if name != "node_modules"
                && name != ".git"
                && name != "target"
                && !name.starts_with('.')
            {
                grep_walk(&fp, pattern, file_glob, results, max);
            }
        } else {
            if let Some(glob) = file_glob {
                if !simple_glob(glob, &name) {
                    continue;
                }
            }
            if let Ok(content) = fs::read_to_string(&fp) {
                for (i, line) in content.lines().enumerate() {
                    if line.contains(pattern) {
                        results.push(format!("{}:{}:{}", fp.display(), i + 1, line.trim()));
                        if results.len() >= max {
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn param(params: &ToolParams, key: &str) -> Result<String, ToolResult> {
    params
        .get(key)
        .cloned()
        .ok_or_else(|| ToolResult::err(format!("缺少参数 / Missing param: {key}")))
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut idx = 0;
    while size >= 1024.0 && idx < UNITS.len() - 1 {
        size /= 1024.0;
        idx += 1;
    }
    format!("{size:.1}{}", UNITS[idx])
}

fn simple_glob(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return name.contains(pattern);
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remaining = name;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match remaining.find(part) {
            Some(pos) => {
                if i == 0 && pos != 0 {
                    return false;
                }
                remaining = &remaining[pos + part.len()..];
            }
            None => return false,
        }
    }
    parts
        .last()
        .map(|l| l.is_empty() || remaining.is_empty())
        .unwrap_or(true)
}
