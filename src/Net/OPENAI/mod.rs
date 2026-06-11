//! OpenAI-compatible API HTTP/HTTPS client
//! 所有参数由控制层传入 / All params from Control layer.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::BufRead;
use std::time::Duration;

// ═══════════════════════════════════════════
// 类型定义 / Type definitions
// ═══════════════════════════════════════════

pub struct Client {
    base_url: String, // "https://api.openai.com/v1"
    api_key: String,
    agent: ureq::Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    // name 始终序列化 — DeepSeek V4 要求 assistant 消息必须带
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

impl Default for ChatRequest {
    fn default() -> Self {
        Self {
            model: String::new(),
            messages: Vec::new(),
            temperature: None,
            top_p: None,
            max_tokens: None,
            max_completion_tokens: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            stream: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            user: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<i64>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    #[serde(skip)]
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: Option<i32>,
    pub message: Option<ChoiceMessage>,
    pub delta: Option<ChoiceDelta>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChoiceMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChoiceDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelListResponse {
    pub object: Option<String>,
    pub data: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: Option<String>,
    pub created: Option<i64>,
    pub owned_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingResponse {
    pub object: Option<String>,
    pub data: Vec<EmbeddingData>,
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingData {
    pub object: Option<String>,
    pub index: Option<u32>,
    pub embedding: Vec<f64>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Chunk(ChatResponse),
    Done,
    Error(String),
}

#[derive(Debug, Clone, Deserialize)]
struct ApiErrorBody {
    error: Option<ApiErrorDetail>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiErrorDetail {
    message: String,
    #[serde(rename = "type")]
    type_: Option<String>,
    code: Option<String>,
}

// ═══════════════════════════════════════════
// Client 实现
// ═══════════════════════════════════════════

impl Client {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(120))
            .timeout_write(Duration::from_secs(60))
            .build();
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            agent,
        }
    }

    /// 动态更新 API Key
    pub fn set_api_key(&mut self, key: impl Into<String>) {
        self.api_key = key.into();
    }

    /// 动态更新 Base URL
    pub fn set_base_url(&mut self, url: impl Into<String>) {
        self.base_url = url.into().trim_end_matches('/').to_string();
    }

    /// Chat Completion（非流式）
    pub fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        let url = format!("{}/chat/completions", self.base_url);
        let body =
            serde_json::to_string(req).map_err(|e| format!("序列化失败 / Serialize: {e}"))?;

        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_string(&body)
            .map_err(|e| map_err(e, &url))?;

        let status = resp.status();
        let raw = resp
            .into_string()
            .map_err(|e| format!("读取响应 / Read: {e}"))?;

        if status >= 400 {
            return Err(parse_err(&raw, status));
        }

        let mut cr: ChatResponse = serde_json::from_str(&raw)
            .map_err(|e| format!("JSON 解析 / Parse: {e}\nRaw: {}", trunc(&raw, 200)))?;
        cr.raw = serde_json::from_str(&raw).ok();
        Ok(cr)
    }

    /// Chat Completion（流式 SSE 收集为完整响应 + 实时打印）
    pub fn chat_stream_collect(
        &self,
        req: &ChatRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<ChatResponse, String> {
        let stream = self.chat_stream(req)?;
        let mut content = String::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        let mut id = String::new();
        let mut model = String::new();
        let mut finish_reason = String::new();

        for event in stream {
            match event {
                StreamEvent::Chunk(chunk) => {
                    if let Some(ref cid) = chunk.id { id = cid.clone(); }
                    if let Some(ref m) = chunk.model { model = m.clone(); }
                    for choice in &chunk.choices {
                        if let Some(ref delta) = choice.delta {
                            if let Some(ref text) = delta.content {
                                on_token(text);
                                content.push_str(text);
                            }
                            // SSE 中 tool_calls 分多块 delta 发送，按 index 合并
                            if let Some(ref delta_tc) = delta.tool_calls {
                                merge_tool_call_deltas(&mut tool_calls, delta_tc);
                            }
                        }
                        if let Some(ref f) = choice.finish_reason {
                            finish_reason = f.clone();
                        }
                    }
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => return Err(e),
            }
        }

        let tc_val = if tool_calls.is_empty() {
            None
        } else {
            Some(Value::Array(tool_calls))
        };

        let choice_msg = ChoiceMessage {
            role: Some("assistant".into()),
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls: tc_val,
        };

        Ok(ChatResponse {
            id: if id.is_empty() { None } else { Some(id) },
            object: Some("chat.completion".into()),
            created: None,
            model: if model.is_empty() { None } else { Some(model) },
            choices: vec![Choice {
                index: Some(0),
                message: Some(choice_msg),
                delta: None,
                finish_reason: if finish_reason.is_empty() { None } else { Some(finish_reason) },
            }],
            usage: None,
            raw: None,
        })
    }

    /// Chat Completion（流式 SSE 原始迭代器）
    pub fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<impl Iterator<Item = StreamEvent>, String> {
        let mut req = req.clone();
        req.stream = Some(true);

        let url = format!("{}/chat/completions", self.base_url);
        let body =
            serde_json::to_string(&req).map_err(|e| format!("序列化失败 / Serialize: {e}"))?;

        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_string(&body)
            .map_err(|e| map_err(e, &url))?;

        let status = resp.status();
        if status >= 400 {
            let raw = resp.into_string().unwrap_or_default();
            return Err(parse_err(&raw, status));
        }

        Ok(SseParser::new(resp.into_reader()))
    }

    /// 模型列表
    pub fn list_models(&self) -> Result<ModelListResponse, String> {
        let url = format!("{}/models", self.base_url);
        let resp = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .call()
            .map_err(|e| map_err(e, &url))?;

        let status = resp.status();
        let raw = resp
            .into_string()
            .map_err(|e| format!("读取响应 / Read: {e}"))?;
        if status >= 400 {
            return Err(parse_err(&raw, status));
        }
        serde_json::from_str(&raw).map_err(|e| format!("JSON 解析 / Parse: {e}"))
    }

    /// 单个模型信息
    pub fn get_model(&self, model_id: &str) -> Result<ModelInfo, String> {
        let url = format!("{}/models/{model_id}", self.base_url);
        let resp = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .call()
            .map_err(|e| map_err(e, &url))?;

        let status = resp.status();
        let raw = resp
            .into_string()
            .map_err(|e| format!("读取响应 / Read: {e}"))?;
        if status >= 400 {
            return Err(parse_err(&raw, status));
        }
        serde_json::from_str(&raw).map_err(|e| format!("JSON 解析 / Parse: {e}"))
    }

    /// Embeddings
    pub fn embeddings(&self, req: &EmbeddingRequest) -> Result<EmbeddingResponse, String> {
        let url = format!("{}/embeddings", self.base_url);
        let body =
            serde_json::to_string(req).map_err(|e| format!("序列化失败 / Serialize: {e}"))?;

        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_string(&body)
            .map_err(|e| map_err(e, &url))?;

        let status = resp.status();
        let raw = resp
            .into_string()
            .map_err(|e| format!("读取响应 / Read: {e}"))?;
        if status >= 400 {
            return Err(parse_err(&raw, status));
        }
        serde_json::from_str(&raw).map_err(|e| format!("JSON 解析 / Parse: {e}"))
    }
}

// ═══════════════════════════════════════════
// SSE 流式解析器
// ═══════════════════════════════════════════

struct SseParser<R: std::io::Read> {
    reader: std::io::BufReader<R>,
    buffer: String,
    done: bool,
}

impl<R: std::io::Read> SseParser<R> {
    fn new(reader: R) -> Self {
        Self {
            reader: std::io::BufReader::new(reader),
            buffer: String::new(),
            done: false,
        }
    }
}

impl<R: std::io::Read> Iterator for SseParser<R> {
    type Item = StreamEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            self.buffer.clear();
            match self.reader.read_line(&mut self.buffer) {
                Ok(0) => {
                    self.done = true;
                    return Some(StreamEvent::Done);
                }
                Ok(_) => {
                    let line = self.buffer.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if line == "data: [DONE]" {
                        self.done = true;
                        return Some(StreamEvent::Done);
                    }
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<ChatResponse>(json_str) {
                            Ok(mut chunk) => {
                                chunk.raw = serde_json::from_str(json_str).ok();
                                return Some(StreamEvent::Chunk(chunk));
                            }
                            Err(e) => {
                                return Some(StreamEvent::Error(format!(
                                    "SSE 解析 / Parse: {e} | data: {}",
                                    trunc(json_str, 200)
                                )));
                            }
                        }
                    }
                }
                Err(e) => {
                    self.done = true;
                    return Some(StreamEvent::Error(format!("SSE 读取 / Read: {e}")));
                }
            }
        }
    }
}

// ═══════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════

fn map_err(e: ureq::Error, url: &str) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            parse_err(&body, code)
        }
        ureq::Error::Transport(t) => format!("网络错误 / Network ({url}): {t}"),
    }
}

