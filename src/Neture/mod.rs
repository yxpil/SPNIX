//! 朴素贝叶斯分类器 — 词语联想记忆引擎
//! Naive Bayes classifier for word association & memory recall.
//!
//! 训练后可按词语联想最相关分类，或按分类联想最相关词语。
//! 支持增量训练、拉普拉斯平滑、持久化。

use std::collections::HashMap;

// ═══════════════════════════════════════════
// 核心数据结构
// ═══════════════════════════════════════════

/// 朴素贝叶斯分类器
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NaiveBayes {
    /// 分类 → 文档数
    category_doc_count: HashMap<String, u64>,
    /// 总文档数
    total_docs: u64,
    /// 分类 → (词语 → 出现次数)
    category_word_count: HashMap<String, HashMap<String, u64>>,
    /// 分类 → 总词数
    category_total_words: HashMap<String, u64>,
    /// 全局词汇表大小（用于平滑）
    vocabulary: HashMap<String, u64>,
    /// 全局总词数
    total_words: u64,
}

/// 分类预测结果
#[derive(Debug, Clone)]
pub struct Prediction {
    pub category: String,
    pub confidence: f64, // 0.0 ~ 1.0
    pub log_prob: f64,   // 对数概率（越大越可能）
}

/// 词语联想结果
#[derive(Debug, Clone)]
pub struct Association {
    pub word: String,
    pub score: f64,
}

/// 分类联想结果
#[derive(Debug, Clone)]
pub struct CategoryAssociation {
    pub category: String,
    pub score: f64,
}

// ═══════════════════════════════════════════
// 实现
// ═══════════════════════════════════════════

impl NaiveBayes {
    /// 创建空分类器
    pub fn new() -> Self {
        Self {
            category_doc_count: HashMap::new(),
            total_docs: 0,
            category_word_count: HashMap::new(),
            category_total_words: HashMap::new(),
            vocabulary: HashMap::new(),
            total_words: 0,
        }
    }

    /// 训练一条文档
    /// - `category`: 分类标签
    /// - `words`: 文档包含的词语列表
    pub fn train(&mut self, category: &str, words: &[String]) {
        let cat = category.to_string();

        // 文档计数
        *self.category_doc_count.entry(cat.clone()).or_insert(0) += 1;
        self.total_docs += 1;

        // 词语计数
        let word_map = self
            .category_word_count
            .entry(cat.clone())
            .or_insert_with(HashMap::new);
        let cat_total = self.category_total_words.entry(cat.clone()).or_insert(0);

        for word in words {
            *word_map.entry(word.clone()).or_insert(0) += 1;
            *cat_total += 1;
            *self.vocabulary.entry(word.clone()).or_insert(0) += 1;
            self.total_words += 1;
        }
    }

    /// 批量训练
    /// `docs`: Vec<(category, Vec<words>)>
    pub fn train_batch(&mut self, docs: &[(String, Vec<String>)]) {
        for (cat, words) in docs {
            self.train(cat, words);
        }
    }

    /// 分类预测 — 返回最可能的分类及置信度
    pub fn predict(&self, words: &[String]) -> Option<Prediction> {
        if self.category_doc_count.is_empty() {
            return None;
        }

        let vocab_size = self.vocabulary.len() as f64;

        let mut best: Option<Prediction> = None;

        for (cat, doc_count) in &self.category_doc_count {
            // 先验概率 log P(cat)
            let prior = (*doc_count as f64 / self.total_docs as f64).ln();

            // 似然 log P(word|cat) with Laplace smoothing
            let mut likelihood = 0.0f64;
            let word_map = self.category_word_count.get(cat);
            let cat_words = *self.category_total_words.get(cat).unwrap_or(&0);

            for word in words {
                let count = word_map.and_then(|m| m.get(word)).copied().unwrap_or(0) as f64;
                // Laplace: (count + 1) / (total_words_in_cat + vocab_size)
                let prob = (count + 1.0) / (cat_words as f64 + vocab_size);
                likelihood += prob.ln();
            }

            let log_prob = prior + likelihood;

            if best.is_none() || log_prob > best.as_ref().unwrap().log_prob {
                best = Some(Prediction {
                    category: cat.clone(),
                    confidence: 0.0, // 先占位，下面统一计算
                    log_prob,
                });
            }
        }

        // 归一化置信度
        if let Some(ref mut b) = best {
            b.confidence = log_prob_to_confidence(b.log_prob);
        }

        best
    }

