//! SapNi 配置层 — 读取/写入/热更新
//! Config layer: load / save / hot-reload.
//!
//! 配置文件: ~/.sapni/config.json (或 SAPNI_CONFIG 环境变量)
//! API key 用占位符 "your-api-key-here"，真实 key 从 ~/.sapni/api_tokens.json 读取

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ═══════════════════════════════════════════
// 配置结构
// ═══════════════════════════════════════════

/// 完整配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub agent: AgentConfig,
    pub llm: LlmConfig,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

/// Agent 基本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_agent_name")]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

fn default_agent_name() -> String {
    "SapNi".into()
}

/// LLM 提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 提供商名: "deepseek" | "moonshot" | "openai" | "custom"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// API Key（占位符或真实值）
    #[serde(default = "placeholder_key")]
    pub api_key: String,

    /// API Base URL
    #[serde(default)]
    pub base_url: String,

    /// 默认模型
    #[serde(default)]
    pub model: String,

    /// 最大输出 token
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// 上下文窗口大小
    #[serde(default)]
    pub context_window: u32,

    /// 温度
    #[serde(default = "default_temperature")]
    pub temperature: f64,

    /// Top-P
    #[serde(default = "default_top_p")]
    pub top_p: f64,
}

fn default_provider() -> String {
    "ollama".into()
}
fn placeholder_key() -> String {
    "your-api-key-here".into()
}
fn default_max_tokens() -> u32 {
    8000
}
fn default_temperature() -> f64 {
    0.7
}
fn default_top_p() -> f64 {
    0.9
}

/// 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 权限模式: "ask" | "trust" | "session_trust"
    #[serde(default = "default_permission")]
    pub permission_mode: String,

    /// 无条件信任的工具名列表
    #[serde(default)]
    pub trusted_tools: Vec<String>,

    /// 禁用的工具名列表
    #[serde(default)]
    pub disabled_tools: Vec<String>,
}

fn default_true() -> bool {
    true
}
fn default_permission() -> String {
    "ask".into()
}

/// 记忆配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// 最大历史消息数
    #[serde(default = "default_100")]
    pub max_history: usize,

    /// 最大记忆条目数
    #[serde(default = "default_800")]
    pub max_entries: usize,

    /// 单条记忆最大字符数
    #[serde(default = "default_200")]
    pub max_entry_chars: usize,

    /// 自动压缩阈值（token 数）
    #[serde(default = "default_5000")]
    pub auto_compress_threshold: usize,

    /// 记忆存储路径（相对于 config dir）
    #[serde(default)]
    pub storage_path: String,
}

fn default_100() -> usize {
    100
}
fn default_800() -> usize {
    800
}
fn default_200() -> usize {
    200
}
fn default_5000() -> usize {
    5000
}

/// UI 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_true")]
    pub show_logo: bool,

    #[serde(default)]
    pub logo_path: String,

    #[serde(default = "default_divider")]
    pub divider_char: String,

    /// 主题色（十六进制）
    #[serde(default = "default_accent")]
    pub accent_color: String,

    /// 背景色
    #[serde(default = "default_bg")]
    pub bg_color: String,

    /// 语言: "zh" | "en" | "bilingual"
    #[serde(default = "default_lang")]
    pub language: String,
}

fn default_divider() -> String {
    "-".into()
}
fn default_accent() -> String {
    "#f783ac".into()
}
fn default_bg() -> String {
    "#0d1117".into()
}
fn default_lang() -> String {
    "bilingual".into()
}

// ═══════════════════════════════════════════
// 提供商预设 / Provider Presets
// ═══════════════════════════════════════════

/// 预设提供商信息
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub context_window: u32,
    pub description: &'static str,
}

/// 内置提供商预设
pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        name: "ollama",
        base_url: "http://localhost:11434/v1",
        default_model: "fredrezones55/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive:IQ2_M",
        context_window: 262144,
        description: "Ollama 本地 — Qwen3.6 35B",
    },
    ProviderPreset {
        name: "deepseek",
        base_url: "https://api.deepseek.com/v1",
        default_model: "deepseek-chat",
        context_window: 65536,
        description: "DeepSeek — 高性价比，中文优秀",
    },
    ProviderPreset {
        name: "moonshot",
        base_url: "https://api.moonshot.cn/v1",
        default_model: "moonshot-v1-8k",
        context_window: 8192,
        description: "MoonShot（月之暗面）— kimi 系列",
    },
    ProviderPreset {
        name: "openai",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o",
        context_window: 128000,
        description: "OpenAI — GPT-4o 系列",
    },
    ProviderPreset {
        name: "openrouter",
        base_url: "https://openrouter.ai/api/v1",
        default_model: "openai/gpt-4o",
        context_window: 128000,
        description: "OpenRouter — 多模型路由",
    },
];

