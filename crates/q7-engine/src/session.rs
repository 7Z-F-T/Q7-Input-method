//! 输入会话状态机：驱动组字、候选、上屏。
//!
//! 纯逻辑、无平台依赖。服务端为宿主的每个文本服务实例维护一个 `Session`。

use q7_core::candidate::Candidate;
use q7_core::keys::{KeyEvent, KeyState};
use q7_core::protocol::{CompositionState, KeyResult};

use crate::dict::Dict;

/// 一个输入会话（对应宿主一个文本服务实例）
#[derive(Default)]
pub struct Session {
    /// 当前输入串（小写英文字母）
    input: String,
    candidates: Vec<Candidate>,
    selected: usize,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// 处理一次按键，返回给 DLL 的执行结果
    pub fn handle_key(&mut self, ev: &KeyEvent) -> KeyResult {
        if ev.state != KeyState::Down {
            return KeyResult::ignored();
        }
        // Ctrl/Alt 组合键：取消组字后透传
        if ev.modifiers.ctrl() || ev.modifiers.alt() {
            return self.cancel_if_composing();
        }
        match ev.vk {
            0x41..=0x5A => self.push_letter(ev.vk as u8),
            0x08 => self.backspace(),
            0x1B => self.escape(),
            0x20 => self.space(),
            0x31..=0x39 => self.select_digit(ev.vk),
            0x0D => self.enter(),
            _ => self.cancel_if_composing(),
        }
    }

    /// 英文小写字母（忽略 Shift/CapsLock，恒小写拼音）
    fn push_letter(&mut self, vk: u8) -> KeyResult {
        self.input.push((vk - 0x41 + b'a') as char);
        self.refresh_candidates();
        self.composing_result()
    }

    fn backspace(&mut self) -> KeyResult {
        if self.input.pop().is_none() {
            return KeyResult::ignored();
        }
        self.refresh_candidates();
        self.composing_result()
    }

    fn escape(&mut self) -> KeyResult {
        if self.input.is_empty() {
            return KeyResult::ignored();
        }
        self.reset();
        KeyResult {
            handled: true,
            composition: None,
            commit: None,
            candidates: Vec::new(),
        }
    }

    fn space(&mut self) -> KeyResult {
        match self.candidates.first().cloned() {
            Some(first) => self.commit(first.text),
            None => self.cancel_if_composing(),
        }
    }

    fn select_digit(&mut self, vk: u16) -> KeyResult {
        let idx = (vk - 0x31) as usize;
        match self.candidates.get(idx).cloned() {
            Some(c) => self.commit(c.text),
            None => self.cancel_if_composing(),
        }
    }

    fn enter(&mut self) -> KeyResult {
        if self.input.is_empty() {
            return KeyResult::ignored();
        }
        // 有候选则上屏选中候选，否则上屏原始字母
        let text = self
            .candidates
            .get(self.selected)
            .map(|c| c.text.clone())
            .unwrap_or_else(|| self.input.clone());
        self.commit(text)
    }

    /// 组字中遇到未处理按键：结束组字（文档中的组字文本由 DLL 删除）并透传
    fn cancel_if_composing(&mut self) -> KeyResult {
        if self.input.is_empty() {
            return KeyResult::ignored();
        }
        self.reset();
        KeyResult::ignored()
    }

    fn commit(&mut self, text: String) -> KeyResult {
        self.reset();
        KeyResult {
            handled: true,
            composition: None,
            commit: Some(text),
            candidates: Vec::new(),
        }
    }

    /// 组字中：composition 携带当前输入串；输入被删空时通知 DLL 结束组字
    fn composing_result(&self) -> KeyResult {
        let composition = if self.input.is_empty() {
            None
        } else {
            Some(CompositionState {
                text: self.input.clone(),
                cursor: self.input.chars().count() as u32,
            })
        };
        KeyResult {
            handled: true,
            composition,
            commit: None,
            candidates: self.candidates.clone(),
        }
    }

    fn refresh_candidates(&mut self) {
        self.candidates = Dict::builtin().lookup(&self.input);
        self.selected = 0;
    }

    fn reset(&mut self) {
        self.input.clear();
        self.candidates.clear();
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use q7_core::keys::Modifiers;

    fn key(vk: u16) -> KeyEvent {
        KeyEvent {
            vk,
            scan_code: 0,
            modifiers: Modifiers::default(),
            state: KeyState::Down,
        }
    }

    /// 依次输入一串小写字母，返回最后一次结果
    fn type_str(s: &mut Session, text: &str) -> KeyResult {
        let mut last = KeyResult::ignored();
        for ch in text.chars() {
            let vk = 0x41 + (ch as u16 - 'a' as u16);
            last = s.handle_key(&key(vk));
        }
        last
    }

    #[test]
    fn typing_pinyin_shows_composition_and_candidates() {
        let mut s = Session::new();
        let r = type_str(&mut s, "nihao");
        assert!(r.handled);
        assert_eq!(r.composition.as_ref().unwrap().text, "nihao");
        assert_eq!(r.candidates[0].text, "你好");
    }

    #[test]
    fn space_commits_first_candidate() {
        let mut s = Session::new();
        type_str(&mut s, "nihao");
        let r = s.handle_key(&key(0x20));
        assert!(r.handled);
        assert_eq!(r.commit.as_deref(), Some("你好"));
        assert!(r.composition.is_none());
    }

    #[test]
    fn digit_selects_nth_candidate() {
        let mut s = Session::new();
        type_str(&mut s, "shi");
        let r = s.handle_key(&key(0x32)); // 数字 2 → 第二个候选"时"
        assert!(r.handled);
        assert_eq!(r.commit.as_deref(), Some("时"));
    }

    #[test]
    fn escape_cancels_without_commit() {
        let mut s = Session::new();
        type_str(&mut s, "ni");
        let r = s.handle_key(&key(0x1B));
        assert!(r.handled);
        assert!(r.commit.is_none());
        assert!(r.composition.is_none());
    }

    #[test]
    fn backspace_updates_composition() {
        let mut s = Session::new();
        type_str(&mut s, "nihao");
        let r = s.handle_key(&key(0x08));
        assert_eq!(r.composition.as_ref().unwrap().text, "niha");
    }

    #[test]
    fn unhandled_key_cancels_composition() {
        let mut s = Session::new();
        type_str(&mut s, "ni");
        let r = s.handle_key(&key(0x2C)); // 逗号
        assert!(!r.handled);
        assert!(r.composition.is_none());
    }

    #[test]
    fn enter_commits_raw_input_without_candidates() {
        let mut s = Session::new();
        type_str(&mut s, "zz");
        let r = s.handle_key(&key(0x0D));
        assert!(r.handled);
        assert_eq!(r.commit.as_deref(), Some("zz"));
    }

    #[test]
    fn idle_letters_are_handled_idle_punct_passthrough() {
        let mut s = Session::new();
        let r = s.handle_key(&key(0x2C)); // 空闲时逗号透传
        assert!(!r.handled);
        let r = type_str(&mut s, "wo");
        assert!(r.handled);
        assert_eq!(r.candidates[0].text, "我");
    }
}