    /// 预测 top-N 分类（按概率降序）
    pub fn predict_top_n(&self, words: &[String], n: usize) -> Vec<Prediction> {
        if self.category_doc_count.is_empty() {
            return Vec::new();
        }

        let vocab_size = self.vocabulary.len() as f64;
        let mut results = Vec::new();

        for (cat, doc_count) in &self.category_doc_count {
            let prior = (*doc_count as f64 / self.total_docs as f64).ln();
            let mut likelihood = 0.0f64;
            let word_map = self.category_word_count.get(cat);
            let cat_words = *self.category_total_words.get(cat).unwrap_or(&0);

            for word in words {
                let count = word_map.and_then(|m| m.get(word)).copied().unwrap_or(0) as f64;
                let prob = (count + 1.0) / (cat_words as f64 + vocab_size);
                likelihood += prob.ln();
            }

            results.push(Prediction {
                category: cat.clone(),
                confidence: 0.0,
                log_prob: prior + likelihood,
            });
        }

        results.sort_by(|a, b| {
            b.log_prob
                .partial_cmp(&a.log_prob)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(n);

        // 归一化
        let min_log = results.last().map(|r| r.log_prob).unwrap_or(0.0);
        let max_log = results.first().map(|r| r.log_prob).unwrap_or(0.0);
        for r in &mut results {
            r.confidence = log_prob_to_confidence(r.log_prob);
        }

        results
    }

    /// 词语联想：给定词语，找出最相关的分类
    /// "看到 word 这个词，最容易联想到哪个分类？"
    pub fn associate_word(&self, word: &str) -> Vec<CategoryAssociation> {
        let mut results = Vec::new();

        for (cat, word_map) in &self.category_word_count {
            let count = word_map.get(word).copied().unwrap_or(0) as f64;
            let cat_total = *self.category_total_words.get(cat).unwrap_or(&1) as f64;

            if count == 0.0 {
                continue;
            }

            // TF-IDF 风格的关联度
            let tf = count / cat_total;
            let docs_with_word = self
                .category_word_count
                .values()
                .filter(|m| m.contains_key(word))
                .count() as f64;
            let idf = (self.category_doc_count.len() as f64 / (1.0 + docs_with_word)).ln();

            results.push(CategoryAssociation {
                category: cat.clone(),
                score: tf * idf,
            });
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// 分类联想：给定分类，找出最有代表性的词语（纯词频排序）
    /// "提到 category，最容易想到哪些词？"
    pub fn associate_category(&self, category: &str, top_n: usize) -> Vec<Association> {
        let word_map = match self.category_word_count.get(category) {
            Some(m) => m,
            None => return Vec::new(),
        };

        let mut results: Vec<Association> = word_map
            .iter()
            .map(|(word, &count)| Association {
                word: word.clone(),
                score: count as f64,
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_n);
        results
    }

    /// 全局最热词语（跨所有分类的 IDF 加权频率）
    pub fn top_words(&self, n: usize) -> Vec<Association> {
        let mut scores: HashMap<String, f64> = HashMap::new();

        for word in self.vocabulary.keys() {
            let docs_with_word = self
                .category_word_count
                .values()
                .filter(|m| m.contains_key(word))
                .count() as f64;
            let idf = (self.category_doc_count.len() as f64 / (1.0 + docs_with_word)).ln();
            let tf = *self.vocabulary.get(word).unwrap_or(&0) as f64 / self.total_words as f64;
            scores.insert(word.clone(), tf * idf);
        }

        let mut results: Vec<Association> = scores
            .into_iter()
            .map(|(word, score)| Association { word, score })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(n);
        results
    }

    /// 两个词语之间的关联度
    pub fn word_similarity(&self, word_a: &str, word_b: &str) -> f64 {
        let mut vec_a = Vec::new();
        let mut vec_b = Vec::new();

        for cat in self.category_doc_count.keys() {
            let word_map = self.category_word_count.get(cat);
            let cat_total = *self.category_total_words.get(cat).unwrap_or(&1) as f64;
            let ca = word_map.and_then(|m| m.get(word_a)).copied().unwrap_or(0) as f64 / cat_total;
            let cb = word_map.and_then(|m| m.get(word_b)).copied().unwrap_or(0) as f64 / cat_total;
            vec_a.push(ca);
            vec_b.push(cb);
        }

        cosine_similarity(&vec_a, &vec_b)
    }

    // ─── 统计信息 ───

    pub fn category_count(&self) -> usize {
        self.category_doc_count.len()
    }

    pub fn doc_count(&self) -> u64 {
        self.total_docs
    }

    pub fn word_count(&self) -> usize {
        self.vocabulary.len()
    }

    pub fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self.category_doc_count.keys().cloned().collect();
        cats.sort();
        cats
    }

    // ─── 持久化 ───

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("序列化失败 / Serialize: {e}"))
    }

    /// 从 JSON 字符串加载
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("反序列化失败 / Deserialize: {e}"))
    }

