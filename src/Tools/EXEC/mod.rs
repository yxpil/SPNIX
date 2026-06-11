use crate::Tools::{make_schema, Tool, ToolParams, ToolResult, ToolSchema};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static TIMERS: std::sync::LazyLock<Mutex<Vec<TimerEntry>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

struct TimerEntry {
    id: String,
    message: String,
    deadline: Instant,
}

pub struct ExecTool;

impl Tool for ExecTool {
    fn name(&self) -> &'static str {
        "EXEC"
    }

    fn description(&self) -> &'static str {
        "命令执行(纯ASCII) / Command execution (ASCII only).  \
         action=exec(默认)|wait|timer_set|timer_list|timer_wait  \
         exec: cmd(required), cwd?, timeout_secs?  同步短命令  \
         wait: cmd(required), cwd?, timeout_secs?  spawn长命令  \
         timer_set: seconds, message?  设定时器  \
         timer_list: 列出活跃定时器  \
         timer_wait: seconds  阻塞等待  \
         Windows 自动 GBK→UTF-8"
    }

    fn schema(&self) -> ToolSchema {
        make_schema(
            "EXEC",
            "Execute shell commands, timers, and wait operations / 执行终端命令、定时器",
            &[
                ("action", "string", "Action: exec, wait, timer_set, timer_list, timer_wait / 操作类型", true),
                ("cmd", "string", "Shell command to execute (ASCII only) / 终端命令", false),
                ("cwd", "string", "Working directory for command / 工作目录", false),
                ("timeout_secs", "string", "Timeout in seconds (default 300) / 超时秒数", false),
                ("seconds", "string", "Seconds for timer_set/timer_wait / 秒数", false),
                ("message", "string", "Message for timer / 定时器消息", false),
            ],
        )
    }

    fn execute(&self, params: &ToolParams) -> ToolResult {
        let action = params.get("action").map(|s| s.as_str()).unwrap_or("exec");
        match action {
            "exec" => exec_cmd(params),
            "wait" => wait_cmd(params),
            "timer_set" => timer_set(params),
            "timer_list" => timer_list(),
            "timer_wait" => timer_wait(params),
            _ => ToolResult::err(format!("未知操作 / Unknown action: {action}")),
        }
    }
}

fn exec_cmd(params: &ToolParams) -> ToolResult {
    let cmd_str = try_tool!(param(params, "cmd"));
    try_tool!(check_ascii(&cmd_str));

    let mut cmd = build_shell(&cmd_str, params);
    match cmd.output() {
        Ok(output) => {
            let stdout = decode_output(&output.stdout);
            let stderr = decode_output(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);
            if exit_code == 0 {
                ToolResult::ok_with_data(
                    format!("命令成功 / Command OK (exit={exit_code})"),
                    stdout,
                )
            } else {
                let combined = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                ToolResult::err(format!(
                    "命令失败 / Command failed (exit={exit_code}): {combined}"
                ))
            }
        }
        Err(e) => ToolResult::err(format!("执行失败 / Execution error: {e}")),
    }
}

fn wait_cmd(params: &ToolParams) -> ToolResult {
    let cmd_str = try_tool!(param(params, "cmd"));
    try_tool!(check_ascii(&cmd_str));
    let timeout_secs: u64 = params
        .get("timeout_secs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let start = Instant::now();

    let mut child = build_shell(&cmd_str, params);
    child.stdout(Stdio::piped());
    child.stderr(Stdio::piped());

    let mut child = match child.spawn() {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("启动失败 / Spawn failed: {e}")),
    };

    let deadline = start + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let elapsed = start.elapsed().as_secs_f64();
                let exit_code = status.code().unwrap_or(-1);
                let stdout = read_stdout(child.stdout.take());
                let stderr = read_stderr(child.stderr.take());

                let msg = if exit_code == 0 {
                    format!("命令成功 / Command OK (exit={exit_code}, {elapsed:.1}s)")
                } else {
                    format!("命令失败 / Command failed (exit={exit_code}, {elapsed:.1}s)")
                };
                let output = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n[stderr]\n{stderr}")
                };
                return ToolResult::ok_with_data(msg, output);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    child.kill().ok();
                    let elapsed = start.elapsed().as_secs_f64();
                    return ToolResult::err(format!("命令超时 / Command timeout ({elapsed:.1}s)"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return ToolResult::err(format!("等待失败 / Wait error: {e}")),
        }
    }
}

fn timer_set(params: &ToolParams) -> ToolResult {
    let seconds: u64 = params
        .get("seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
        .min(300)
        .max(1);
    let message = params
        .get("message")
        .cloned()
        .unwrap_or_else(|| "定时器触发 / Timer fired".into());
    let mut timers = TIMERS.lock().unwrap();
    let id = format!("t{}", timers.len() + 1);
    timers.push(TimerEntry {
        id: id.clone(),
        message,
        deadline: Instant::now() + Duration::from_secs(seconds),
    });
    ToolResult::ok(format!("定时器已设置 / Timer set: [{id}] {seconds}s"))
}

fn timer_list() -> ToolResult {
    let timers = TIMERS.lock().unwrap();
    let now = Instant::now();
    let active: Vec<_> = timers.iter().filter(|t| t.deadline > now).collect();
    if active.is_empty() {
        return ToolResult::ok("无活跃定时器 / No active timers");
    }
    let lines: Vec<String> = active
        .iter()
        .map(|t| {
            let remaining = t.deadline.duration_since(now).as_secs();
            format!("  [{id}] {remaining}s left — {msg}", id = t.id, msg = t.message)
        })
        .collect();
    ToolResult::ok_with_data(
        format!("{} 个活跃定时器 / {} active timers", lines.len(), lines.len()),
        lines.join("\n"),
    )
}

fn timer_wait(params: &ToolParams) -> ToolResult {
    let seconds: u64 = params
        .get("seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
        .min(300)
        .max(1);
    std::thread::sleep(Duration::from_secs(seconds));
    ToolResult::ok(format!("等待完成 / Wait done: {seconds}s"))
}

fn param(params: &ToolParams, key: &str) -> Result<String, ToolResult> {
    params
        .get(key)
        .cloned()
        .ok_or_else(|| ToolResult::err(format!("缺少参数 / Missing param: {key}")))
}

fn check_ascii(cmd: &str) -> Result<(), ToolResult> {
    if !cmd.is_ascii() {
        let bad: Vec<String> = cmd
            .chars()
            .filter(|c| !c.is_ascii())
            .take(5)
            .map(|c| format!("U+{:04X}", c as u32))
            .collect();
        return Err(ToolResult::err(format!(
            "拒绝非ASCII指令 / Non-ASCII rejected: {}",
            bad.join(", ")
        )));
    }
    Ok(())
}

fn build_shell(cmd_str: &str, params: &ToolParams) -> Command {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd_str]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd_str]);
        c
    };
    if let Some(cwd) = params.get("cwd") {
        cmd.current_dir(cwd);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

fn read_stdout(pipe: Option<std::process::ChildStdout>) -> String {
    read_any(pipe)
}
fn read_stderr(pipe: Option<std::process::ChildStderr>) -> String {
    read_any(pipe)
}
fn read_any<R: std::io::Read>(pipe: Option<R>) -> String {
    pipe.map(|mut p| {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut p, &mut buf).ok();
        decode_output(&buf)
    })
    .unwrap_or_default()
}

fn decode_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            #[cfg(target_os = "windows")]
            {
                let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
                if !had_errors {
                    return cow.into_owned();
                }
            }
            bytes.iter().map(|&b| b as char).collect()
        }
    }
}
