//! SapNi 库目标 (library target).
//!
//! 这个 crate 原本只有一个二进制入口 `src/main.rs`。为了让 `tests/` 下的
//! 集成测试能够直接调用工具层 (`Tools`)、配置层 (`config`) 等内部模块，
//! 这里额外暴露一个 library target。二进制入口 `main.rs` 未做任何改动，
//! 它仍然独立编译自己的模块树；本 lib 只是同一批 `src/**` 源文件的另一个
//! 编译出口，**仅用于测试**。
//!
//! 运行测试:
//! - 单元测试(内置于各模块 `#[cfg(test)]`): `cargo test --bin SapNi`
//! - 集成测试(本 lib + `tests/*.rs`):       `cargo test --lib` 与 `cargo test --test '*'`
//! - 全部:                                 `cargo test`

#[allow(non_snake_case)]
pub mod Agent;
#[allow(non_snake_case)]
pub mod Control;
pub mod DB;
#[allow(non_snake_case)]
pub mod Net;
pub mod Neture;
#[allow(non_snake_case)]
pub mod Tools;
pub mod config;
#[allow(non_snake_case)]
pub mod UI;
