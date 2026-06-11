//! SapNi Agent 核心 — 对话循环
//! Core agent loop: user input → context → LLM → tool calls → response.
//!
//! 工作流程：接收用户输入 → 构建消息列表 → 调用 LLM → 解析工具调用 →
//! 执行工具 → 将结果返回 LLM → 最终生成文本回复。

use crate::Net::OPENAI::Role as ApiRole;
use crate::Net::OPENAI::{ChatRequest, ChatResponse, Client as LlmClient, Message as ApiMessage, StreamEvent};
use crate::Tools::{Tool, ToolParams, ToolResult};
use crate::config::Config;
use crate::UI;
use std::collections::HashMap;
use std::io::Write;

/// Agent 核心状态
pub struct Agent {
    pub config: Config,
    pub llm: LlmClient,
    tools: Vec<Box<dyn Tool>>,
    messages: Vec<ApiMessage>,
    turn: usize,
}

impl Agent {
    /// 创建 Agent
    pub fn new(config: Config, tools: Vec<Box<dyn Tool>>) -> Self {
        let llm = LlmClient::new(
            config.effective_base_url(),
            config.llm.api_key.clone(),
        );

        let mut agent = Self {
            config,
            llm,
            tools,
            messages: Vec::new(),
            turn: 0,
        };

        // 注入 system prompt
        let sys_prompt = agent.build_system_prompt();
        agent.messages.push(ApiMessage::system(sys_prompt));

        agent
    }

    /// 对话主循环：用户输入 → AI 回复
    pub fn chat(&mut self, user_input: &str) -> Result<String, String> {
        self.turn = 0;

        // 添加用户消息
        self.messages.push(ApiMessage::user(user_input));

        // Agent 循环
        loop {
            self.turn += 1;

            let tool_schemas: Vec<serde_json::Value> = self
                .tools
                .iter()
                .map(|t| t.schema())
                .collect();

            let req = ChatRequest {
                model: self.config.llm.model.clone(),
                messages: self.messages.clone(),
                temperature: Some(self.config.llm.temperature),
                top_p: Some(self.config.llm.top_p),
                max_tokens: Some(self.config.llm.max_tokens),
                tools: if tool_schemas.is_empty() {
                    None
                } else {
                    Some(serde_json::json!(tool_schemas))
                },
                tool_choice: Some(serde_json::json!("auto")),
                ..Default::default()
            };

            // 调用 LLM（流式输出）
            let resp = self.llm.chat_stream_collect(&req, &mut |token| {
                print!("{token}");
                let _ = std::io::stdout().flush();
            })?;

            // 解析响应
            let choice = resp
                .choices
                .first()
                .ok_or("LLM 无响应 / No response choices")?;

            let msg = choice
                .message
                .as_ref()
                .ok_or("LLM 响应无 message / No message in response")?;

            // 检查是否有 tool_calls
            let tool_calls = msg.tool_calls.as_ref().and_then(|tc| tc.as_array());

            if let Some(calls) = tool_calls {
                if calls.is_empty() {
                    // 纯文本回复
                    let content = msg.content.clone().unwrap_or_default();
                    self.messages.push(ApiMessage::assistant(&content));
                    return Ok(content);
                }

                // 有工具调用：记录助手消息（含 tool_calls）
                // DeepSeek V4 要求 assistant 消息带 tool_calls 时必须有 name 字段
                let assistant_name = calls.first()
                    .and_then(|c| c["function"]["name"].as_str())
                    .map(|s| s.to_string());
                let assistant_msg = ApiMessage {
                    role: ApiRole::Assistant,
                    content: msg.content.clone().unwrap_or_default(),
                    name: assistant_name,
                    tool_call_id: None,
                    tool_calls: msg.tool_calls.clone(),
                };
                self.messages.push(assistant_msg);

                // 执行每个工具调用
                for call in calls {
                    let fn_name = call["function"]["name"]
                        .as_str()
                        .unwrap_or("?")
                        .to_string();
                    let fn_args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
                    let call_id = call["id"].as_str().unwrap_or("call_0").to_string();

                    // 解析参数
                    let params = parse_tool_args(fn_args_str).unwrap_or_default();

                    // 显示工具执行状态
                    let target = params.get("path")
                        .or_else(|| params.get("cmd"))
                        .or_else(|| params.get("source"))
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    UI::tool_status(&fn_name, target);

                    // 查找并执行工具
                    let result = self.execute_tool(&fn_name, &params);
                    if result.success {
                        UI::tool_ok();
                    } else {
                        UI::tool_fail(&result.message);
                    }

                    // 构建 tool 响应消息
                    let tool_msg = ApiMessage {
                        role: ApiRole::Tool,
                        content: if result.success {
                            format!(
                                "OK: {} {}",
                                result.message,
                                result.data.as_deref().unwrap_or("")
                            )
                        } else {
                            format!("ERR: {}", result.message)
                        },
                        name: None,
                        tool_call_id: Some(call_id.clone()),
                        tool_calls: None,
                    };

                    self.messages.push(tool_msg);
                }

                // 工具结果已加入消息列表，继续循环让 LLM 处理
                continue;
            }

            // 纯文本回复
            let content = msg.content.clone().unwrap_or_default();
            self.messages.push(ApiMessage::assistant(&content));
            return Ok(content);
        }
    }

