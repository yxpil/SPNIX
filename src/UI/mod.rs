//! SapNi 终端 UI 层 / Terminal UI layer
//! 彩色输出、动态动画、横幅 / Colored output, animations, banners.

use crossterm::style::{style, Attribute, Color, Stylize};
use std::io::Write;

/// 主题色 —— 樱粉
pub const ACCENT: Color = Color::Rgb { r: 247, g: 131, b: 172 };
/// 青蓝辅助
pub const CYAN: Color = Color::Rgb { r: 139, g: 213, b: 255 };
/// 金色
pub const GOLD: Color = Color::Rgb { r: 255, g: 215, b: 0 };
/// 暗灰
pub const DIM: Color = Color::DarkGrey;
/// 绿
pub const SUCCESS: Color = Color::Rgb { r: 80, g: 250, b: 123 };
/// 橙
pub const WARN: Color = Color::Rgb { r: 255, g: 184, b: 108 };
/// 红
pub const ERROR: Color = Color::Rgb { r: 255, g: 85, b: 85 };
/// 灰（比 DIM 更亮）
pub const MUTED: Color = Color::Grey;

// ═══════════════════════════════════════════
// 动态旋转动画
// ═══════════════════════════════════════════

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// 启动一个旋转动画线程，返回停止函数
pub fn spinner(label: &str) -> Box<dyn FnOnce() + Send> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let msg = label.to_string();

    thread::spawn(move || {
        let mut i = 0;
        while r.load(Ordering::Relaxed) {
            print!(
                "\r  {} {}",
                style(SPINNER_FRAMES[i % SPINNER_FRAMES.len()]).with(CYAN),
                style(&msg).with(DIM)
            );
            let _ = std::io::stdout().flush();
            i += 1;
            thread::sleep(Duration::from_millis(80));
        }
    });

    Box::new(move || {
        running.store(false, Ordering::Relaxed);
        // 清除行
        print!("\r\x1b[K");
        let _ = std::io::stdout().flush();
        // 等线程退出
        thread::sleep(Duration::from_millis(100));
    })
}

// ═══════════════════════════════════════════
// 边框绘制
// ═══════════════════════════════════════════

/// 双线框（更精致）
pub fn draw_box(title: &str, lines: &[&str]) {
    let max_w = lines.iter().map(|l| l.len()).max().unwrap_or(0).max(title.len());
    let inner_w = max_w + 4;

    // 顶边 — 渐变色
    println!(
        "{}",
        style(format!("╭{}╮", "─".repeat(inner_w))).with(ACCENT).bold()
    );

    if !title.is_empty() {
        let pad_left = (inner_w - title.len()) / 2;
        let pad_right = inner_w - title.len() - pad_left;
        println!(
            "  {}{}{}",
            " ".repeat(pad_left),
            style(title).with(GOLD).bold(),
            " ".repeat(pad_right),
        );
    }

    for line in lines {
        println!("  {}", style(*line).with(MUTED));
    }

    println!(
        "{}",
        style(format!("╰{}╯", "─".repeat(inner_w))).with(ACCENT).bold()
    );
}

/// 简洁状态框（单行）
pub fn draw_status_box(icon: &str, msg: &str, color: Color) {
    let line = format!(" {icon}  {msg} ");
    let w = line.len() + 2;
    println!(
        "{}",
        style(format!("╭{}╮\n│{}│\n╰{}╯", "─".repeat(w), line, "─".repeat(w))).with(color)
    );
}

// ═══════════════════════════════════════════
// 横幅 & 状态
// ═══════════════════════════════════════════

pub fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    draw_box(
        &format!("  SapNi v{version}"),
        &[
            "     Console AI Agent",
            "     /help · Ctrl+D exit",
        ],
    );
    println!();
}

pub fn success(msg: &str) {
    println!(
        " {} {}",
        style("✓").with(SUCCESS).bold(),
        style(msg).with(MUTED)
    );
}

pub fn error(msg: &str) {
    eprintln!(
        " {} {}",
        style("✗").with(ERROR).bold(),
        style(msg).with(ERROR)
    );
}

pub fn warn(msg: &str) {
    println!(
        " {} {}",
        style("⚡").with(WARN).bold(),
        style(msg).with(WARN)
    );
}

pub fn info(msg: &str) {
    println!(
        " {} {}",
        style("·").with(CYAN).bold(),
        style(msg).with(DIM)
    );
}

/// 思考中（脉冲圆点）
pub fn thinking() {
    print!("{} ", style("◉").with(ACCENT).attribute(Attribute::SlowBlink));
    let _ = std::io::stdout().flush();
}

/// 提示符
pub fn prompt() {
    print!("\n{} ", style("❯").with(ACCENT).bold());
    let _ = std::io::stdout().flush();
}

pub fn divider() {
    println!(
        "{}",
        style("─".repeat(60)).with(DIM).attribute(Attribute::Dim)
    );
}

// ═══════════════════════════════════════════
// 帮助 & 工具列表
// ═══════════════════════════════════════════

pub fn print_help() {
    draw_box(
        "Commands",
        &[
            "/help, /?          — 帮助",
            "/key <KEY>         — 设置 API Key",
            "/model <NAME>      — 切换模型",
            "/provider <NAME>   — 切换提供商",
            "/url <URL>         — 设置 API 地址",
            "/temp <0.0-2.0>    — 温度",
            "/topp <0.0-1.0>    — TopP",
            "/tokens <N>        — 最大输出",
            "/config            — 显示配置",
            "/tools             — 工具列表",
            "/reset             — 新会话",
            "/quit, /exit, /q   — 退出",
        ],
    );
}

pub fn print_tools(tools: &[(String, String)]) {
    let div = tools.iter().map(|(n, _)| n.len()).max().unwrap_or(0) + 2;
    for (name, desc) in tools {
        println!(
            "  {}{}{}",
            style(name).with(CYAN).bold(),
            " ".repeat(div - name.len()),
            style(desc).with(MUTED)
        );
    }
}

/// 工具执行状态（一行）
pub fn tool_status(tool_name: &str, target: &str) {
    println!(
        "\n  {} {} {}",
        style("┌").with(DIM),
        style(tool_name).with(CYAN).bold(),
        style(target).with(DIM),
    );
    let _ = std::io::stdout().flush();
}

/// 工具执行结果
pub fn tool_ok() {
    println!("  {} {}", style("└").with(DIM), style("✓ OK").with(SUCCESS).bold());
    let _ = std::io::stdout().flush();
}

pub fn tool_fail(msg: &str) {
    println!("  {} {}", style("└").with(DIM), style(format!("✗ {msg}")).with(ERROR));
    let _ = std::io::stdout().flush();
}
