//! 消息列表管理 / Message list management
//!
//! 控制层的消息管道：组装 system prompt → 注入记忆 → 加载历史 → 提交 LLM。
//! Control layer message pipeline: system prompt → memory injection → history → LLM.

use crate::Net::OPENAI::Message as ApiMessage;
use crate::Net::OPENAI::Role;
use crate::config::Config;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════
// 消息列表
// ═══════════════════════════════════════════

/// 消息列表 — LLM 请求的消息管道
#[derive(Debug, Clone)]
pub struct MessageList {
    messages: Vec<ApiMessage>,
    /// 原始系统提示词（不含注入内容）
    system_prompt_raw: String,
    /// 总 token 估算
    estimated_tokens: usize,
    /// 最大 token 限制
    max_tokens: usize,
}

impl MessageList {
    /// 创建空消息列表
    pub fn new(config: &Config) -> Self {
        let max_tokens = config.llm.context_window as usize;
        let system_prompt = config.system_prompt.clone();

        let mut list = Self {
            messages: Vec::new(),
            system_prompt_raw: system_prompt,
            estimated_tokens: 0,
            max_tokens,
        };

        // 预留 30% 给响应
        let effective_max = (max_tokens as f64 * 0.7) as usize;
        list.max_tokens = effective_max;

        list
    }

