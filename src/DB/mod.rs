//! 对话存储 + 关键点记忆 — 借助贝叶斯联想
//! Chat storage + key-point memory with Bayesian association.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ═══════════════════════════════════════════
// 对话存储 / Chat Storage
// ═══════════════════════════════════════════

/// 一条消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub timestamp: u64, // Unix 秒
    pub tokens: Option<u32>,
}

/// 一个会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created: u64,
    pub updated: u64,
    pub messages: Vec<Message>,
    pub tags: Vec<String>,
}

/// 对话数据库（JSON 文件持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDB {
    sessions: Vec<Session>,
    next_id: u64,
    /// 分类 → 文档列表（供贝叶斯训练）
    #[serde(skip)]
    category_docs: HashMap<String, Vec<String>>,
}

impl ChatDB {
    /// 创建或加载数据库
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            next_id: 1,
            category_docs: HashMap::new(),
        }
    }

    /// 从文件加载
    pub fn load(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| format!("读取失败 / Read: {e}"))?;
        let mut db: Self =
            serde_json::from_str(&json).map_err(|e| format!("解析失败 / Parse: {e}"))?;
        db.category_docs = HashMap::new();
        Ok(db)
    }

    /// 保存到文件
    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化失败 / Serialize: {e}"))?;
        std::fs::write(path, &json).map_err(|e| format!("写入失败 / Write: {e}"))?;
        Ok(())
    }

    // ─── 会话管理 ───

    /// 创建新会话
    pub fn create_session(&mut self, title: &str, tags: &[String]) -> String {
        let now = now_secs();
        let id = format!("s{}", self.next_id);
        self.next_id += 1;

        self.sessions.push(Session {
            id: id.clone(),
            title: title.to_string(),
            created: now,
            updated: now,
            messages: Vec::new(),
            tags: tags.to_vec(),
        });
        id
    }

    /// 添加消息到会话
    pub fn add_message(
        &mut self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<(), String> {
        let session = self.get_session_mut(session_id)?;
        session.messages.push(Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: now_secs(),
            tokens: None,
        });
        session.updated = now_secs();
        Ok(())
    }

    /// 获取会话（只读）
    pub fn get_session(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// 列出所有会话
    pub fn list_sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// 搜索会话（标题 + 标签关键词）
    pub fn search_sessions(&self, query: &str) -> Vec<&Session> {
        let q = query.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| {
                s.title.to_lowercase().contains(&q)
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// 删除会话
    pub fn delete_session(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .sessions
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| format!("会话不存在 / Session not found: {id}"))?;
        self.sessions.remove(idx);
        Ok(())
    }

    /// 获取会话最近 N 条消息
    pub fn recent_messages(&self, session_id: &str, n: usize) -> Result<Vec<&Message>, String> {
        let session = self
            .get_session(session_id)
            .ok_or_else(|| format!("会话不存在 / Session not found: {session_id}"))?;
        let start = session.messages.len().saturating_sub(n);
        Ok(session.messages[start..].iter().collect())
    }

    /// 全文搜索消息内容
    pub fn search_messages(&self, query: &str, limit: usize) -> Vec<MessageHit> {
        let q = query.to_lowercase();
        let mut hits = Vec::new();

        for session in &self.sessions {
            for msg in &session.messages {
                if msg.content.to_lowercase().contains(&q) {
                    hits.push(MessageHit {
                        session_id: session.id.clone(),
                        session_title: session.title.clone(),
                        role: msg.role.clone(),
                        content: msg.content.clone(),
                        timestamp: msg.timestamp,
                    });
                    if hits.len() >= limit {
                        break;
                    }
                }
            }
            if hits.len() >= limit {
                break;
            }
        }
        hits
    }

    /// 对所有会话内容做贝叶斯训练（按会话标签做分类）
    pub fn train_bayes(&self, bayes: &mut crate::Neture::NaiveBayes) {
        for session in &self.sessions {
            // 用标签作为分类，无标签则用 "general"
            let cats: Vec<String> = if session.tags.is_empty() {
                vec!["general".into()]
            } else {
                session.tags.clone()
            };

            // 合并所有消息内容并分词
            let all_text: String = session
                .messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            let words = crate::Neture::tokenize(&all_text);
            if words.is_empty() {
                continue;
            }

            for cat in &cats {
                bayes.train(cat, &words);
            }
        }
    }

    /// 导出为贝叶斯训练文档
    pub fn export_training_docs(&self) -> Vec<(String, Vec<String>)> {
        let mut docs = Vec::new();
        for session in &self.sessions {
            let cats: Vec<String> = if session.tags.is_empty() {
                vec!["general".into()]
            } else {
                session.tags.clone()
            };
            let all_text: String = session
                .messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let words = crate::Neture::tokenize(&all_text);
            if words.is_empty() {
                continue;
            }
            for cat in &cats {
                docs.push((cat.clone(), words.clone()));
            }
        }
        docs
    }

    fn get_session_mut(&mut self, id: &str) -> Result<&mut Session, String> {
        self.sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("会话不存在 / Session not found: {id}"))
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
    pub fn message_count(&self) -> usize {
        self.sessions.iter().map(|s| s.messages.len()).sum()
    }
}

#[derive(Debug, Clone)]
pub struct MessageHit {
    pub session_id: String,
    pub session_title: String,
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

// ═══════════════════════════════════════════
// 关键点记忆 / Key-Point Memory
// ═══════════════════════════════════════════

/// 一条关键记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,        // 关键点内容
    pub category: String,       // 分类（如 "preference", "fact", "todo"）
    pub source_session: String, // 来源会话 ID
    pub created: u64,
    pub importance: u8, // 1-10 重要度
    pub tags: Vec<String>,
}

/// 记忆数据库
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDB {
    entries: Vec<MemoryEntry>,
    next_id: u64,
}

impl MemoryDB {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
        }
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| format!("读取失败 / Read: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("解析失败 / Parse: {e}"))
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化失败 / Serialize: {e}"))?;
        std::fs::write(path, &json).map_err(|e| format!("写入失败 / Write: {e}"))?;
        Ok(())
    }

    /// 添加一条关键记忆
    pub fn remember(
        &mut self,
        content: &str,
        category: &str,
        source_session: &str,
        importance: u8,
        tags: &[String],
    ) -> String {
        let id = format!("m{}", self.next_id);
        self.next_id += 1;

        self.entries.push(MemoryEntry {
            id: id.clone(),
            content: content.to_string(),
            category: category.to_string(),
            source_session: source_session.to_string(),
            created: now_secs(),
            importance: importance.min(10).max(1),
            tags: tags.to_vec(),
        });
        id
    }

    /// 按分类列出记忆
    pub fn by_category(&self, category: &str) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.category == category)
            .collect()
    }

    /// 搜索记忆（内容关键词匹配）
    pub fn search(&self, query: &str, limit: usize) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        let mut results: Vec<&MemoryEntry> = self
            .entries
            .iter()
            .filter(|e| {
                e.content.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect();
        results.sort_by_key(|e| -(e.importance as i32));
        results.truncate(limit);
        results
    }

    /// 联想记忆：给定词语，通过贝叶斯找最相关记忆
    pub fn associate(
        &self,
        word: &str,
        bayes: &crate::Neture::NaiveBayes,
        limit: usize,
    ) -> Vec<&MemoryEntry> {
        let associations = bayes.associate_word(&word.to_lowercase());

        let mut scored: Vec<(&MemoryEntry, f64)> = self
            .entries
            .iter()
            .filter_map(|e| {
                let matching_cat = associations.iter().find(|a| a.category == e.category);
                matching_cat.map(|a| (e, a.score * e.importance as f64))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored.into_iter().map(|(e, _)| e).collect()
    }

    /// 训练贝叶斯（用所有记忆条目，按分类标签）
    pub fn train_bayes(&self, bayes: &mut crate::Neture::NaiveBayes) {
        for entry in &self.entries {
            let words = crate::Neture::tokenize(&entry.content);
            if !words.is_empty() {
                bayes.train(&entry.category, &words);
            }
        }
    }

    /// 更新记忆内容
    pub fn update(&mut self, id: &str, content: &str) -> Result<(), String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("记忆不存在 / Memory not found: {id}"))?;
        entry.content = content.to_string();
        Ok(())
    }

    /// 删除记忆
    pub fn forget(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| format!("记忆不存在 / Memory not found: {id}"))?;
        self.entries.remove(idx);
        Ok(())
    }

    /// 列出所有记忆
    pub fn list_all(&self) -> &[MemoryEntry] {
        &self.entries
    }

    /// 按重要度排序
    pub fn top_important(&self, n: usize) -> Vec<&MemoryEntry> {
        let mut entries: Vec<&MemoryEntry> = self.entries.iter().collect();
        entries.sort_by_key(|e| -(e.importance as i32));
        entries.truncate(n);
        entries
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

/// 联合数据库：对话 + 记忆 + 贝叶斯
pub struct SapNiDB {
    pub chat: ChatDB,
    pub memory: MemoryDB,
    pub bayes: crate::Neture::NaiveBayes,
    dir: PathBuf,
}

impl SapNiDB {
    /// 创建或加载数据库
    pub fn open(dir: &str) -> Result<Self, String> {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败 / Mkdir: {e}"))?;

        let chat_path = dir.join("chat.json");
        let mem_path = dir.join("memory.json");
        let bayes_path = dir.join("bayes.json");

        let chat = if chat_path.exists() {
            ChatDB::load(chat_path.to_str().unwrap())?
        } else {
            ChatDB::new()
        };

        let memory = if mem_path.exists() {
            MemoryDB::load(mem_path.to_str().unwrap())?
        } else {
            MemoryDB::new()
        };

        let mut bayes = if bayes_path.exists() {
            crate::Neture::NaiveBayes::load(bayes_path.to_str().unwrap())?
        } else {
            crate::Neture::NaiveBayes::new()
        };

        // 首次加载时重新训练贝叶斯
        if bayes.category_count() == 0 && memory.count() > 0 {
            memory.train_bayes(&mut bayes);
            chat.train_bayes(&mut bayes);
        }

        Ok(Self {
            chat,
            memory,
            bayes,
            dir,
        })
    }

    /// 持久化所有数据
    pub fn flush(&self) -> Result<(), String> {
        self.chat
            .save(self.dir.join("chat.json").to_str().unwrap())?;
        self.memory
            .save(self.dir.join("memory.json").to_str().unwrap())?;
        self.bayes
            .save(self.dir.join("bayes.json").to_str().unwrap())?;
        Ok(())
    }

    /// 从对话中自动提取关键记忆（基于重要度规则）
    pub fn auto_remember(&mut self, session_id: &str, content: &str, role: &str) -> Vec<String> {
        let mut ids = Vec::new();

        // 规则 1：用户明确说"记住"/"别忘了"/"重要"
        if role == "user" {
            let lower = content.to_lowercase();
            let importance = if lower.contains("重要") || lower.contains("important") {
                9
            } else if lower.contains("记住")
                || lower.contains("别忘了")
                || lower.contains("remember")
            {
                8
            } else {
                return ids; // 不自动记忆
            };

            let id = self
                .memory
                .remember(content, "auto", session_id, importance, &[]);

            // 增量训练贝叶斯
            let words = crate::Neture::tokenize(content);
            self.bayes.train("auto", &words);

            ids.push(id);
        }

        ids
    }

    /// 联想查询：用自然语言找到最相关的记忆
    pub fn recall(&self, query: &str, limit: usize) -> Vec<&MemoryEntry> {
        let words = crate::Neture::tokenize(query);
        if let Some(pred) = self.bayes.predict(&words) {
            // 先按分类找
            let mut results = self.memory.by_category(&pred.category);
            results.sort_by_key(|e| -(e.importance as i32));
            results.truncate(limit);
            if !results.is_empty() {
                return results;
            }
        }

        // 贝叶斯无结果，关键词回退
        self.memory.search(query, limit)
    }
}

// ═══════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ═══════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> String {
        format!("/tmp/sapni_db_test_{}", std::process::id())
    }

    fn cleanup(dir: &str) {
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_chat_crud() {
        let mut db = ChatDB::new();
        let sid = db.create_session("测试会话", &["test".into()]);
        db.add_message(&sid, "user", "你好").unwrap();
        db.add_message(&sid, "assistant", "你好！有什么可以帮你？")
            .unwrap();

        assert_eq!(db.session_count(), 1);
        assert_eq!(db.message_count(), 2);

        let msgs = db.recent_messages(&sid, 1).unwrap();
        assert_eq!(msgs[0].content, "你好！有什么可以帮你？");
    }

    #[test]
    fn test_chat_search() {
        let mut db = ChatDB::new();
        let sid = db.create_session("Rust学习", &["rust".into()]);
        db.add_message(&sid, "user", "什么是所有权？").unwrap();

        let hits = db.search_messages("所有权", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_title, "Rust学习");
    }

    #[test]
    fn test_memory_remember_and_search() {
        let mut mem = MemoryDB::new();
        mem.remember(
            "用户喜欢 Rust 语言",
            "preference",
            "s1",
            8,
            &["rust".into()],
        );
        mem.remember("项目使用 PostgreSQL", "fact", "s2", 6, &["db".into()]);

        let results = mem.search("Rust", 10);
        assert_eq!(results.len(), 1);

        let results = mem.search("PostgreSQL", 10);
        assert_eq!(results.len(), 1);

        // 不匹配
        assert!(mem.search("MongoDB", 10).is_empty());
    }

    #[test]
    fn test_memory_bayes_association() {
        let mut mem = MemoryDB::new();
        mem.remember("Rust memory safety ownership system", "tech", "s1", 8, &[]);
        mem.remember("Python data science machine learning", "tech", "s2", 7, &[]);
        mem.remember("pizza dinner tonight", "life", "s3", 3, &[]);
        mem.remember("mountain hiking weekend", "life", "s4", 4, &[]);

        // 训练贝叶斯
        let mut bayes = crate::Neture::NaiveBayes::new();
        mem.train_bayes(&mut bayes);

        // 联想："Rust" → tech 分类
        let results = mem.associate("Rust", &bayes, 5);
        assert!(!results.is_empty(), "Rust should associate to tech");

        // 联想："pizza" → life 分类
        let results = mem.associate("pizza", &bayes, 5);
        assert!(!results.is_empty(), "pizza should associate to life");
    }

    #[test]
    fn test_sapni_db_open_and_flush() {
        let dir = temp_dir();
        let mut db = SapNiDB::open(&dir).unwrap();

        let sid = db.chat.create_session("测试", &[]);
        db.chat.add_message(&sid, "user", "测试消息").unwrap();
        db.memory.remember("测试记忆", "test", &sid, 5, &[]);

        // 训练
        let words = crate::Neture::tokenize("测试消息");
        db.bayes.train("test", &words);

        db.flush().unwrap();

        // 重新打开
        let db2 = SapNiDB::open(&dir).unwrap();
        assert_eq!(db2.chat.session_count(), 1);
        assert_eq!(db2.memory.count(), 1);

        cleanup(&dir);
    }

    #[test]
    fn test_auto_remember() {
        let dir = temp_dir();
        let mut db = SapNiDB::open(&dir).unwrap();
        let sid = db.chat.create_session("test", &[]);

        // "记住"触发自动记忆
        db.chat
            .add_message(&sid, "user", "这是一条普通消息")
            .unwrap();
        let ids = db.auto_remember(&sid, "这是一条普通消息", "user");
        assert!(ids.is_empty()); // 不含关键词，不自动记

        let ids = db.auto_remember(&sid, "记住：用户喜欢 Rust", "user");
        assert!(!ids.is_empty()); // 含"记住"，自动记

        cleanup(&dir);
    }
}
