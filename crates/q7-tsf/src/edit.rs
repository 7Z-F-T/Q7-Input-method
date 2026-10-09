//! 编辑会话与组字管理：把引擎的 `KeyResult` 落地到宿主文档。
//!
//! TSF 约束：对 `ITfContext` 的一切写入必须在 `ITfEditSession::DoEditSession` 内执行。
//! 这里使用**同步编辑会话**（TF_ES_SYNC），请求方即 TSF 回调所在公寓线程。

use std::sync::{Arc, Mutex};

use q7_core::protocol::KeyResult;
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::UI::TextServices::{
    INSERT_TEXT_AT_SELECTION_FLAGS, ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl,
    ITfContext, ITfContextComposition, ITfEditSession, ITfEditSession_Impl, ITfInsertAtSelection,
    ITfRange, TF_CONTEXT_EDIT_CONTEXT_FLAGS, TF_ES_READWRITE, TF_ES_SYNC,
};
use windows::core::{Interface, Ref, Result, implement};

use crate::panic_guard::catch;

/// 活动组字的句柄集合
#[derive(Clone)]
struct ActiveComposition {
    composition: ITfComposition,
    range: ITfRange,
    /// 当前组字串（阶段 1：用于增量更新/回退优化，PoC 仅保留）
    #[allow(dead_code)]
    text: String,
}

/// 组字管理器（DLL 侧每个文本服务实例一份）
///
/// SAFETY（Send）：本类型仅由持有它的 `TextService` 在**同一个 TSF 公寓线程**中访问，
/// 外部经 `Mutex` 串行化；COM 接口引用计数本身线程安全。
#[derive(Default)]
pub struct CompositionManager {
    active: Option<ActiveComposition>,
}

unsafe impl Send for CompositionManager {}

impl CompositionManager {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn reset(&mut self) {
        self.active = None;
    }
}

/// 一次编辑会话要执行的操作
enum EditOp {
    /// 组字：开始或更新组字串
    Compose {
        text: String,
        existing: Option<ActiveComposition>,
    },
    /// 上屏：有组字则在组字范围内替换为最终文本并结束组字，否则在光标处插入
    Commit {
        text: String,
        existing: Option<ActiveComposition>,
    },
    /// 取消组字：删除已组字文本
    Cancel { existing: ActiveComposition },
}

/// 编辑会话执行结果（经 Arc 回传给请求方）
type Slot = Arc<Mutex<Option<ActiveComposition>>>;

/// 同步编辑会话
#[implement(ITfEditSession)]
struct Q7EditSession {
    ctx: ITfContext,
    op: EditOp,
    slot: Slot,
}

// SAFETY：编辑会话在一次 `RequestEditSession` 调用内同步执行完毕（TF_ES_SYNC），
// 不跨线程传递。
unsafe impl Send for Q7EditSession {}
unsafe impl Sync for Q7EditSession {}

impl ITfEditSession_Impl for Q7EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        catch(Err(E_FAIL.into()), || match &self.op {
            EditOp::Compose { text, existing } => {
                let active = match existing {
                    Some(active) => {
                        set_range_text(&active.range, ec, text)?;
                        ActiveComposition {
                            composition: active.composition.clone(),
                            range: active.range.clone(),
                            text: text.clone(),
                        }
                    }
                    None => start_composition(&self.ctx, ec, text)?,
                };
                *self.slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(active);
                Ok(())
            }
            EditOp::Commit { text, existing } => {
                match existing {
                    Some(active) => {
                        set_range_text(&active.range, ec, text)?;
                        unsafe { active.composition.EndComposition(ec)? };
                    }
                    None => insert_text(&self.ctx, ec, text)?,
                }
                Ok(())
            }
            EditOp::Cancel { existing } => {
                set_range_text(&existing.range, ec, "")?;
                unsafe { existing.composition.EndComposition(ec)? };
                Ok(())
            }
        })
    }
}

/// 应用一次按键结果到宿主文档（同步执行，调用方必须是 TSF 公寓线程）
pub fn apply_key_result(
    ctx: &ITfContext,
    tid: u32,
    manager: &Mutex<CompositionManager>,
    result: &KeyResult,
) {
    // 取走当前组字状态（Commit/Cancel 之后不再持有；Compose 成功会写回新状态）
    let existing = manager
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .active
        .take();

    let (op, keep_slot) = if let Some(text) = &result.commit {
        (
            EditOp::Commit {
                text: text.clone(),
                existing,
            },
            false,
        )
    } else {
        match (&result.composition, existing) {
            (Some(comp), existing) => (
                EditOp::Compose {
                    text: comp.text.clone(),
                    existing,
                },
                true,
            ),
            (None, Some(existing)) => (EditOp::Cancel { existing }, false),
            // 无组字、无需处理
            (None, None) => return,
        }
    };

    // Arc 内含 COM 接口（非 Send/Sync），但只在当前线程内传递使用
    #[allow(clippy::arc_with_non_send_sync)]
    let slot: Slot = Arc::new(Mutex::new(None));
    let session = Q7EditSession {
        ctx: ctx.clone(),
        op,
        slot: slot.clone(),
    };
    let itf_session: ITfEditSession = session.into();
    let flags = TF_CONTEXT_EDIT_CONTEXT_FLAGS(TF_ES_SYNC.0 | TF_ES_READWRITE.0);

    match unsafe { ctx.RequestEditSession(tid, &itf_session, flags) } {
        Ok(hr) if hr.is_ok() => {
            if keep_slot {
                let new_active = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
                manager.lock().unwrap_or_else(|e| e.into_inner()).active = new_active;
            }
        }
        _ => {
            crate::panic_guard::debug_string("Q7-tsf: 编辑会话未能执行（宿主可能拒绝写入）\n");
        }
    }
}

/// 在光标处插入文本并开始组字
fn start_composition(ctx: &ITfContext, ec: u32, text: &str) -> Result<ActiveComposition> {
    let insert: ITfInsertAtSelection = ctx.cast()?;
    let wide: Vec<u16> = text.encode_utf16().collect();
    let range =
        unsafe { insert.InsertTextAtSelection(ec, INSERT_TEXT_AT_SELECTION_FLAGS(0), &wide) }?;

    let comp_ctx: ITfContextComposition = ctx.cast()?;
    let sink: ITfCompositionSink = Q7CompositionSink.into();
    let composition = unsafe { comp_ctx.StartComposition(ec, &range, &sink) }?;

    Ok(ActiveComposition {
        composition,
        range,
        text: text.to_string(),
    })
}

/// 在光标处直接插入文本（无组字）
fn insert_text(ctx: &ITfContext, ec: u32, text: &str) -> Result<()> {
    let insert: ITfInsertAtSelection = ctx.cast()?;
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        insert.InsertTextAtSelection(ec, INSERT_TEXT_AT_SELECTION_FLAGS(0), &wide)?;
    }
    Ok(())
}

/// 替换范围文本
fn set_range_text(range: &ITfRange, ec: u32, text: &str) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe { range.SetText(ec, 0, &wide) }
}

/// 组字被宿主/系统终止时的回调。
///
/// PoC 简化：仅记录；本地状态由后续按键惰性清理（阶段 1 完善）。
#[implement(ITfCompositionSink)]
struct Q7CompositionSink;

impl ITfCompositionSink_Impl for Q7CompositionSink_Impl {
    fn OnCompositionTerminated(
        &self,
        _ecwrite: u32,
        _pcomposition: Ref<'_, ITfComposition>,
    ) -> Result<()> {
        Ok(())
    }
}
