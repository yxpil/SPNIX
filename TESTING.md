# SapNi (SPNIX) 测试说明 / Testing Guide

- 测试完成：是（2026-10-04）
- 测试日期：2026-10-04
- 测试内容：单元测试（原 50 个，覆盖工具/DB/协议等）；集成测试 `tests/`（tools_integration 8：LS/WRITE/DEL 文件往返、分页、缺参/未知动作错误、OpenAI schema；injection_security 6：EXEC 拒绝非 ASCII、shell 元字符在文件名是字面量、XSS 载荷原样往返、搜索/glob 字面匹配；plugin_hooks 7：插件注册→list、重复注册拒绝、移除/启用未注册拒绝、状态翻转、未知 action 拒绝）。
- 运行命令：`cargo test --no-fail-fast`
- 测试框架：Rust `#[cfg(test)]` + 外部 `tests/` 集成测试
- 模型：豆包（Doubao）生成

本仓库原本只有内置于 `src/**` 的 `#[cfg(test)]` 单元测试（50 个用例）。
本次补测在不改动产品行为的前提下，新增了一个 library target（`src/lib.rs`，
仅用于让 `tests/` 能调用工具/配置层）以及 `tests/` 下的集成测试。

## 测试目录与文档位置

- 单元测试：各模块内 `#[cfg(test)]`（`src/**`，共 50 个）。
- 集成测试：仓库根 `tests/`
  - `tests/tools_integration.rs` —— 核心工具行为（LS/WRITE/DEL 的文件系统往返、
    offset/limit 分页、缺失参数/未知动作错误路径、OpenAI function schema 形状）。
  - `tests/injection_security.rs` —— **输入注入测试**（见下）。
  - `tests/plugin_hooks.rs` —— **插件/钩子机制测试**（见下）。
- 说明文档：本文件 `TESTING.md`。

## 运行命令

```powershell
# 全部测试（单元 + 集成）
cargo test

# 仅单元测试（lib + bin）
cargo test --lib
cargo test --bin SapNi

# 仅集成测试
cargo test --test tools_integration
cargo test --test injection_security
cargo test --test plugin_hooks

# 跑某个具体用例
cargo test --test injection_security exec_rejects_non_ascii
```

## 测了什么 / 预期通过数

基线（补测前）：50 passed / 0 failed（仅 bin 单元测试）。

补测后本地两次完整 `cargo test` 结果：

| 目标 | 通过 | 失败 |
|---|---|---|
| lib 单元测试 (`unittests src/lib.rs`) | 50 | 0 |
| bin 单元测试 (`unittests src/main.rs`) | 50 | 0 |
| 集成 `tests/tools_integration.rs` | 8 | 0 |
| 集成 `tests/injection_security.rs` | 6 | 0 |
| 集成 `tests/plugin_hooks.rs` | 7 | 0 |
| **合计** | **121** | **0** |

> 说明：lib 与 bin 两个 target 会各自编译同一套 `src/**` 模块树并运行其中的
> `#[cfg(test)]`，因此 50 个单元用例会在两个 target 各跑一遍（这是新增
> `src/lib.rs` 暴露测试入口的副作用，不是重复实现）。

## 输入注入测试（`tests/injection_security.rs`，共 6 个）

SapNi 的文件工具是"任意路径"的本地 agent 工具，**不做根目录沙箱**，因此这里
不断言"路径被拦截"，而是断言代码真实存在的安全边界：

1. `exec_rejects_non_ascii_command` —— EXEC 层的 `check_ascii` 安全闸：含中文/
   重音字符的命令在 `spawn` 之前即被拒绝（`success=false`），绝不真正执行。
2. `exec_timer_seconds_is_clamped` —— `timer_set` 的秒数收敛到 `[1,300]`，越界
   输入被钳制，不 panic。
3. `shell_metacharacters_in_filename_are_literal` —— 文件系统层从不经过 shell：
   文件名里的 `; touch pwned && echo` 被当作**字面文件名**落盘，且**不会**凭空
   生成副作用文件 `pwned`（证明 `;`/`&&` 未被 shell 解释）。
4. `xss_payload_is_verbatim_data_not_executed` —— `<script>…</script>`、
   `<img onerror=…>` 等 XSS 载荷作为文件内容**原样往返**，不被转写/执行。
5. `search_treats_regex_metachars_literally` —— 搜索是字面子串匹配，不是正则引擎：
   `$100.00` 中的 `.`/`$` 不被当作正则元字符。
6. `glob_only_wildcards_not_regex` —— `*.rs` 只按通配过滤，排除 `.txt`。

## 插件/钩子机制测试（`tests/plugin_hooks.rs`，共 7 个）

PLUGS 工具维护一个全局插件注册表（list/add/remove/enable/disable）。验证钩子
机制应有的不变量：

1. `plugin_register_then_list_shows_it` —— 注册后 list 可见，版本/描述参数正确保存。
2. `plugin_duplicate_add_is_rejected` —— 同名重复注册被拒绝（幂等保护）。
3. `plugin_remove_nonexistent_is_rejected` —— 移除未注册插件被拒绝（不存在隔离）。
4. `plugin_enable_disable_state_transitions` —— enable/disable 状态翻转正确。
5. `plugin_enable_nonexistent_is_rejected` —— 对未注册插件 enable 被拒绝（越权隔离）。
6. `plugin_remove_then_list_drops_it` —— 移除后不再出现在 list。
7. `plugin_unknown_action_is_rejected` —— 未知 action 被拒绝。

> 各用例使用唯一名（进程 id + 原子计数器），避免全局注册表在并行测试间互相污染。

## 备注：一处预存测试隔离修复

`src/DB/mod.rs` 测试模块里原先所有 DB 用例共用同一个硬编码临时目录
`/tmp/sapni_db_test_{pid}`，在 cargo 默认并行线程下会互相 `remove_dir_all`，
偶发 `flush` 时报"系统找不到指定的路径 (os error 3)"。本次把 `temp_dir()`
改为每次调用取唯一目录（原子计数器），使整套测试确定性通过。这是测试隔离修复，
未改动任何产品逻辑。