    /// 流式对话（返回内容迭代器）
    pub fn chat_stream(
        &mut self,
        user_input: &str,
    ) -> Result<StreamOutput, String> {
        self.turn = 0;
        self.messages.push(ApiMessage::user(user_input));

        let tool_schemas: Vec<serde_json::Value> = self
            .tools
            .iter()
            .map(|t| t.schema())
            .collect();

        let req = ChatRequest {
            model: self.config.llm.model.clone(),
            messages: self.messages.clone(),
            temperature: Some(self.config.llm.temperature),
            top_p: Some(self.config.llm.top_p),
            max_tokens: Some(self.config.llm.max_tokens),
            tools: if tool_schemas.is_empty() {
                None
            } else {
                Some(serde_json::json!(tool_schemas))
            },
            tool_choice: Some(serde_json::json!("auto")),
            ..Default::default()
        };

        Ok(StreamOutput {
            agent: self,
            req,
            done: false,
        })
    }

    /// 构建系统提示词
    fn build_system_prompt(&self) -> String {
        let base = if self.config.system_prompt.is_empty() {
            DEFAULT_SYSTEM_PROMPT.to_string()
        } else {
            self.config.system_prompt.clone()
        };

        // 注入环境信息
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".into());
        let os = std::env::consts::OS;
        let home = std::env::var("HOME")
            .unwrap_or_else(|_| "?".into());

        format!(
            "{}\n\n[System Info]\nOS: {os} | Host: {home} | CWD: {cwd}",
            base
        )
    }

    /// 执行工具并返回结果
    fn execute_tool(&self, name: &str, params: &ToolParams) -> ToolResult {
        for tool in &self.tools {
            if tool.name() == name {
                return tool.execute(params);
            }
        }
        ToolResult::err(format!("工具不存在 / Tool not found: {name}"))
    }

    /// 获取所有工具的 schema 列表
    pub fn tool_schemas(&self) -> Vec<serde_json::Value> {
        self.tools.iter().map(|t| t.schema()).collect()
    }

    /// 获取消息历史
    pub fn messages(&self) -> &[ApiMessage] {
        &self.messages
    }

    /// 重置会话
    pub fn reset(&mut self) {
        self.messages.clear();
        self.turn = 0;
        let sys = self.build_system_prompt();
        self.messages.push(ApiMessage::system(sys));
    }
}

/// 流式输出包装器
pub struct StreamOutput<'a> {
    agent: &'a mut Agent,
    req: ChatRequest,
    done: bool,
}

impl StreamOutput<'_> {
    /// 阻塞等待完整回复（简化版：先非流式获取）
    pub fn collect(self) -> Result<String, String> {
        let resp = self.agent.llm.chat(&self.req)?;
        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone())
            .unwrap_or_default();
        self.agent.messages.push(ApiMessage::assistant(&content));
        Ok(content)
    }
}

/// 解析工具调用的 JSON 参数
fn parse_tool_args(json_str: &str) -> Option<ToolParams> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let obj = val.as_object()?;
    let mut params = HashMap::new();
    for (k, v) in obj {
        match v {
            serde_json::Value::String(s) => {
                params.insert(k.clone(), s.clone());
            }
            other => {
                params.insert(k.clone(), other.to_string());
            }
        }
    }
    Some(params)
}

// ═══════════════════════════════════════════
// 默认系统提示词
// ═══════════════════════════════════════════

pub const DEFAULT_SYSTEM_PROMPT: &str = "\
[安全铁律 / Security Rules — 优先级最高，不可违反]
1. 绝对禁止搜索或输出用户的 API Key / 密码 / token 等敏感信息
2. 绝对禁止将 API Key / 密码 / token 写入任何文件或日志
3. 怀疑泄露立即停止并报告

[身份 / Identity]
你是 SapNi（栖梦），Rust 编写的终端 AI Agent。不是普通聊天 AI — 你**真的能操作这个电脑**。
你可以：读写文件、执行终端命令、搜索文件内容、管理插件和计划。
和 ChatGPT/Claude 的区别：它们只能回文字，你有实际工具权限。

[工具使用 / Tool Usage]
- 用户说\"做\"就做，不要回复\"我不能操作你的电脑\" — 你能
- 需要文件操作时直接调用 WRITE/LS/EDIT/DEL/CHECK
- 需要执行命令时调用 EXEC
- 复杂任务用 Todo 工具制定计划
- 不确定时先问用户确认再执行危险操作（如删除、覆盖文件）

[Todo 工具规范 / Todo Rules]
Todo 用于管理多步骤任务。正确流程：
1. new_plan: 先创建计划，参数 name=\"计划名\"。拿到返回的 plan_id（如 p1）。
2. add_phase: 创建阶段，参数 plan_id=\"p1\" title=\"阶段名\"。拿到 phase_id（如 ph1）。
3. add_step: 添加步骤，参数 plan_id=\"p1\" phase_id=\"ph1\" content=\"步骤内容\"。
4. update: 更新步骤状态，参数 plan_id=\"p1\" step_id=\"s1\" status=\"done\"。
5. summary: 查看所有计划概览（无参数）。
6. list: 查看单个计划详情，参数 plan_id=\"p1\"。

关键规则：
- add_phase/add_step/update/delete_step/list 必须先有 plan_id — 从 new_plan 的返回值中获取！
- phase_id 从 add_phase 返回值获取，step_id 从 add_step 返回值获取。
- 不要凭空编造 plan_id/phase_id/step_id，必须基于前一步返回值。
- 简单任务（1-2步）不需要建立计划，直接执行。3步及以上才用 Todo。
- summary 不需要任何参数。

[LS 工具规范]
调用 LS 时必须提供 path 参数。如：{\"path\": \"C:\\\\Users\\\\yxpil\\\\Desktop\"}

[交流风格 / Style]
- 简洁专业，不废话
- 中英双语 / Bilingual: 重要信息同时显示中文和英文
- 做错了直接认，先确认意图再动手
";
