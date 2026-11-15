//! Deterministic tokenizer for the BM25 index.
//!
//! Latin words are case-folded and split on punctuation; CJK runs are indexed
//! as character bigrams (plus a unigram for a single-character run). This keeps
//! keyword and proper-noun recall usable for both English and Chinese without a
//! tokenizer dependency.

/// Common English function words. They carry no retrieval signal.
const ENGLISH_STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "can", "do", "does", "for",
    "from", "had", "has", "have", "he", "her", "his", "how", "i", "if", "in", "into", "is", "it",
    "its", "just", "me", "my", "no", "not", "of", "on", "or", "our", "she", "should", "so", "than",
    "that", "the", "their", "them", "then", "there", "these", "they", "this", "those", "to", "was",
    "we", "were", "what", "when", "where", "which", "who", "why", "will", "with", "would", "you",
    "your",
];

/// CJK function characters. A bigram made of two of these is dropped; a bigram
/// containing one is kept ("是否", "用户" stay meaningful).
const CJK_STOPCHARS: &[char] = &[
    '的', '了', '是', '在', '我', '你', '他', '她', '它', '们', '这', '那', '有', '和', '与', '及',
    '或', '请', '吗', '呢', '吧', '啊', '呀', '着', '过', '也', '都', '就', '而', '被', '把', '给',
    '让', '从', '到', '对', '为', '以', '于', '之', '其', '中', '上', '下', '个', '些',
];

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3040..=0x30FF     // kana
        | 0x3400..=0x4DBF   // CJK extension A
        | 0x4E00..=0x9FFF   // CJK unified ideographs
        | 0xAC00..=0xD7AF   // hangul syllables
        | 0xF900..=0xFAFF   // compatibility ideographs
        | 0x20000..=0x2FA1F // CJK extensions B+
    )
}

fn is_cjk_stopword(ch: char) -> bool {
    CJK_STOPCHARS.contains(&ch)
}

fn flush_latin(buffer: &mut String, out: &mut Vec<String>) {
    if buffer.is_empty() {
        return;
    }
    if !ENGLISH_STOPWORDS.contains(&buffer.as_str()) {
        out.push(buffer.clone());
    }
    buffer.clear();
}

fn flush_cjk(run: &mut Vec<char>, out: &mut Vec<String>) {
    match run.len() {
        0 => {}
        1 => {
            if !is_cjk_stopword(run[0]) {
                out.push(run[0].to_string());
            }
        }
        _ => {
            for pair in run.windows(2) {
                if is_cjk_stopword(pair[0]) && is_cjk_stopword(pair[1]) {
                    continue;
                }
                out.push(pair.iter().collect());
            }
        }
    }
    run.clear();
}

/// Tokenize text into index terms. The same function is used for facts and
/// queries, so a term always matches itself.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();
    for ch in text.chars() {
        if is_cjk(ch) {
            flush_latin(&mut latin, &mut tokens);
            cjk.push(ch);
        } else if ch.is_alphanumeric() || ch == '_' {
            flush_cjk(&mut cjk, &mut tokens);
            for lower in ch.to_lowercase() {
                latin.push(lower);
            }
        } else {
            flush_latin(&mut latin, &mut tokens);
            flush_cjk(&mut cjk, &mut tokens);
        }
    }
    flush_latin(&mut latin, &mut tokens);
    flush_cjk(&mut cjk, &mut tokens);
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_english_and_drops_stopwords() {
        assert_eq!(
            tokenize("The user prefers dark mode."),
            vec!["user", "prefers", "dark", "mode"]
        );
    }

    #[test]
    fn keeps_identifiers_together() {
        assert_eq!(
            tokenize("set max_steps to 20"),
            vec!["set", "max_steps", "20"]
        );
        assert_eq!(tokenize("MAIN.RS"), vec!["main", "rs"]);
    }

    #[test]
    fn indexes_chinese_as_bigrams() {
        assert_eq!(
            tokenize("用户偏好深色主题"),
            vec!["用户", "户偏", "偏好", "好深", "深色", "色主", "主题"]
        );
        assert_eq!(tokenize("深色"), vec!["深色"]);
        assert_eq!(tokenize("是"), Vec::<String>::new());
    }

    #[test]
    fn handles_mixed_scripts() {
        assert_eq!(
            tokenize("使用 pnpm 管理依赖"),
            vec!["使用", "pnpm", "管理", "理依", "依赖"]
        );
    }
}