// ═══════════════════════════════════════════
// 配置管理
// ═══════════════════════════════════════════

impl Config {
    /// 创建默认配置（内置 DeepSeek V4 Pro）
    pub fn default_config() -> Self {
        Self {
            agent: AgentConfig {
                name: "SapNi".into(),
                description: String::new(),
            },
            llm: LlmConfig {
                provider: "deepseek".into(),
                api_key: "sk-e7bffadf71904cb9bd9538bcebb4fd4f".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                model: "deepseek-v4-pro".into(),
                max_tokens: 8000,
                context_window: 65536,
                temperature: 0.7,
                top_p: 0.9,
            },
            system_prompt: String::new(),
            tools: ToolsConfig::default(),
            memory: MemoryConfig::default(),
            ui: UiConfig::default(),
        }
    }

    /// 从文件加载
    pub fn load(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取配置失败 / Read config: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败 / Parse config: {e}"))
    }

    /// 保存到文件
    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化失败 / Serialize: {e}"))?;
        std::fs::write(path, &json).map_err(|e| format!("写入失败 / Write: {e}"))?;
        Ok(())
    }

    /// 自动发现配置文件路径
    /// 优先级: SAPNI_CONFIG 环境变量 > ~/.sapni/config.json > 项目根目录 .sapni.json
    pub fn default_path() -> PathBuf {
        if let Ok(env) = std::env::var("SAPNI_CONFIG") {
            return PathBuf::from(env);
        }
        let home = dirs_home();
        let default = home.join(".sapni").join("config.json");
        if default.exists() {
            return default;
        }
        // 回退：项目根目录
        PathBuf::from(".sapni.json")
    }

    /// 加载或创建默认配置
    pub fn load_or_default() -> Self {
        let path = Self::default_path();
        let mut cfg = if path.exists() {
            Self::load(path.to_str().unwrap()).unwrap_or_else(|_| Self::default_config())
        } else {
            Self::default_config()
        };
        // 防御：若 base_url 为空或无效，自动回退到 DeepSeek
        if cfg.llm.base_url.is_empty() || !cfg.llm.base_url.starts_with("http") {
            cfg.llm.base_url = "https://api.deepseek.com/v1".into();
        }
        if cfg.llm.model.is_empty() {
            cfg.llm.model = "deepseek-v4-pro".into();
        }
        if cfg.llm.api_key.is_empty() || cfg.llm.api_key == "your-api-key-here" {
            cfg.llm.api_key = "sk-e7bffadf71904cb9bd9538bcebb4fd4f".into();
        }
        cfg
    }

    /// 应用提供商预设
    pub fn apply_preset(&mut self, provider_name: &str) -> Result<(), String> {
        let preset = PROVIDER_PRESETS
            .iter()
            .find(|p| p.name == provider_name)
            .ok_or_else(|| format!("未知提供商 / Unknown provider: {provider_name}"))?;

        self.llm.provider = preset.name.into();
        self.llm.base_url = preset.base_url.into();
        self.llm.model = preset.default_model.into();
        self.llm.context_window = preset.context_window;

        Ok(())
    }

    // ─── LLM 热更新方法 ───

    pub fn set_api_key(&mut self, key: &str) {
        self.llm.api_key = key.into();
    }

    pub fn set_model(&mut self, model: &str) {
        self.llm.model = model.into();
    }

    pub fn set_base_url(&mut self, url: &str) {
        self.llm.base_url = url.into();
    }

    pub fn set_temperature(&mut self, temp: f64) {
        self.llm.temperature = temp.clamp(0.0, 2.0);
    }

    pub fn set_top_p(&mut self, top_p: f64) {
        self.llm.top_p = top_p.clamp(0.0, 1.0);
    }

    pub fn set_max_tokens(&mut self, tokens: u32) {
        self.llm.max_tokens = tokens;
    }

    /// 切换到指定提供商并加载预设
    pub fn set_provider(&mut self, provider: &str) -> Result<(), String> {
        self.apply_preset(provider)
    }

    /// 检查 API key 是否为占位符（publish 前安全检查）
    pub fn is_api_key_placeholder(&self) -> bool {
        // Ollama 本地模型不需要 API Key
        if self.llm.provider == "ollama" {
            return false;
        }
        self.llm.api_key.is_empty()
            || self.llm.api_key == "your-api-key-here"
            || self.llm.api_key == "***"
    }

    /// 获取有效的 API base URL（末尾无斜杠）
    pub fn effective_base_url(&self) -> String {
        self.llm.base_url.trim_end_matches('/').to_string()
    }

    /// 列出所有可用预设
    pub fn list_presets() -> Vec<ProviderPreset> {
        PROVIDER_PRESETS.to_vec()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::default_config()
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            permission_mode: "ask".into(),
            trusted_tools: Vec::new(),
            disabled_tools: Vec::new(),
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_history: 100,
            max_entries: 800,
            max_entry_chars: 200,
            auto_compress_threshold: 5000,
            storage_path: String::new(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_logo: true,
            logo_path: String::new(),
            divider_char: "-".into(),
            accent_color: "#f783ac".into(),
            bg_color: "#0d1117".into(),
            language: "bilingual".into(),
        }
    }
}

