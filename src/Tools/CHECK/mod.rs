use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::path::Path;

pub struct CheckTool;

impl Tool for CheckTool {
    fn name(&self) -> &'static str {
        "CHECK"
    }

    fn description(&self) -> &'static str {
        "校验文件 / Validate file existence, syntax, or content. Params: path (required), mode (exists|syntax|size, default: exists)"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "CHECK",
            "Validate file: check existence, syntax (rust/python/json/toml/js/ts/html/c), or size / 校验文件：存在性、语法、大小",
            &[
                ("path", "string", "File path to check / 文件路径", true),
                ("mode", "string", "Check mode: exists, syntax, size / 检查模式", false),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let path = match params.get("path") {
            Some(p) => p,
            None => return ToolResult::err("缺少参数 / Missing param: path"),
        };

        let mode = params.get("mode").map(|s| s.as_str()).unwrap_or("exists");
        let target = Path::new(path);

        match mode {
            "exists" => {
                if target.exists() {
                    let kind = if target.is_dir() {
                        "目录 / directory"
                    } else {
                        "文件 / file"
                    };
                    ToolResult::ok(format!("存在 / {kind}: {path}"))
                } else {
                    ToolResult::err(format!("不存在 / Not found: {path}"))
                }
            }

            "size" => match target.metadata() {
                Ok(meta) => {
                    if meta.is_dir() {
                        ToolResult::err(format!("是目录不是文件 / Is a directory: {path}"))
                    } else {
                        let size = meta.len();
                        let human = human_size(size);
                        ToolResult::ok_with_data(format!("大小 / Size: {human}"), format!("{size}"))
                    }
                }
                Err(e) => ToolResult::err(format!("无法读取 / Cannot read: {e}")),
            },

            "syntax" => {
                if !target.exists() {
                    return ToolResult::err(format!("文件不存在 / File not found: {path}"));
                }
                let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("");
                match ext {
                    "rs" => check_rust(path),
                    "py" => check_python(path),
                    "json" => check_json(path),
                    "toml" => check_toml(path),
                    "js" | "mjs" | "cjs" => check_javascript(path),
                    "ts" => check_typescript(path),
                    "html" | "htm" => check_html(path),
                    "c" | "h" => check_c(path),
                    _ => ToolResult::ok(format!(
                        "不支持语法检查的扩展名 / No syntax check for .{ext} — 支持 .rs .py .json .toml .js .ts .html .c .h"
                    )),
                }
            }

            _ => ToolResult::err(format!(
                "未知模式 / Unknown mode: {mode} (use exists|syntax|size)"
            )),
        }
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{size:.1} {}", UNITS[unit_idx])
}

fn check_rust(path: &str) -> ToolResult {
    let output = std::process::Command::new("rustc")
        .args(["--edition", "2024", "--check", path, "--color", "never"])
        .output();
    match output {
        Ok(o) if o.status.success() => ToolResult::ok("Rust 语法检查通过 / Rust syntax OK"),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            ToolResult::err(format!("Rust 语法错误 / Rust syntax error:\n{stderr}"))
        }
        Err(e) => ToolResult::err(format!("rustc 不可用 / rustc unavailable: {e}")),
    }
}

fn check_python(path: &str) -> ToolResult {
    let py = if cfg!(target_os = "windows") && which("python").is_some() {
        "python"
    } else {
        "python3"
    };
    let output = std::process::Command::new(py)
        .args(["-m", "py_compile", path])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            ToolResult::ok(format!("Python 语法检查通过({py}) / Python syntax OK ({py})"))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if py == "python3" && stderr.contains("not found") {
                return check_python_fallback(path, "python");
            }
            ToolResult::err(format!("Python 语法错误 / Python syntax error:\n{stderr}"))
        }
        Err(e) => {
            if py == "python3" {
                return check_python_fallback(path, "python");
            }
            ToolResult::err(format!("Python 不可用 / Python unavailable: {e}"))
        }
    }
}

fn check_python_fallback(path: &str, cmd: &str) -> ToolResult {
    let output = std::process::Command::new(cmd)
        .args(["-m", "py_compile", path])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            ToolResult::ok(format!("Python 语法检查通过({cmd}) / Python syntax OK ({cmd})"))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            ToolResult::err(format!("Python 语法错误({cmd}) / Python error:\n{stderr}"))
        }
        Err(e) => ToolResult::err(format!("Python 不可用 / Python unavailable: {e}")),
    }
}

fn check_json(path: &str) -> ToolResult {
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(_) => ToolResult::ok("JSON 语法检查通过 / JSON syntax OK"),
            Err(e) => ToolResult::err(format!("JSON 语法错误 / JSON error: {e}")),
        },
        Err(e) => ToolResult::err(format!("读取失败 / Read failed: {e}")),
    }
}

