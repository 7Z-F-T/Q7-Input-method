//! PoC 阶段的静态小词库。
//!
//! 阶段 1 将替换为 `data/dict-src` 编译出的 `.q7dict`（mmap + FST）词典，
//! 对外接口（`lookup`）保持不变，调用方无需改动。
//!
//! PoC 简化：不做音节切分，整串精确匹配（整词）+ 单音节回退（单字）。

use q7_core::candidate::{Candidate, CandidateSource};

/// 词库：整词表 + 单字表，均按完整拼音索引
pub struct Dict {
    words: &'static [(&'static str, &'static [&'static str])],
    chars: &'static [(&'static str, &'static str)],
}

impl Dict {
    /// 内置 PoC 词库
    pub fn builtin() -> &'static Dict {
        static DICT: Dict = Dict {
            words: WORDS,
            chars: CHARS,
        };
        &DICT
    }

    /// 按输入串查询候选（最多 9 个）
    pub fn lookup(&self, input: &str) -> Vec<Candidate> {
        let mut out = Vec::new();
        if input.is_empty() {
            return out;
        }

        if let Some((_, words)) = self.words.iter().find(|(k, _)| *k == input) {
            for w in *words {
                out.push(Candidate {
                    text: (*w).to_string(),
                    comment: Some(input.to_string()),
                    source: CandidateSource::Dict,
                });
            }
        }
        if let Some((_, chars)) = self.chars.iter().find(|(k, _)| *k == input) {
            for c in chars.chars() {
                out.push(Candidate {
                    text: c.to_string(),
                    comment: None,
                    source: CandidateSource::Dict,
                });
            }
        }
        out.truncate(9);
        out
    }
}

/// 整词表
static WORDS: &[(&str, &[&str])] = &[
    ("nihao", &["你好"]),
    ("shijie", &["世界"]),
    ("shurufa", &["输入法"]),
    ("shuru", &["输入"]),
    ("ceshi", &["测试"]),
    ("zhongguo", &["中国"]),
    ("diannao", &["电脑"]),
    ("pengyou", &["朋友"]),
    ("xiexie", &["谢谢"]),
    ("zaijian", &["再见"]),
    ("keyi", &["可以"]),
    ("shijian", &["时间"]),
    ("gongzuo", &["工作"]),
    ("xuexi", &["学习"]),
    ("shanghai", &["上海"]),
    ("beijing", &["北京"]),
    ("women", &["我们"]),
    ("shenme", &["什么"]),
    ("weishenme", &["为什么"]),
    ("zenme", &["怎么"]),
    ("meiyou", &["没有"]),
    ("zhidao", &["知道"]),
    ("xihuan", &["喜欢"]),
    ("danshi", &["但是"]),
    ("suoyi", &["所以"]),
    ("yinwei", &["因为"]),
    ("kuaile", &["快乐"]),
];

/// 单音节单字表
static CHARS: &[(&str, &str)] = &[
    ("ni", "你尼"),
    ("wo", "我"),
    ("hao", "好号"),
    ("shi", "是时十"),
    ("zai", "在再"),
    ("you", "有又"),
    ("bu", "不部"),
    ("le", "了"),
    ("ren", "人"),
    ("ri", "日"),
    ("yue", "月"),
    ("tian", "天"),
    ("de", "的地得"),
    ("he", "和合"),
    ("shang", "上"),
    ("xia", "下"),
    ("da", "大"),
    ("xiao", "小"),
    ("zhong", "中"),
    ("guo", "国"),
    ("zi", "字自"),
    ("ci", "词此"),
    ("fa", "发法"),
    ("shuo", "说"),
    ("ming", "明名"),
    ("nian", "年"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_word() {
        let c = Dict::builtin().lookup("nihao");
        assert_eq!(c[0].text, "你好");
    }

    #[test]
    fn lookup_single_syllable_chars() {
        let c = Dict::builtin().lookup("shi");
        let texts: Vec<&str> = c.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["是", "时", "十"]);
    }

    #[test]
    fn lookup_unknown_is_empty() {
        assert!(Dict::builtin().lookup("zzzz").is_empty());
    }
}