    /// 构建系统消息（含注入内容）
    pub fn build_system(&self, injections: &[String]) -> ApiMessage {
        let mut content = self.system_prompt_raw.clone();

        if !injections.is_empty() {
            content.push_str("\n\n[上下文注入 / Context Injection]\n");
            for (i, inj) in injections.iter().enumerate() {
                content.push_str(&format!("{}. {}\n", i + 1, inj));
            }
        }

        ApiMessage {
            role: Role::System,
            content,
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// 添加用户消息
    pub fn push_user(&mut self, content: &str) {
        self.messages.push(ApiMessage::user(content));
        self.estimated_tokens += estimate_tokens(content);
    }

    /// 添加助手消息
    pub fn push_assistant(&mut self, content: &str) {
        self.messages.push(ApiMessage::assistant(content));
        self.estimated_tokens += estimate_tokens(content);
    }

    /// 添加系统消息（罕见，通常只在开头一次）
    pub fn push_system(&mut self, content: &str) {
        self.messages.push(ApiMessage::system(content));
        self.estimated_tokens += estimate_tokens(content);
    }

    /// 批量加载历史消息
    pub fn load_history(&mut self, messages: &[(String, String)]) {
        for (role, content) in messages {
            match role.as_str() {
                "user" => self.push_user(content),
                "assistant" => self.push_assistant(content),
                "system" => self.push_system(content),
                _ => {}
            }
        }
    }

    /// 从 DB 会话加载最近 N 条消息
    pub fn load_from_session(
        &mut self,
        db: &crate::DB::ChatDB,
        session_id: &str,
        limit: usize,
    ) -> Result<(), String> {
        let msgs = db.recent_messages(session_id, limit)?;
        for msg in msgs {
            match msg.role.as_str() {
                "user" => self.push_user(&msg.content),
                "assistant" => self.push_assistant(&msg.content),
                "system" => self.push_system(&msg.content),
                _ => {}
            }
        }
        Ok(())
    }

    /// 获取完整消息列表（用于 LLM API 调用）
    /// 返回: Vec<(role, content)> 其中第一条是 system
    pub fn to_api_messages(&self, injections: &[String]) -> Vec<ApiMessage> {
        let mut full = Vec::with_capacity(self.messages.len() + 1);
        full.push(self.build_system(injections));
        full.extend(self.messages.clone());
        full
    }

    /// 检查是否接近 token 限制
    pub fn is_near_limit(&self) -> bool {
        self.estimated_tokens > self.max_tokens.saturating_sub(1000)
    }

    /// 超过限制时自动截断旧消息
    pub fn auto_truncate(&mut self, keep_recent: usize) -> usize {
        if !self.is_near_limit() {
            return 0;
        }

        let to_remove = self.messages.len().saturating_sub(keep_recent);
        if to_remove == 0 {
            return 0;
        }

        // 计算被移除消息的 token 估算
        let mut removed_tokens = 0;
        for msg in self.messages.drain(..to_remove) {
            removed_tokens += estimate_tokens(&msg.content);
        }
        self.estimated_tokens = self.estimated_tokens.saturating_sub(removed_tokens);

        to_remove
    }

    // ─── 状态查询 ───

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn estimated_tokens(&self) -> usize {
        self.estimated_tokens
    }

    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// 获取最近 N 条消息（不含 system）
    pub fn recent(&self, n: usize) -> &[ApiMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    /// 清空消息列表（保留 system prompt）
    pub fn clear(&mut self) {
        self.messages.clear();
        self.estimated_tokens = 0;
    }
}

// ═══════════════════════════════════════════
// 上下文构建器
// ═══════════════════════════════════════════

/// 上下文构建器 — 组装每次 LLM 调用前的完整上下文
pub struct ContextBuilder {
    /// 系统注入（每次调用动态生成）
    injections: Vec<String>,
    /// 最近一次构建耗时
    last_build_time: Duration,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self {
            injections: Vec::new(),
            last_build_time: Duration::ZERO,
        }
    }

    /// 注入一条上下文（如当前时间、工作目录等）
    pub fn inject(&mut self, context: &str) {
        self.injections.push(context.to_string());
    }

    /// 从贝叶斯记忆库注入相关记忆
    pub fn inject_memories(&mut self, query: &str, db: &crate::DB::SapNiDB, limit: usize) {
        let memories = db.recall(query, limit);
        for mem in memories {
            self.injections.push(format!(
                "[记忆/Memory] (重要度:{}) {}",
                mem.importance, mem.content
            ));
        }
    }

    /// 注入当前环境信息
    pub fn inject_environment(&mut self) {
        let now = chrono_now();
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".into());
        let os = std::env::consts::OS;

        self.injections.push(format!(
            "当前时间: {now} | 操作系统: {os} | 工作目录: {cwd}"
        ));
    }

    /// 注入工具列表
    pub fn inject_tools(&mut self, tool_names: &[String]) {
        if !tool_names.is_empty() {
            self.injections
                .push(format!("可用工具: {}", tool_names.join(", ")));
        }
    }

    /// 构建最终消息列表（含 system + 注入 + 历史）
    pub fn build(&self, message_list: &MessageList) -> Vec<ApiMessage> {
        let start = Instant::now();
        let result = message_list.to_api_messages(&self.injections);
        // self.last_build_time = start.elapsed(); // would need &mut self
        let _ = start;
        result
    }

    /// 清空注入
    pub fn clear_injections(&mut self) {
        self.injections.clear();
    }

    pub fn injection_count(&self) -> usize {
        self.injections.len()
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════
// 对话管理 / Conversation Manager
// ═══════════════════════════════════════════

/// 对话管理器 — 统一管理会话生命周期
pub struct Conversation {
    pub session_id: String,
    pub messages: MessageList,
    pub builder: ContextBuilder,
    /// 对话轮数
    pub turn: u64,
    /// 是否激活
    pub active: bool,
}

impl Conversation {
    /// 创建新对话
    pub fn new(config: &Config, db: &mut crate::DB::ChatDB, title: &str, tags: &[String]) -> Self {
        let session_id = db.create_session(title, tags);
        Self {
            session_id,
            messages: MessageList::new(config),
            builder: ContextBuilder::new(),
            turn: 0,
            active: true,
        }
    }

    /// 加载已有会话
    pub fn resume(
        config: &Config,
        db: &crate::DB::ChatDB,
        session_id: &str,
        history_limit: usize,
    ) -> Result<Self, String> {
        let session = db
            .get_session(session_id)
            .ok_or_else(|| format!("会话不存在 / Session not found: {session_id}"))?;

        let mut messages = MessageList::new(config);
        messages.load_from_session(db, session_id, history_limit)?;

        Ok(Self {
            session_id: session_id.to_string(),
            messages,
            builder: ContextBuilder::new(),
            turn: session.messages.len() as u64 / 2,
            active: true,
        })
    }

    /// 处理用户输入 — 返回组装好的消息列表
    pub fn process_input(&mut self, user_input: &str, db: &crate::DB::SapNiDB) -> Vec<ApiMessage> {
        self.turn += 1;

        // 1. 清空上次注入
        self.builder.clear_injections();

        // 2. 注入环境信息
        self.builder.inject_environment();

        // 3. 从贝叶斯记忆联想相关上下文
        self.builder.inject_memories(user_input, db, 3);

        // 4. 添加用户消息到历史
        self.messages.push_user(user_input);

        // 5. 自动截断
        let removed = self.messages.auto_truncate(20);
        if removed > 0 {
            self.builder.inject(&format!(
                "[注意] 已自动截断 {removed} 条旧消息以保持在 token 限制内"
            ));
        }

        // 6. 构建
        self.builder.build(&self.messages)
    }

    /// 记录助手回复
    pub fn record_response(
        &mut self,
        db: &mut crate::DB::SapNiDB,
        response: &str,
    ) -> Result<(), String> {
        self.messages.push_assistant(response);
        db.chat.add_message(&self.session_id, "assistant", response)?;
        db.auto_remember(&self.session_id, response, "assistant");
        Ok(())
    }

    /// 保存用户消息到 DB
    pub fn record_user_message(
        &mut self,
        db: &mut crate::DB::SapNiDB,
        content: &str,
    ) -> Result<(), String> {
        db.chat.add_message(&self.session_id, "user", content)?;
        db.auto_remember(&self.session_id, content, "user");
        Ok(())
    }

    /// 结束对话
    pub fn end(&mut self) {
        self.active = false;
    }
}

// ═══════════════════════════════════════════
// Token 估算
// ═══════════════════════════════════════════

/// 粗略估算文本的 token 数
/// 中文：约 1.5 字/token，英文：约 4 字/token
fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let cjk: usize = text.chars().filter(|c| *c as u32 > 0x2E80).count();
    let ascii = chars - cjk;
    // 中文 ~1.5 字/token, 英文 ~4 字/token
    (cjk as f64 / 1.5 + ascii as f64 / 4.0).ceil() as usize
}

fn chrono_now() -> String {
    // 简易时间戳，避免额外依赖
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // 粗略格式化: 转为 UTC+8 (北京时间)
    let total_secs = secs + 8 * 3600;
    let days = total_secs / 86400;
    let time_of_day = total_secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // 简易日期计算（从 Unix epoch 1970-01-01 起算）
    let (year, month, day) = days_to_ymd(days as i64);

    format!("{year}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02}")
}

fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    // 从 1970-01-01 起的粗略日期计算
    let mut year = 1970i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &md in &month_days {
        if days < md as i64 {
            break;
        }
        days -= md as i64;
        month += 1;
    }

    (year, month, (days + 1) as u32)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ═══════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        let mut cfg = Config::default_config();
        cfg.llm.context_window = 8000;
        cfg.system_prompt = "你是一个测试助手 / You are a test assistant.".into();
        cfg
    }

    #[test]
    fn test_message_list_basic() {
        let cfg = test_config();
        let mut list = MessageList::new(&cfg);

        list.push_user("你好");
        list.push_assistant("你好！");

        assert_eq!(list.len(), 2);
        assert!(list.estimated_tokens() > 0);
    }

    #[test]
    fn test_to_api_messages() {
        let cfg = test_config();
        let mut list = MessageList::new(&cfg);
        list.push_user("测试");

        let api_msgs = list.to_api_messages(&[]);
        assert_eq!(api_msgs.len(), 2); // system + user
        assert!(matches!(api_msgs[0].role, Role::System));
        assert!(matches!(api_msgs[1].role, Role::User));
    }

    #[test]
    fn test_system_injection() {
        let cfg = test_config();
        let list = MessageList::new(&cfg);

        let api_msgs = list.to_api_messages(&["当前时间: 2026-01-01".into()]);
        let sys_content = &api_msgs[0].content;
        assert!(sys_content.contains("2026-01-01"));
        assert!(sys_content.contains("上下文注入"));
    }

    #[test]
    fn test_auto_truncate() {
        let mut cfg = test_config();
        cfg.llm.context_window = 200; // 极小窗口
        let mut list = MessageList::new(&cfg);

        // 填满
        for i in 0..20 {
            list.push_user(&format!("消息内容 {}", i));
        }

        assert!(list.is_near_limit());
        let removed = list.auto_truncate(5);
        assert!(removed > 0);
        assert_eq!(list.len(), 5);
    }

    #[test]
    fn test_context_builder_inject() {
        let cfg = test_config();
        let list = MessageList::new(&cfg);
        let mut builder = ContextBuilder::new();

        builder.inject("工作目录: /home/user");
        builder.inject_tools(&["read".into(), "write".into()]);

        let api_msgs = builder.build(&list);
        let sys = &api_msgs[0].content;
        assert!(sys.contains("工作目录"));
        assert!(sys.contains("可用工具"));
    }

    #[test]
    fn test_estimate_tokens() {
        // 英文
        let en = estimate_tokens("Hello world this is a test message");
        assert!(en > 0 && en < 30);

        // 中文
        let zh = estimate_tokens("这是一条测试消息用于估算token数量");
        assert!(zh > 0 && zh < 30);
    }

    #[test]
    fn test_chrono_now() {
        let now = chrono_now();
        assert!(now.contains("202"));
        assert!(now.contains(":"));
    }

    #[test]
    fn test_conversation_lifecycle() {
        // 需要 DB 的完整测试已在 DB 模块覆盖
        // 此处验证基础流程
        let cfg = test_config();
        let mut list = MessageList::new(&cfg);
        list.push_user("hello");
        list.push_assistant("hi");
        assert_eq!(list.len(), 2);
        list.clear();
        assert_eq!(list.len(), 0);
    }
}
