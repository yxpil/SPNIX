//! 集成测试：核心工具行为（LS / WRITE / DEL）。
//!
//! 这些用例从外部视角调用 `SapNi` 暴露的工具 trait，验证真实文件系统行为、
//! 参数校验与错误路径。所有临时文件都落在系统临时目录，结束后清理。

use SapNi::Tools::{DEL::DelTool, LS::LsTool, Tool, ToolParams, WRITE::WriteTool};
use std::fs;

fn p(pairs: &[(&str, &str)]) -> ToolParams {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn tmp_path(name: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("sapni_it_{}_{}", std::process::id(), name));
    d
}

#[test]
fn ls_list_dir_lists_entries() {
    let dir = tmp_path("ls_dir");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.rs"), "fn main() {}").unwrap();

    let r = LsTool.execute(&p(&[("action", "list_dir"), ("path", &dir.to_string_lossy())]));
    assert!(r.success, "list_dir should succeed: {:?}", r.message);
    let data = r.data.unwrap();
    assert!(data.contains("a.txt"), "data should list a.txt:\n{data}");
    assert!(data.contains("b.rs"), "data should list b.rs:\n{data}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ls_read_file_offset_and_limit() {
    let f = tmp_path("readme.txt");
    let body = "line1\nline2\nline3\nline4\nline5\n";
    fs::write(&f, body).unwrap();

    // offset=2, limit=2 -> lines 2..=3
    let r = LsTool.execute(&p(&[
        ("action", "read_file"),
        ("path", &f.to_string_lossy()),
        ("offset", "2"),
        ("limit", "2"),
    ]));
    assert!(r.success, "read_file: {:?}", r.message);
    let data = r.data.unwrap();
    assert!(data.contains("line2"), "should contain line2:\n{data}");
    assert!(data.contains("line3"), "should contain line3:\n{data}");
    assert!(!data.contains("line4"), "limit=2 must exclude line4:\n{data}");
    let _ = fs::remove_file(&f);
}

#[test]
fn ls_read_missing_file_errors() {
    let r = LsTool.execute(&p(&[
        ("action", "read_file"),
        ("path", "definitely_not_here_sapni_xyz.txt"),
    ]));
    assert!(!r.success, "missing file must be an error");
    assert!(r.message.contains("not found") || r.message.contains("不存在"), "msg: {}", r.message);
}

#[test]
fn ls_unknown_action_errors() {
    let r = LsTool.execute(&p(&[("action", "nuke_all"), ("path", ".")]));
    assert!(!r.success);
    assert!(r.message.contains("Unknown action") || r.message.contains("未知操作"), "{}", r.message);
}

#[test]
fn ls_missing_param_errors() {
    // list_dir requires `path`
    let r = LsTool.execute(&p(&[("action", "list_dir")]));
    assert!(!r.success);
    assert!(r.message.contains("Missing param") || r.message.contains("缺少参数"), "{}", r.message);
}

#[test]
fn write_then_read_roundtrip() {
    let f = tmp_path("roundtrip.txt");
    let r = WriteTool.execute(&p(&[
        ("action", "write"),
        ("path", &f.to_string_lossy()),
        ("content", "persist me"),
    ]));
    assert!(r.success, "write: {:?}", r.message);
    assert_eq!(fs::read_to_string(&f).unwrap(), "persist me");

    let rr = LsTool.execute(&p(&[("action", "read_file"), ("path", &f.to_string_lossy())]));
    assert!(rr.success);
    assert!(rr.data.unwrap().contains("persist me"));
    let _ = fs::remove_file(&f);
}

#[test]
fn del_removes_file_and_missing_errors() {
    let f = tmp_path("todelete.txt");
    fs::write(&f, "bye").unwrap();
    let r = DelTool.execute(&p(&[("path", &f.to_string_lossy())]));
    assert!(r.success, "del: {:?}", r.message);
    assert!(!f.exists());

    // deleting a second time -> error
    let r2 = DelTool.execute(&p(&[("path", &f.to_string_lossy())]));
    assert!(!r2.success);
}

#[test]
fn tool_schema_shape_is_openai_compatible() {
    let schema = LsTool.schema();
    assert_eq!(schema["type"], "function");
    assert_eq!(schema["function"]["name"], "LS");
    // required params must be declared
    let required = schema["function"]["parameters"]["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "path"));
}
