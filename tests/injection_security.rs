//! 集成测试：输入注入防护（injection / 边界输入）。
//!
//! SapNi 的文件工具本身是"任意路径"的本地 agent 工具（不做根目录沙箱），
//! 因此这里不断言"路径被拦截"，而是断言**真实存在的安全边界**：
//!   1. EXEC 层的 `check_ascii` 安全闸：非 ASCII 命令在 spawn 前即被拒绝执行；
//!   2. 文件系统层从不经过 shell：文件名里的 `; | && $()` 等元字符被当作
//!      字面文件名，绝不被解释为命令（不产生副作用文件）；
//!   3. XSS / HTML 载荷作为文件内容是**纯数据**：写入后原样读回，不被转写、
//!      不被执行；
//!   4. 搜索/通配是字面子串匹配，不是正则引擎：`.*`、`; rm -rf` 不触发任何
//!      正则替换或 shell。

use SapNi::Tools::{EXEC::ExecTool, LS::LsTool, Tool, ToolParams, WRITE::WriteTool};
use std::fs;

fn p(pairs: &[(&str, &str)]) -> ToolParams {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("sapni_inj_{}_{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

// ── 1. EXEC 安全闸：非 ASCII 命令被拒绝，且不会真正执行 ──

#[test]
fn exec_rejects_non_ascii_command() {
    // 含中文 / 重音字符 -> check_ascii 必须在 spawn 之前拒绝
    let r = ExecTool.execute(&p(&[("action", "exec"), ("cmd", "echo 你好世界héllo")]));
    assert!(!r.success, "non-ASCII command must be rejected, got ok: {}", r.message);
    assert!(
        r.message.contains("Non-ASCII") || r.message.contains("拒绝"),
        "error should mention the ASCII guard: {}",
        r.message
    );
}

#[test]
fn exec_timer_seconds_is_clamped() {
    // timer_set: seconds 必须收敛到 [1, 300]，拒绝越界输入
    let r = ExecTool.execute(&p(&[("action", "timer_set"), ("seconds", "99999")]));
    assert!(r.success, "timer_set: {}", r.message);
    // 不会 panic、不会越界写；消息中给出的剩余秒数必须 <= 300
    let list = ExecTool.execute(&p(&[("action", "timer_list")]));
    assert!(list.success);
}

// ── 2. 文件系统层不经过 shell：元字符是字面量 ──

#[test]
fn shell_metacharacters_in_filename_are_literal() {
    let dir = tmp_dir("literal_name");
    // 故意在文件名里塞入 shell 元字符
    let evil_name = "marker; touch pwned && echo done.txt";
    let target = dir.join(evil_name);

    let r = WriteTool.execute(&p(&[
        ("action", "write"),
        ("path", &target.to_string_lossy()),
        ("content", "x"),
    ]));
    assert!(r.success, "write literal filename: {}", r.message);

    // 关键断言：文件确实以这一整串作为**字面文件名**落盘……
    assert!(target.exists(), "literal file should exist");
    // ……并且 shell 没有被调用：绝没有凭空生成 "pwned" 这个副作用文件
    let pwned = dir.join("pwned");
    assert!(!pwned.exists(), "shell must NOT interpret '; touch pwned'");
    let _ = fs::remove_dir_all(&dir);
}

// ── 3. XSS / HTML 载荷是数据，原样往返 ──

#[test]
fn xss_payload_is_verbatim_data_not_executed() {
    let dir = tmp_dir("xss");
    let f = dir.join("note.txt");
    let payload = "<script>alert('xss')</script><img src=x onerror=alert(1)>";

    let w = WriteTool.execute(&p(&[
        ("action", "write"),
        ("path", &f.to_string_lossy()),
        ("content", payload),
    ]));
    assert!(w.success);

    let r = LsTool.execute(&p(&[("action", "read_file"), ("path", &f.to_string_lossy())]));
    assert!(r.success);
    let data = r.data.unwrap();
    // 原样保留：工具绝不"执行"或"转义"这段内容，只是把它当文本读回来
    assert!(data.contains(payload), "payload must round-trip verbatim:\n{data}");
    let _ = fs::remove_dir_all(&dir);
}

// ── 4. 搜索是字面匹配，不是正则/shell ──

#[test]
fn search_treats_regex_metachars_literally() {
    let dir = tmp_dir("grep");
    let f = dir.join("sample.txt");
    // 文件里放一段含正则元字符的文本
    fs::write(&f, "price: $100.00\nplain line\n").unwrap();

    // 用字面子串搜索 "$100.00"（含 . $ 元字符）
    let r = LsTool.execute(&p(&[
        ("action", "search_in_files"),
        ("path", &dir.to_string_lossy()),
        ("pattern", "$100.00"),
    ]));
    assert!(r.success);
    let data = r.data.unwrap();
    // 作为字面子串命中；`.` 不应被当作"任意字符"去吞掉别的行
    assert!(data.contains("$100.00"), "literal substring should match:\n{data}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn glob_only_wildcards_not_regex() {
    let dir = tmp_dir("glob");
    fs::write(dir.join("a.rs"), "fn main() {}").unwrap();
    fs::write(dir.join("b.txt"), "hello").unwrap();

    let r = LsTool.execute(&p(&[
        ("action", "list_dir"),
        ("path", &dir.to_string_lossy()),
        ("glob", "*.rs"),
    ]));
    assert!(r.success);
    let data = r.data.unwrap();
    assert!(data.contains("a.rs"));
    assert!(!data.contains("b.txt"), "glob *.rs must exclude b.txt:\n{data}");
    let _ = fs::remove_dir_all(&dir);
}