fn check_toml(path: &str) -> ToolResult {
    match std::fs::read_to_string(path) {
        Ok(content) => match content.parse::<toml::Table>() {
            Ok(_) => ToolResult::ok("TOML 语法检查通过 / TOML syntax OK"),
            Err(e) => ToolResult::err(format!("TOML 语法错误 / TOML error: {e}")),
        },
        Err(e) => ToolResult::err(format!("读取失败 / Read failed: {e}")),
    }
}

fn check_javascript(path: &str) -> ToolResult {
    let output = std::process::Command::new("node")
        .args(["--check", path])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            ToolResult::ok("JavaScript 语法检查通过 / JavaScript syntax OK")
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            ToolResult::err(format!("JavaScript 语法错误 / JavaScript syntax error:\n{stderr}"))
        }
        Err(e) => ToolResult::err(format!("node 不可用 / node unavailable: {e}")),
    }
}

fn check_typescript(path: &str) -> ToolResult {
    let output = std::process::Command::new("npx")
        .args(["tsc", "--noEmit", "--strict", path])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            return ToolResult::ok("TypeScript 语法检查通过 / TypeScript syntax OK");
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("Cannot find module") || stderr.contains("command not found") {
                return check_javascript(path);
            }
            return ToolResult::err(format!("TypeScript 语法错误 / TypeScript error:\n{stderr}"));
        }
        Err(e) => {
            let output = std::process::Command::new("node")
                .args(["--check", "--input-type=module", path])
                .output();
            match output {
                Ok(o) if o.status.success() => ToolResult::ok(
                    "TypeScript 检查通过(回退node) / TypeScript OK (node fallback)",
                ),
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    ToolResult::err(format!(
                        "TypeScript error (node fallback):\n{stderr}"
                    ))
                }
                Err(e2) => ToolResult::err(format!(
                    "tsc/node 均不可用 / both unavailable: {e} / {e2}"
                )),
            }
        }
    }
}

fn check_html(path: &str) -> ToolResult {
    let tidy = std::process::Command::new("tidy")
        .args(["-errors", "-quiet", "-utf8", path])
        .output();
    match tidy {
        Ok(o) if o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.trim().is_empty() {
                return ToolResult::ok("HTML 语法检查通过 / HTML syntax OK (tidy)");
            }
            ToolResult::err(format!("HTML 警告 / HTML warnings (tidy):\n{stderr}"))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            ToolResult::err(format!("HTML 语法错误 / HTML error (tidy):\n{stderr}"))
        }
        Err(_) => check_html_basic(path),
    }
}

fn check_html_basic(path: &str) -> ToolResult {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("读取失败 / Read failed: {e}")),
    };
    let opens = content.matches('<').count();
    let closes = content.matches('>').count();
    if opens != closes {
        return ToolResult::err(format!(
            "HTML 语法错误 / HTML error: unbalanced <> — <:{opens} >:{closes}"
        ));
    }
    let dq = content.matches('"').count();
    if dq % 2 != 0 {
        return ToolResult::err(format!(
            "HTML 语法错误 / HTML error: unbalanced double-quotes ({dq})"
        ));
    }
    ToolResult::ok(
        "HTML 基础检查通过(无tidy) / HTML basic check OK (no tidy, bracket balance only)",
    )
}

fn check_c(path: &str) -> ToolResult {
    let cc = if which("clang").is_some() { "clang" } else { "cc" };
    let output = std::process::Command::new(cc)
        .args(["-fsyntax-only", "-Wall", path])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            ToolResult::ok(format!("C 语法检查通过({cc}) / C syntax OK ({cc})"))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            ToolResult::err(format!("C 语法错误 / C syntax error ({cc}):\n{stderr}"))
        }
        Err(e) => {
            if cc == "clang" {
                let output2 = std::process::Command::new("gcc")
                    .args(["-fsyntax-only", "-Wall", path])
                    .output();
                match output2 {
                    Ok(o) if o.status.success() => {
                        return ToolResult::ok("C 语法检查通过(gcc) / C syntax OK (gcc)");
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        return ToolResult::err(format!("C 语法错误(gcc):\n{stderr}"));
                    }
                    Err(e2) => {
                        return ToolResult::err(format!(
                            "C 编译器不可用 / No C compiler: clang: {e} / gcc: {e2}"
                        ));
                    }
                }
            }
            ToolResult::err(format!("C 编译器不可用 / C compiler unavailable: {e}"))
        }
    }
}

fn which(cmd: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("where").arg(cmd).output();

    #[cfg(not(target_os = "windows"))]
    let result = std::process::Command::new("which").arg(cmd).output();

    match result {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => None,
    }
}