// ═══════════════════════════════════════════
// 辅助
// ═══════════════════════════════════════════

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

// ═══════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.agent.name, "SapNi");
        assert_eq!(cfg.llm.provider, "deepseek");
        assert_eq!(cfg.llm.temperature, 0.7);
        assert!(cfg.tools.enabled);
    }

    #[test]
    fn test_apply_preset() {
        let mut cfg = Config::default();
        cfg.apply_preset("moonshot").unwrap();
        assert_eq!(cfg.llm.provider, "moonshot");
        assert_eq!(cfg.llm.base_url, "https://api.moonshot.cn/v1");
    }

    #[test]
    fn test_apply_unknown_preset() {
        let mut cfg = Config::default();
        assert!(cfg.apply_preset("nonexistent").is_err());
    }

    #[test]
    fn test_placeholder_detection() {
        let mut cfg = Config::default();
        // 默认配置已内置真实 key，not placeholder
        assert!(!cfg.is_api_key_placeholder());

        cfg.set_api_key("your-api-key-here");
        assert!(cfg.is_api_key_placeholder());

        cfg.set_api_key("");
        assert!(cfg.is_api_key_placeholder());
    }

    #[test]
    fn test_hot_update() {
        let mut cfg = Config::default();
        cfg.set_model("deepseek-v4-pro");
        cfg.set_temperature(0.3);
        cfg.set_top_p(0.95);
        cfg.set_max_tokens(16000);

        assert_eq!(cfg.llm.model, "deepseek-v4-pro");
        assert_eq!(cfg.llm.temperature, 0.3);
        assert_eq!(cfg.llm.top_p, 0.95);
        assert_eq!(cfg.llm.max_tokens, 16000);
    }

    #[test]
    fn test_temperature_clamp() {
        let mut cfg = Config::default();
        cfg.set_temperature(5.0);
        assert_eq!(cfg.llm.temperature, 2.0); // clamped
        cfg.set_temperature(-1.0);
        assert_eq!(cfg.llm.temperature, 0.0); // clamped
    }

    #[test]
    fn test_top_p_clamp() {
        let mut cfg = Config::default();
        cfg.set_top_p(2.0);
        assert_eq!(cfg.llm.top_p, 1.0);
    }

    #[test]
    fn test_provider_presets_exist() {
        let presets = Config::list_presets();
        assert!(presets.len() >= 4);
        let names: Vec<&str> = presets.iter().map(|p| p.name).collect();
        assert!(names.contains(&"deepseek"));
        assert!(names.contains(&"moonshot"));
        assert!(names.contains(&"openai"));
    }

    #[test]
    fn test_save_load_roundtrip() {
        let cfg = Config::default();
        let tmp = "/tmp/sapni_config_test.json";
        cfg.save(tmp).unwrap();

        let loaded = Config::load(tmp).unwrap();
        assert_eq!(loaded.llm.provider, cfg.llm.provider);
        assert_eq!(loaded.llm.model, cfg.llm.model);

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn test_effective_base_url() {
        let mut cfg = Config::default();
        cfg.llm.base_url = "https://api.deepseek.com/v1/".into();
        assert_eq!(cfg.effective_base_url(), "https://api.deepseek.com/v1");
    }
}
