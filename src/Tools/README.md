# SapNi Tools

SapNi 内置工具集 — 每个工具一个文件，统一 `Tool` trait 接口。

Built-in toolset for SapNi AI agent — one file per tool, unified `Tool` trait.

---

## 架构 / Architecture

```
src/
├── main.rs          # CLI 入口（交互模式 + 演示模式 / interactive + demo）
├── Agent/mod.rs     # Agent 核心循环（对话 → LLM → 工具调用 → 回复）
├── config/mod.rs    # 配置层（读取/保存/热更新 + 提供商预设）
├── Net/OPENAI/mod.rs # OpenAI 兼容 HTTP 客户端（流式 SSE + 非流式）
├── Control/List/mod.rs # 消息管道（上下文构建 + 对话管理 + token 估算）
├── DB/mod.rs        # 数据库层（会话 + 记忆 + 贝叶斯联想）
├── Neture/mod.rs    # 朴素贝叶斯分类器（词语联想引擎）
├── Tools/
│   ├── mod.rs       # Tool trait + ToolResult + make_schema + 模块导出
│   ├── LS/mod.rs    # 文件浏览与搜索 / File browser & search
│   ├── WRITE/mod.rs # 文件操作（写/复制/移动） / File operations
│   ├── EDIT/mod.rs  # 文件编辑（查找替换+行编辑） / File editing
│   ├── DEL/mod.rs   # 文件删除 / File deletion
│   ├── EXEC/mod.rs  # 命令执行 + 定时器 / Command execution
│   ├── CHECK/mod.rs # 语法校验（rust/python/json/toml/js/ts/html/c）
│   ├── PLUGS/mod.rs # 插件管理 / Plugin registry
│   └── Todo/mod.rs  # 计划管理(最多200步) / Plan maker
```

## Tool Trait

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;        // 工具名（大写）
    fn description(&self) -> &'static str; // 中英双语描述
    fn schema(&self) -> ToolSchema;        // OpenAI function schema
    fn execute(&self, params: &ToolParams) -> ToolResult;  // 执行
}
```

`ToolParams` = `HashMap<String, String>`，参数用键值对传入。

`schema()` 返回 OpenAI 兼容的 function-calling schema，使 LLM 能自动发现和调用工具。

## 工具速览 / Quick Reference

| 工具 | 比喻 | 必选参数 | 可选参数 |
|------|------|----------|----------|
| **LS** | 像 `ls` + `find` | `path` | `glob` (通配过滤, 如 `*.rs`) |
| **WRITE** | 像 `echo > file` | `path`, `content` | — |
| **EDIT** | 像 `sed` + 行编辑 | `path`, `mode`(replace\|line) | replace: `old`,`new` — line: `action`(replace\|insert_before\|insert_after\|delete), `line`, `content`, `end_line` |
| **DEL** | 像 `rm -rf` | `path` | — |
| **EXEC** | 像 `sh -c` | `cmd` | `cwd`, `timeout_secs` |
| **CHECK** | 像 `test -f` + linter | `path` | `mode` (exists\|syntax\|size) |
| **CHECK** 支持的语法 | `.rs` `rustc` `.py` `python3` `.json` `serde` `.toml` `toml` `.js/.mjs/.cjs` `node` `.ts` `tsc→node` `.html/.htm` `tidy→basic` `.c/.h` `clang→gcc` |
| **PLUGS** | 像 `npm install` | `action` | `name`, `path`, `version`, `description` |
| **TODO** | 任务管理(200条上限) | `action`(add\|list\|update\|delete\|move\|clear\|get) | add: `content`,`status`,`position`(插队) — update: `id`,`content?`,`status?` — delete: `id` — move: `id`,`to_position` — list: `filter`(pending\|all\|in_progress\|completed) — get: `id` |

## 使用示例 / Usage

```rust
use Tools::{Tool, ToolParams};
use std::collections::HashMap;

let tool = Tools::LS::LsTool;
let mut params = HashMap::new();
params.insert("path".into(), "/home/user".into());
params.insert("glob".into(), "*.rs".into());

let result = tool.execute(&params);
assert!(result.success);
println!("{}", result.message);
if let Some(data) = result.data {
    println!("{}", data);  // 文件列表
}
```

## 添加新工具 / Adding a Tool

1. 在 `src/Tools/` 下新建 `YOURTOOL/mod.rs`
2. 实现 `Tool` trait 三个方法
3. 在 `src/Tools/mod.rs` 加 `pub mod YOURTOOL;`
4. 中英双语 message 描述

## 运行 / Run

```bash
cargo build
cargo run          # 列出所有工具 + LS/CHECK 演示
cargo test         # 运行测试
```

## 设计约定 / Conventions

- **try-catch 兜底**：所有文件/进程操作都有错误处理，失败返回 `ToolResult::err()` 而非 panic
- **父目录自动创建**：WRITE 工具会自动 `create_dir_all`
- **Windows 隐藏窗口**：EXEC 在 Windows 下设置 `CREATE_NO_WINDOW`（开发铁律）
- **中英双语**：所有 message 都是 `中文 / English` 格式
- **全局插件注册表**：PLUGS 使用 `LazyLock<Mutex<HashMap>>` 线程安全单例