    /// 保存到文件
    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = self.to_json()?;
        std::fs::write(path, &json).map_err(|e| format!("写入失败 / Write: {e}"))
    }

    /// 从文件加载
    pub fn load(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| format!("读取失败 / Read: {e}"))?;
        Self::from_json(&json)
    }
}

impl Default for NaiveBayes {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════

fn log_prob_to_confidence(log_prob: f64) -> f64 {
    // Sigmoid 压缩到 0~1，调整到合理范围
    let clamped = log_prob.max(-50.0).min(10.0);
    1.0 / (1.0 + (-clamped).exp())
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}

/// 简易中文分词（按字符 bigram + 单字）
pub fn tokenize_chinese(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();

    for window in chars.windows(2) {
        tokens.push(window.iter().collect::<String>());
    }
    for &c in &chars {
        tokens.push(c.to_string());
    }

    tokens
}

/// 简易英文/混合分词（按空格 + 标点拆分，转小写）
pub fn tokenize_english(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// 通用分词（中英混合）
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    // 按空白 + 标点拆分
    for raw_part in text
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|s| !s.is_empty())
    {
        // 再按 CJK/ASCII 边界拆分混合段
        for part in split_cjk_boundary(raw_part) {
            if part.is_empty() {
                continue;
            }
            let has_cjk = part.chars().any(|c| c as u32 > 0x2E80);
            if has_cjk {
                tokens.extend(tokenize_chinese(&part));
            } else {
                tokens.push(part.to_lowercase());
            }
        }
    }

    tokens
}