fn parse_err(raw: &str, status: u16) -> String {
    match serde_json::from_str::<ApiErrorBody>(raw) {
        Ok(ae) => {
            let msg = ae
                .error
                .map(|d| d.message)
                .unwrap_or_else(|| "未知错误".into());
            format!("API 错误 ({status}): {msg}")
        }
        Err(_) => format!("API 错误 ({status}): {}", trunc(raw, 200)),
    }
}

fn trunc(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

/// 合并 SSE 流中的 tool_calls delta，按 index 字段归并
fn merge_tool_call_deltas(acc: &mut Vec<Value>, delta: &Value) {
    let delta_arr = match delta.as_array() {
        Some(a) => a,
        None => return,
    };
    for delta_tc in delta_arr {
        let idx = delta_tc
            .get("index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        while acc.len() <= idx {
            acc.push(serde_json::json!({}));
        }
        merge_json_deep(&mut acc[idx], delta_tc);
    }
}

/// 深度合并两个 JSON 对象，source 覆盖/合并到 target
/// DeepSeek V4 将 tool_calls arguments 逐 token 流式发送，需拼接字符串
fn merge_json_deep(target: &mut Value, source: &Value) {
    if target.is_object() && source.is_object() {
        let t = target.as_object_mut().unwrap();
        let s = source.as_object().unwrap();
        let merge_keys: Vec<String> = s.keys()
            .filter(|k| t.contains_key(k.clone()))
            .cloned()
            .collect();
        for k in &merge_keys {
            let sv = s.get(k).unwrap().clone();
            merge_json_deep(t.get_mut(k).unwrap(), &sv);
        }
        for (k, v) in s {
            if !t.contains_key(k) {
                t.insert(k.clone(), v.clone());
            }
        }
    } else if target.is_string() && source.is_string() {
        // 字符串拼接：DeepSeek V4 流式 tool_calls arguments 逐 token 发送
        let t = target.as_str().unwrap();
        let s = source.as_str().unwrap();
        *target = Value::String(format!("{t}{s}"));
    } else {
        *target = source.clone();
    }
}

// ═══════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_ctors() {
        assert_eq!(Message::user("hi").content, "hi");
        assert_eq!(Message::system("sys").content, "sys");
    }

    #[test]
    fn test_name_always_serialized() {
        // 验证 name 字段始终出现在 JSON 中（即使是 None）
        let user_msg = Message::user("hello");
        let user_json = serde_json::to_string(&user_msg).unwrap();
        assert!(user_json.contains("\"name\""), "user message missing name: {user_json}");

        let assistant_msg = Message::assistant("hi");
        let assistant_json = serde_json::to_string(&assistant_msg).unwrap();
        assert!(assistant_json.contains("\"name\""), "assistant missing name: {assistant_json}");

        // 带 tool_calls 的 assistant 消息
        let tc_msg = Message {
            role: Role::Assistant,
            content: String::new(),
            name: Some("WRITE".into()),
            tool_call_id: None,
            tool_calls: Some(serde_json::json!([{
                "id": "call_1",
                "type": "function",
                "function": {"name": "WRITE", "arguments": "{}"}
            }])),
        };
        let tc_json = serde_json::to_string(&tc_msg).unwrap();
        eprintln!("tool_call assistant JSON: {tc_json}");
        assert!(tc_json.contains("\"name\":\"WRITE\""), "bad name in: {tc_json}");
        assert!(tc_json.contains("tool_calls"), "missing tool_calls");
    }

    #[test]
    fn test_chat_request_serialize() {
        let req = ChatRequest {
            model: "m1".into(),
            messages: vec![Message::user("hi")],
            temperature: Some(0.7),
            ..Default::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("m1") && json.contains("0.7") && json.contains("user"));
    }

    #[test]
    fn test_parse_err() {
        let raw = r#"{"error":{"message":"Bad key","type":"auth"}}"#;
        let e = parse_err(raw, 401);
        assert!(e.contains("Bad key") && e.contains("401"));
    }

    #[test]
    fn test_sse_done() {
        let input = b"data: [DONE]\n";
        let events: Vec<_> = SseParser::new(&input[..]).collect();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], StreamEvent::Done));
    }

    #[test]
    fn test_sse_chunk() {
        let input = br#"data: {"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}

data: [DONE]
"#;
        let events: Vec<_> = SseParser::new(&input[..]).collect();
        assert_eq!(events.len(), 2);
        if let StreamEvent::Chunk(c) = &events[0] {
            assert_eq!(
                c.choices[0]
                    .delta
                    .as_ref()
                    .unwrap()
                    .content
                    .as_ref()
                    .unwrap(),
                "Hi"
            );
        } else {
            panic!("expected chunk");
        }
    }

    /// DeepSeek V4 要求：assistant 消息带 tool_calls 时必须有 name 字段
    #[test]
    fn test_deepseek_assistant_with_tool_calls_has_name() {
        let msg = Message {
            role: Role::Assistant,
            content: String::new(),
            name: Some("WRITE".into()),
            tool_call_id: None,
            tool_calls: Some(serde_json::json!([{
                "id": "call_001",
                "type": "function",
                "function": {
                    "name": "WRITE",
                    "arguments": "{\"path\":\"test.txt\"}"
                }
            }])),
        };
        let json = serde_json::to_string(&msg).unwrap();
        // name 必须在 JSON 中
        assert!(json.contains("WRITE"), "missing name: {json}");
        // 验证完整往返
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, Some("WRITE".to_string()));
        assert!(parsed.tool_calls.is_some());
    }

    /// DeepSeek V4 要求：tool 消息必须有 tool_call_id
    #[test]
    fn test_deepseek_tool_message_has_tool_call_id() {
        let msg = Message {
            role: Role::Tool,
            content: "OK: done".into(),
            name: None,
            tool_call_id: Some("call_001".into()),
            tool_calls: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("tool_call_id"), "missing tool_call_id: {json}");
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_call_id, Some("call_001".into()));
    }

    /// 普通消息不应包含 tool 字段
    #[test]
    fn test_user_message_no_tool_fields() {
        let msg = Message::user("hello");
        let json = serde_json::to_string(&msg).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("tool_calls").is_none());
        assert!(v.get("tool_call_id").is_none());
    }

    /// 集成测试：模拟 Agent 发送给 LLM 的完整 ChatRequest JSON
    #[test]
    fn test_full_chat_request_with_tool_call() {
        // 模拟 Agent 第二轮的 messages：[system, user, assistant+tool_calls, tool_result]
        let messages = vec![
            Message::system("You are SapNi."),
            Message::user("create file test.txt"),
            Message {
                role: Role::Assistant,
                content: String::new(),
                name: Some("WRITE".into()),
                tool_call_id: None,
                tool_calls: Some(serde_json::json!([{
                    "id": "call_abc",
                    "type": "function",
                    "function": {
                        "name": "WRITE",
                        "arguments": "{\"path\":\"test.txt\",\"content\":\"hello\"}"
                    }
                }])),
            },
            Message {
                role: Role::Tool,
                content: "OK: written".into(),
                name: None,
                tool_call_id: Some("call_abc".into()),
                tool_calls: None,
            },
        ];

        let req = ChatRequest {
            model: "deepseek-v4-pro".into(),
            messages,
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&req).unwrap();
        eprintln!("=== FULL CHATREQUEST JSON ===\n{json}\n=== END ===");

        // 关键验证：messages[2]（assistant）必须有 name 字段
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        let asst = &msgs[2];
        assert!(asst.get("name").is_some(), "messages[2] missing name: {asst}");
        assert_eq!(asst["name"], "WRITE");
        assert!(asst.get("tool_calls").is_some(), "messages[2] missing tool_calls");

        // messages[3]（tool）必须有 tool_call_id
        let tool = &msgs[3];
        assert!(tool.get("tool_call_id").is_some(), "messages[3] missing tool_call_id: {tool}");
        assert_eq!(tool["tool_call_id"], "call_abc");
    }

    /// 测试 SSE tool_calls delta 合并 — 模拟 DeepSeek V4 逐 token 流式
    #[test]
    fn test_merge_tool_call_deltas() {
        let mut acc: Vec<Value> = Vec::new();

        // Delta 1: id + function name
        merge_tool_call_deltas(&mut acc, &serde_json::json!([{
            "index": 0,
            "id": "call_001",
            "type": "function",
            "function": {"name": "WRITE", "arguments": ""}
        }]));

        // Delta 2-N: arguments 逐 token 发送（DeepSeek V4 真实行为）
        let tokens = vec!["{\"", "path", "\":", "\"", "test", ".", "txt", "\"", ",", "\"con", "tent", "\":", "\"", "hello", "\"", "}"];
        for tok in tokens {
            merge_tool_call_deltas(&mut acc, &serde_json::json!([{
                "index": 0,
                "function": {"arguments": tok}
            }]));
        }

        assert_eq!(acc.len(), 1);
        assert_eq!(acc[0]["function"]["name"], "WRITE");
        // 拼接后应为完整 JSON
        assert_eq!(acc[0]["function"]["arguments"], r#"{"path":"test.txt","content":"hello"}"#);

        // 多个 tool_calls
        merge_tool_call_deltas(&mut acc, &serde_json::json!([{
            "index": 1,
            "id": "call_002",
            "type": "function",
            "function": {"name": "EXEC", "arguments": "{\"cmd\":\"dir\"}"}
        }]));
        assert_eq!(acc.len(), 2);
        assert_eq!(acc[0]["function"]["name"], "WRITE");
        assert_eq!(acc[1]["function"]["name"], "EXEC");
    }
}
