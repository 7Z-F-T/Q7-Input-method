//! 文本服务：TSF 生命周期入口 + 按键接管。
//!
//! 每个宿主线程一个实例。按键经 IPC 交 q7-server 的引擎处理；
//! 处理结果（组字/候选/上屏）由 [`crate::edit`] 落地到宿主文档。
//! 所有 COM 入口经 panic_guard 保护。

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use q7_core::keys::{KeyEvent, KeyState, Modifiers};
use q7_core::protocol::KeyResult;
use windows::Win32::Foundation::{E_FAIL, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::TextServices::{
    ITfContext, ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfTextInputProcessor,
    ITfTextInputProcessor_Impl, ITfTextInputProcessorEx, ITfTextInputProcessorEx_Impl,
    ITfThreadMgr,
};
use windows::core::{BOOL, IUnknownImpl, Interface, Ref, Result, implement};

use crate::edit::{self, CompositionManager};
use crate::ipc_client;
use crate::panic_guard::catch;

/// 全局会话 ID 计数器（一个文本服务实例 = 一个会话）
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// 线程管理器槽位。
///
/// SAFETY：TSF 文本服务的所有回调都发生在同一公寓线程（tid 于激活时取得），
/// 经 Mutex 串行访问；COM 引用计数线程安全。
struct ThreadMgrSlot(Mutex<Option<ITfThreadMgr>>);

unsafe impl Send for ThreadMgrSlot {}
unsafe impl Sync for ThreadMgrSlot {}

/// Q7 文本服务实例
#[implement(ITfTextInputProcessor, ITfTextInputProcessorEx, ITfKeyEventSink)]
pub struct TextService {
    session: u64,
    thread_mgr: ThreadMgrSlot,
    tid: Mutex<u32>,
    composition: Mutex<CompositionManager>,
}

impl TextService {
    pub fn create() -> Self {
        Self {
            session: SESSION_COUNTER.fetch_add(1, Ordering::Relaxed),
            thread_mgr: ThreadMgrSlot(Mutex::new(None)),
            tid: Mutex::new(0),
            composition: Mutex::new(CompositionManager::default()),
        }
    }

    /// 激活：挂接按键事件汇（sink 由 `_Impl` 层取得后传入，见 `Activate`/`ActivateEx`）
    fn activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32, sink: ITfKeyEventSink) -> Result<()> {
        catch(Err(E_FAIL.into()), || {
            let tm: ITfThreadMgr = ptim.ok()?.clone();
            let km: ITfKeystrokeMgr = tm.cast()?;
            unsafe { km.AdviseKeyEventSink(tid, &sink, true) }?;

            *self.thread_mgr.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(tm);
            *self.tid.lock().unwrap_or_else(|e| e.into_inner()) = tid;
            Ok(())
        })
    }

    /// 失活：卸载按键事件汇
    fn deactivate(&self) -> Result<()> {
        catch(Ok(()), || {
            let tid = *self.tid.lock().unwrap_or_else(|e| e.into_inner());
            let tm = self
                .thread_mgr
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(tm) = tm
                && let Ok(km) = tm.cast::<ITfKeystrokeMgr>()
            {
                unsafe { km.UnadviseKeyEventSink(tid) }.ok();
            }
            self.composition
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reset();
            ipc_client::end_session(self.session);
            Ok(())
        })
    }

    /// OnTestKeyDown 的本地判断（不发 IPC，保持零延迟）
    fn should_swallow(&self, vk: u16) -> bool {
        let composing = self
            .composition
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_active();
        match vk {
            0x41..=0x5A => true,                    // 字母：可能进入拼音组字
            0x08 | 0x1B | 0x20 | 0x0D => composing, // 退格/取消/空格/回车：组字中才吞
            0x31..=0x39 => composing,               // 数字选词
            _ => false,
        }
    }
}

/// 从按键消息取修饰键状态（在按键回调内调用）
fn current_modifiers() -> Modifiers {
    let mut m = Modifiers::default();
    unsafe {
        if GetKeyState(0x10) < 0 {
            m = m.with(Modifiers::SHIFT);
        }
        if GetKeyState(0x11) < 0 {
            m = m.with(Modifiers::CTRL);
        }
        if GetKeyState(0x12) < 0 {
            m = m.with(Modifiers::ALT);
        }
        if GetKeyState(0x14) & 1 != 0 {
            m = m.with(Modifiers::CAPS_LOCK);
        }
    }
    m
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        let sink: ITfKeyEventSink = IUnknownImpl::to_interface::<ITfKeyEventSink>(self);
        self.activate(ptim, tid, sink)
    }

    fn Deactivate(&self) -> Result<()> {
        self.deactivate()
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32, _dwflags: u32) -> Result<()> {
        let sink: ITfKeyEventSink = IUnknownImpl::to_interface::<ITfKeyEventSink>(self);
        self.activate(ptim, tid, sink)
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> {
        // PoC：不处理焦点变化（阶段 1：焦点丢失时结束组字）
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        _pic: Ref<'_, ITfContext>,
        wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        catch(Ok(BOOL::from(false)), || {
            Ok(BOOL::from(self.should_swallow(wparam.0 as u16)))
        })
    }

    fn OnKeyDown(&self, pic: Ref<'_, ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        catch(Ok(BOOL::from(false)), || {
            let Some(ctx) = pic.as_ref() else {
                return Ok(BOOL::from(false));
            };
            let event = KeyEvent {
                vk: wparam.0 as u16,
                scan_code: ((lparam.0 >> 16) & 0xFF) as u16,
                modifiers: current_modifiers(),
                state: KeyState::Down,
            };

            let tid = *self.tid.lock().unwrap_or_else(|e| e.into_inner());
            match ipc_client::send_key(self.session, event) {
                Some(result) => {
                    edit::apply_key_result(ctx, tid, &self.composition, &result);
                    Ok(BOOL::from(result.handled))
                }
                None => {
                    // 服务端不可用：结束可能存在的组字并透传按键（绝不阻塞输入）
                    edit::apply_key_result(ctx, tid, &self.composition, &KeyResult::ignored());
                    Ok(BOOL::from(false))
                }
            }
        })
    }

    fn OnTestKeyUp(
        &self,
        _pic: Ref<'_, ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(BOOL::from(false))
    }

    fn OnKeyUp(&self, _pic: Ref<'_, ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(BOOL::from(false))
    }

    fn OnPreservedKey(
        &self,
        _pic: Ref<'_, ITfContext>,
        _rguid: *const windows::core::GUID,
    ) -> Result<BOOL> {
        Ok(BOOL::from(false))
    }
}