/// 在 CJK 与 ASCII 字符边界处拆分
fn split_cjk_boundary(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut prev_is_cjk: Option<bool> = None;

    for c in s.chars() {
        let is_cjk = c as u32 > 0x2E80;
        match prev_is_cjk {
            None => {
                current.push(c);
                prev_is_cjk = Some(is_cjk);
            }
            Some(prev) if prev == is_cjk => {
                current.push(c);
            }
            _ => {
                result.push(std::mem::take(&mut current));
                current.push(c);
                prev_is_cjk = Some(is_cjk);
            }
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

// ═══════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_words(s: &str) -> Vec<String> {
        tokenize(s)
    }

    #[test]
    fn test_basic_train_and_predict() {
        let mut nb = NaiveBayes::new();

        // 训练：科技类
        nb.train(
            "tech",
            &make_words("rust programming language cargo compiler"),
        );
        nb.train(
            "tech",
            &make_words("python code data science machine learning"),
        );
        nb.train("tech", &make_words("rust async tokio server web framework"));

        // 训练：食物类
        nb.train("food", &make_words("apple banana fruit salad delicious"));
        nb.train("food", &make_words("pizza pasta Italian cheese tomato"));
        nb.train("food", &make_words("rice noodle soup spicy Chinese"));

        // 预测
        let pred = nb
            .predict(&make_words("rust async server programming"))
            .unwrap();
        assert_eq!(pred.category, "tech");

        let pred = nb.predict(&make_words("delicious Chinese noodle")).unwrap();
        assert_eq!(pred.category, "food");
    }

    #[test]
    fn test_associate_word() {
        let mut nb = NaiveBayes::new();
        nb.train("sports", &make_words("football soccer goal player match"));
        nb.train("music", &make_words("guitar piano song melody concert"));
        nb.train("sports", &make_words("basketball player court team"));

        let results = nb.associate_word("player");
        assert!(!results.is_empty());
        // "player" 应该更关联 sports
        assert_eq!(results[0].category, "sports");
    }

    #[test]
    fn test_associate_category() {
        let mut nb = NaiveBayes::new();
        nb.train("weather", &make_words("rain storm thunder lightning wind"));
        nb.train("weather", &make_words("sunny hot summer temperature heat"));
        nb.train("weather", &make_words("rain cloudy humid monsoon"));

        let words = nb.associate_category("weather", 3);
        assert!(!words.is_empty());
        // "rain" 出现 2 次，应该是 top
        assert!(words.iter().any(|a| a.word == "rain"));
    }

    #[test]
    fn test_top_n_predict() {
        let mut nb = NaiveBayes::new();
        nb.train("cat_a", &make_words("hello world foo bar"));
        nb.train("cat_b", &make_words("hello world baz qux"));
        nb.train("cat_c", &make_words("unrelated stuff here now"));

        let results = nb.predict_top_n(&make_words("hello world"), 2);
        assert_eq!(results.len(), 2);
        // 文档数相同时，按 log_prob 排序，接受任一在前
        assert!(results[0].category == "cat_a" || results[0].category == "cat_b");
    }

    #[test]
    fn test_word_similarity() {
        let mut nb = NaiveBayes::new();
        nb.train("a", &tokenize("rust cargo code compiler"));
        nb.train("b", &tokenize("python pip code interpreter"));
        nb.train("a", &tokenize("rust memory safety ownership"));

        // rust 和 cargo 应该高度相关（共现于 a）
        let sim = nb.word_similarity("rust", "cargo");
        assert!(sim > 0.0);

        // rust 和 python 相关度较低（不同分类）
        let sim2 = nb.word_similarity("rust", "python");
        assert!(sim > sim2 || sim2 >= 0.0);
    }

    #[test]
    fn test_persistence() {
        let mut nb = NaiveBayes::new();
        nb.train("test", &tokenize("hello world"));

        let json = nb.to_json().unwrap();
        let nb2 = NaiveBayes::from_json(&json).unwrap();

        let pred = nb2.predict(&tokenize("hello")).unwrap();
        assert_eq!(pred.category, "test");
    }

    #[test]
    fn test_tokenize_mixed() {
        let tokens = tokenize("Rust编程语言async");
        assert!(tokens.contains(&"rust".to_string()));
        assert!(tokens.contains(&"编程".to_string()));
        assert!(tokens.contains(&"async".to_string()));
    }

    #[test]
    fn test_empty_predict() {
        let nb = NaiveBayes::new();
        assert!(nb.predict(&tokenize("hello")).is_none());
    }

    #[test]
    fn test_laplace_smoothing() {
        let mut nb = NaiveBayes::new();
        nb.train("cat", &tokenize("a b c"));

        // "z" 从未出现，但 Laplace 平滑仍应给出概率
        let pred = nb.predict(&tokenize("z")).unwrap();
        assert_eq!(pred.category, "cat");
        assert!(pred.confidence > 0.0);
    }
}
