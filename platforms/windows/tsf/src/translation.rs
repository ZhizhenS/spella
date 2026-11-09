//! Explicit selection editing. COM objects stay on the host's TSF apartment.
use crate::tip::{guarded, lock, TipState};
#[cfg(test)]
use retype_ai::protocol::MAX_TEXT;
use retype_ai::protocol::{AssistTask, Operation, Response, MAX_ASSIST_TEXT};
use std::{
    mem::ManuallyDrop,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Weak,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{implement, w, Interface, Result, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::*,
        UI::{HiDpi::GetDpiForWindow, TextServices::*, WindowsAndMessaging::*},
    },
};
const TIMER: usize = 88;
const STATUS_WIDTH: i32 = 360;
const STATUS_HEIGHT: i32 = 30;
const PREVIEW_HEIGHT: i32 = 208;
const ERROR_NOTICE_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub(crate) struct SelectedText {
    range: ITfRange,
    original: Vec<u16>,
    guard: Arc<EditGuard>,
    manager: Option<ITfThreadMgr>,
}
struct EditGuard {
    source: ITfSource,
    cookie: u32,
    changed: Arc<AtomicBool>,
}
impl Drop for EditGuard {
    fn drop(&mut self) {
        // SAFETY: guard and source stay on the TSF apartment.
        let _ = unsafe { self.source.UnadviseSink(self.cookie) };
    }
}
#[implement(ITfTextEditSink)]
struct SelectedTextObserver {
    changed: Arc<AtomicBool>,
}
impl ITfTextEditSink_Impl for SelectedTextObserver_Impl {
    fn OnEndEdit(
        &self,
        _context: windows_core::Ref<'_, ITfContext>,
        _ec: u32,
        record: windows_core::Ref<'_, ITfEditRecord>,
    ) -> Result<()> {
        guarded(|| {
            // SAFETY: TSF supplies an apartment-owned edit record. Any change,
            // including change-then-undo to the same text, permanently invalidates this request.
            let changed = unsafe {
                (|| -> Result<bool> {
                    let record = record.ok()?;
                    if record.GetSelectionStatus()?.as_bool() {
                        return Ok(true);
                    }
                    let ranges = record
                        .GetTextAndPropertyUpdates(TF_GTP_INCL_TEXT, &[&GUID_PROP_INPUTSCOPE])?;
                    let mut values = [None];
                    let mut count = 0;
                    ranges.Next(&mut values, &mut count)?;
                    Ok(count != 0)
                })()
            };
            if changed.unwrap_or(true) {
                self.changed.store(true, Ordering::Release);
            }
            Ok(())
        })
    }
}
impl SelectedText {
    #[cfg(test)]
    pub(crate) fn capture(context: &ITfContext, ec: u32) -> std::result::Result<Self, String> {
        Self::capture_with_manager(context, ec, None)
    }
    fn capture_with_manager(
        context: &ITfContext,
        ec: u32,
        manager: Option<ITfThreadMgr>,
    ) -> std::result::Result<Self, String> {
        // SAFETY: caller holds a read/write cookie, interfaces remain on this apartment.
        unsafe {
            if context
                .GetStatus()
                .map_err(|_| "无法读取输入框状态")?
                .dwDynamicFlags
                & TS_SD_READONLY
                != 0
            {
                return Err("输入框为只读".into());
            }
            let mut selected = [TF_SELECTION::default()];
            let mut count = 0;
            context
                .GetSelection(ec, TF_DEFAULT_SELECTION, &mut selected, &mut count)
                .map_err(|_| "此输入框不支持读取选区")?;
            let range = ManuallyDrop::take(&mut selected[0].range).ok_or("请先选中英文句子")?;
            if count != 1 || !explicit_public_scope(context, ec, &range, manager.as_ref()) {
                return Err("此输入框的隐私属性不允许 AI 编辑".into());
            }
            let original = read_selected(&range, ec)
                .map_err(|_| "选区过长或无法读取，请选中 2048 个字符以内的英文句子")?;
            if original.is_empty() {
                return Err("请先选中要处理的英文句子".into());
            }
            validate_text(&original)?;
            let source: ITfSource = context.cast().map_err(|_| "此应用不支持安全编辑观察")?;
            let changed = Arc::new(AtomicBool::new(false));
            let observer: ITfTextEditSink = SelectedTextObserver {
                changed: Arc::clone(&changed),
            }
            .into();
            let cookie = source
                .AdviseSink(&ITfTextEditSink::IID, &observer)
                .map_err(|_| "此应用不支持安全编辑观察")?;
            Ok(Self {
                range,
                original,
                manager,
                guard: Arc::new(EditGuard {
                    source,
                    cookie,
                    changed,
                }),
            })
        }
    }
    pub(crate) fn current(&self) -> bool {
        !self.guard.changed.load(Ordering::Acquire)
    }
    pub(crate) fn replace(&self, context: &ITfContext, ec: u32, text: &str) -> Result<()> {
        let units: Vec<u16> = text.encode_utf16().collect();
        if !self.current() || units.len() > 6_144 || validate_text(&units).is_err() {
            return Err(E_ABORT.into());
        }
        self.with_verified(context, ec, || unsafe {
            self.range.SetText(ec, 0, &units)?;
            // Text is already committed; a host rejecting caret movement must not
            // cause us to report that the original text was left unchanged.
            if let Ok(caret) = self.range.Clone() {
                if caret.Collapse(ec, TF_ANCHOR_END).is_ok() {
                    let selection = TF_SELECTION {
                        range: ManuallyDrop::new(Some(caret)),
                        style: TF_SELECTIONSTYLE {
                            ase: TF_AE_NONE,
                            fInterimChar: false.into(),
                        },
                    };
                    let _ = context.SetSelection(ec, std::slice::from_ref(&selection));
                    let _ = ManuallyDrop::into_inner(selection.range);
                }
            }
            Ok(())
        })
    }
    pub(crate) fn with_verified(
        &self,
        context: &ITfContext,
        ec: u32,
        dispatch: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if !self.current() {
            return Err(E_ABORT.into());
        }
        // SAFETY: verify selection, privacy, original content and the independent
        // revision guard while holding the fresh cookie, before reading or dispatch.
        unsafe {
            let mut selected = [TF_SELECTION::default()];
            let mut count = 0;
            context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selected, &mut count)?;
            let current = ManuallyDrop::take(&mut selected[0].range).ok_or(E_FAIL)?;
            if count != 1
                || current.CompareStart(ec, &self.range, TF_ANCHOR_START)? != 0
                || current.CompareEnd(ec, &self.range, TF_ANCHOR_END)? != 0
                || !explicit_public_scope(context, ec, &self.range, self.manager.as_ref())
                || context.GetStatus()?.dwDynamicFlags & TS_SD_READONLY != 0
                || read_selected(&self.range, ec)? != self.original
                || !self.current()
            {
                return Err(E_ABORT.into());
            }
        }
        dispatch()
    }
}
fn explicit_public_scope(
    context: &ITfContext,
    ec: u32,
    range: &ITfRange,
    manager: Option<&ITfThreadMgr>,
) -> bool {
    let mut diagnostic = ScopeDiagnostic::default();
    let allowed = inspect_public_scope(context, ec, range, manager, None, &mut diagnostic);
    // Fixed metadata only. Never include selected text, window names, paths or credentials.
    #[cfg(not(test))]
    crate::settings_log::event("launcher", "ai_scope_check", 0, format!(
        "allowed={allowed} stage={} status_hr={:?} static={:?} dynamic={:?} property_hr={:?} value_hr={:?} vt={:?} scope_hr={:?} scope_count={:?} enum_hr={:?} enum_next_hr={:?} ranges={} scopes={:?}",
        diagnostic.stage, diagnostic.status_hr, diagnostic.static_flags, diagnostic.dynamic_flags,
        diagnostic.property_hr, diagnostic.value_hr, diagnostic.variant, diagnostic.scope_hr,
        diagnostic.scope_count, diagnostic.enum_hr, diagnostic.enum_next_hr,
        diagnostic.ranges, diagnostic.scopes,
    ));
    allowed
}
/// Local-only metadata gate: inspect the requested range's declared scopes, with
/// native compatibility anchored to the *current* selection rather than nearby text.
pub(crate) fn local_public_scope(
    context: &ITfContext,
    ec: u32,
    range: &ITfRange,
    selection: &ITfRange,
    manager: Option<&ITfThreadMgr>,
) -> bool {
    inspect_public_scope(
        context,
        ec,
        range,
        manager,
        Some(selection),
        &mut ScopeDiagnostic::default(),
    )
}
#[derive(Default)]
struct ScopeDiagnostic {
    stage: &'static str,
    status_hr: Option<i32>,
    static_flags: Option<u32>,
    dynamic_flags: Option<u32>,
    property_hr: Option<i32>,
    value_hr: Option<i32>,
    variant: Option<u16>,
    scope_hr: Option<i32>,
    scope_count: Option<u32>,
    enum_hr: Option<i32>,
    enum_next_hr: Option<i32>,
    ranges: usize,
    scopes: Vec<i32>,
}
#[derive(PartialEq, Eq, Debug)]
enum ScopeValue {
    Absent,
    Public,
    Reject,
}
fn scope_value(
    hr: windows_core::HRESULT,
    value: &windows::Win32::System::Variant::VARIANT,
    diagnostic: &mut ScopeDiagnostic,
) -> ScopeValue {
    use windows::Win32::System::Variant::{VT_EMPTY, VT_UNKNOWN};
    // SAFETY: read only the discriminator before converting a validated VARIANT.
    let variant = unsafe { value.Anonymous.Anonymous.vt };
    diagnostic.value_hr = Some(hr.0);
    diagnostic.variant = Some(variant.0);
    // Native TSF also returns S_OK when the owner explicitly returns a supported
    // TS_ATTRVAL with VT_EMPTY. Neither success proves whole-range absence.
    if matches!(hr, S_OK | S_FALSE) && variant == VT_EMPTY {
        return ScopeValue::Absent;
    }
    if hr != S_OK || variant != VT_UNKNOWN {
        return ScopeValue::Reject;
    }
    let scope = match windows_core::IUnknown::try_from(value)
        .and_then(|unknown| unknown.cast::<ITfInputScope>())
    {
        Ok(scope) => scope,
        Err(error) => {
            diagnostic.scope_hr = Some(error.code().0);
            diagnostic.stage = "scope_query";
            return ScopeValue::Reject;
        }
    };
    // SAFETY: COM owns the output allocation; check status/count before slicing
    // and always release it. S_FALSE/zero/null are not documented default scopes.
    unsafe {
        let mut scopes = std::ptr::null_mut();
        let mut count = 0;
        let result = (scope.vtable().GetInputScopes)(scope.as_raw(), &mut scopes, &mut count);
        diagnostic.scope_hr = Some(result.0);
        diagnostic.scope_count = Some(count);
        diagnostic.stage = "scope_array";
        let allowed = result == S_OK && (1..=64).contains(&count) && !scopes.is_null() && {
            let scopes = std::slice::from_raw_parts(scopes, count as usize);
            diagnostic.scopes.extend(
                scopes
                    .iter()
                    .take(16usize.saturating_sub(diagnostic.scopes.len()))
                    .map(|scope| scope.0),
            );
            scopes
                .iter()
                .all(|scope| matches!(*scope, IS_DEFAULT | IS_TEXT))
        };
        windows::Win32::System::Com::CoTaskMemFree(Some(scopes.cast()));
        if allowed {
            ScopeValue::Public
        } else {
            ScopeValue::Reject
        }
    }
}
fn scope_status_allows_editing(status: &TS_STATUS) -> bool {
    // NOHIDDENTEXT describes a document capability, not whether a field is private.
    status.dwDynamicFlags & (TS_SD_READONLY | TS_SD_LOADING) == 0
}
fn inspect_public_scope(
    context: &ITfContext,
    ec: u32,
    range: &ITfRange,
    manager: Option<&ITfThreadMgr>,
    local_selection: Option<&ITfRange>,
    diagnostic: &mut ScopeDiagnostic,
) -> bool {
    // SAFETY: the caller holds a cookie for this context/range. Preserve raw
    // HRESULTs because generated Result bindings erase the S_FALSE distinction.
    unsafe {
        diagnostic.stage = "status";
        let mut status = TS_STATUS::default();
        let status_hr = (context.vtable().GetStatus)(context.as_raw(), &mut status);
        diagnostic.status_hr = Some(status_hr.0);
        if status_hr != S_OK {
            return false;
        }
        diagnostic.static_flags = Some(status.dwStaticFlags);
        diagnostic.dynamic_flags = Some(status.dwDynamicFlags);
        if !scope_status_allows_editing(&status) {
            return false;
        }
        diagnostic.stage = "property";
        let mut raw = std::ptr::null_mut();
        let hr =
            (context.vtable().GetAppProperty)(context.as_raw(), &GUID_PROP_INPUTSCOPE, &mut raw);
        diagnostic.property_hr = Some(hr.0);
        let property = (!raw.is_null()).then(|| ITfReadOnlyProperty::from_raw(raw));
        // Unsupported/missing interfaces and errors do not prove a default field.
        let Some(property) = property.filter(|_| hr == S_OK) else {
            return false;
        };
        let read_value = |target: &ITfRange, diagnostic: &mut ScopeDiagnostic| {
            let mut value = windows::Win32::System::Variant::VARIANT::default();
            let hr =
                (property.vtable().GetValue)(property.as_raw(), ec, target.as_raw(), &mut value);
            scope_value(hr, &value, diagnostic)
        };
        diagnostic.stage = "value";
        match read_value(range, diagnostic) {
            ScopeValue::Public => {
                diagnostic.stage = "explicit_public";
                return true;
            }
            ScopeValue::Reject => {
                if diagnostic
                    .value_hr
                    .zip(diagnostic.variant)
                    .is_some_and(|(hr, vt)| crate::native_scope::eligible(hr, vt))
                {
                    let allowed = if let Some(selection) = local_selection {
                        crate::native_scope::verify_local(manager, context, ec, selection, range)
                    } else {
                        crate::native_scope::verify(manager, context, ec, range)
                    };
                    diagnostic.stage = if allowed {
                        "native_plain_compat"
                    } else {
                        "native_plain_rejected"
                    };
                    return allowed;
                }
                return false;
            }
            ScopeValue::Absent => {}
        }
        // GetValue(success, VT_EMPTY) also means mixed/partial coverage. Enumerate
        // every clipped subrange, including empty values, and verify exact coverage.
        diagnostic.stage = "enum_ranges";
        let mut raw = std::ptr::null_mut();
        let hr = (property.vtable().EnumRanges)(property.as_raw(), ec, &mut raw, range.as_raw());
        diagnostic.enum_hr = Some(hr.0);
        let ranges = (!raw.is_null()).then(|| IEnumTfRanges::from_raw(raw));
        let Some(ranges) = ranges.filter(|_| hr == S_OK) else {
            return false;
        };
        let Ok(mut cursor) = range.Clone() else {
            return false;
        };
        if cursor.Collapse(ec, TF_ANCHOR_START).is_err() {
            return false;
        }
        let mut absent = false;
        loop {
            let mut raw = std::ptr::null_mut();
            let mut fetched = 0;
            let hr = (ranges.vtable().Next)(ranges.as_raw(), 1, &mut raw, &mut fetched);
            diagnostic.enum_next_hr = Some(hr.0);
            let chunk = (!raw.is_null()).then(|| ITfRange::from_raw(raw));
            if hr == S_FALSE && fetched == 0 && chunk.is_none() {
                break;
            }
            diagnostic.stage = "enum_partition";
            let Some(chunk) = chunk.filter(|_| hr == S_OK && fetched == 1) else {
                return false;
            };
            diagnostic.ranges += 1;
            if diagnostic.ranges > MAX_ASSIST_TEXT
                || !chunk.IsEmpty(ec).is_ok_and(|empty| !empty.as_bool())
                || chunk.CompareStart(ec, &cursor, TF_ANCHOR_END).ok() != Some(0)
                || !chunk
                    .CompareEnd(ec, range, TF_ANCHOR_END)
                    .is_ok_and(|end| end <= 0)
            {
                return false;
            }
            diagnostic.stage = "enum_value";
            match read_value(&chunk, diagnostic) {
                ScopeValue::Public => {}
                ScopeValue::Absent => absent = true,
                ScopeValue::Reject => return false,
            }
            cursor = chunk;
        }
        diagnostic.stage = "enum_coverage";
        if diagnostic.ranges == 0 || cursor.CompareEnd(ec, range, TF_ANCHOR_END).ok() != Some(0) {
            return false;
        }
        diagnostic.stage = if absent {
            "verified_default"
        } else {
            "enumerated_public"
        };
        true
    }
}
fn read_selected(range: &ITfRange, ec: u32) -> Result<Vec<u16>> {
    // SAFETY: bounded clone-based range read; the original anchors are never moved.
    unsafe {
        let expected_length = if let Ok(acp) = range.cast::<ITfRangeACP>() {
            let mut start = 0;
            let mut length = 0;
            acp.GetExtent(&mut start, &mut length)?;
            if length < 0 || length as usize > MAX_ASSIST_TEXT {
                return Err(E_INVALIDARG.into());
            }
            Some(length as usize)
        } else {
            None
        };
        let cursor = range.Clone()?;
        let mut text = Vec::new();
        loop {
            let mut buffer = [0u16; 512];
            let mut count = 0;
            // Do not let a short read implicitly move the start anchor. Advance
            // by the actual returned UTF-16 count and verify that movement.
            cursor.GetText(ec, 0, &mut buffer, &mut count)?;
            if count == 0 {
                // A zero-length response is not proof that the selected range
                // was read completely. A truncated host must never let a prefix
                // authorize replacement of the entire original range.
                if !cursor.IsEmpty(ec)?.as_bool() {
                    return Err(E_FAIL.into());
                }
                break;
            }
            if count as usize > buffer.len() || text.len() + count as usize > MAX_ASSIST_TEXT {
                return Err(E_INVALIDARG.into());
            }
            text.extend_from_slice(&buffer[..count as usize]);
            let mut moved = 0;
            cursor.ShiftStart(ec, count as i32, &mut moved, std::ptr::null())?;
            if moved != count as i32 || cursor.CompareEnd(ec, range, TF_ANCHOR_END)? != 0 {
                return Err(E_FAIL.into());
            }
        }
        if expected_length.is_some_and(|length| length != text.len()) {
            return Err(E_FAIL.into());
        }
        Ok(text)
    }
}

struct Frame {
    state: Weak<TipState>,
    context: ITfContext,
    epoch: u32,
    original: Vec<u16>,
    selected: Option<SelectedText>,
    task: AssistTask,
    response: Option<mpsc::Receiver<std::result::Result<Response, String>>>,
    cancelled: Arc<AtomicBool>,
    translation: Option<String>,
    message: String,
    scroll: i32,
    max_scroll: i32,
    owner: HWND,
    applying: bool,
    applied: bool,
    busy: bool,
    preview: bool,
    spinner_phase: u32,
    hide_at: Option<Instant>,
}
impl Drop for Frame {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
pub(crate) fn dismiss(state: &TipState) {
    let handle = lock(&state.translation_window).take();
    if let Some(handle) = handle {
        // SAFETY: this window is created and destroyed on this TSF apartment.
        unsafe {
            let _ = DestroyWindow(HWND(handle as *mut _));
        }
    }
}
pub(crate) fn capture(state: &Arc<TipState>, context: &ITfContext, ec: u32) -> Result<()> {
    capture_or_notice(state, context, ec, false)
}
pub(crate) fn selection_required(
    state: &Arc<TipState>,
    context: &ITfContext,
    ec: u32,
) -> Result<()> {
    capture_or_notice(state, context, ec, true)
}
fn capture_or_notice(
    state: &Arc<TipState>,
    context: &ITfContext,
    ec: u32,
    composing: bool,
) -> Result<()> {
    dismiss(state);
    let snapshot = if composing {
        Err("请先完成当前单词，再选中要处理的英文句子".into())
    } else if state.session().is_some_and(|session| session.shadow) {
        Err("此输入框暂不支持 AI 选区编辑".into())
    } else {
        let manager = lock(&state.thread_mgr).clone();
        SelectedText::capture_with_manager(context, ec, manager)
    };
    let (selected, original, message) = match snapshot {
        Ok(selected) => {
            let original = selected.original.clone();
            (Some(selected), original, "正在修正语法…".to_owned())
        }
        Err(e) => (None, Vec::new(), e),
    };
    let owner = unsafe { context.GetActiveView().and_then(|v| v.GetWnd()) }.unwrap_or_default();
    let cancelled = Arc::new(AtomicBool::new(false));
    let data = Frame {
        state: Arc::downgrade(state),
        context: context.clone(),
        epoch: state.epoch.load(Ordering::SeqCst),
        original: original.clone(),
        selected,
        task: AssistTask::GrammarCorrect,
        response: None,
        cancelled: Arc::clone(&cancelled),
        translation: None,
        message,
        scroll: 0,
        max_scroll: 0,
        owner,
        applying: false,
        applied: false,
        busy: !original.is_empty(),
        preview: false,
        spinner_phase: 0,
        hide_at: None,
    };
    let hwnd = create(data, context, ec)?;
    *lock(&state.translation_window) = Some(hwnd.0 as usize);
    if !original.is_empty() {
        start_task(hwnd, AssistTask::GrammarCorrect);
    }
    Ok(())
}
fn start_task(hwnd: HWND, task: AssistTask) {
    let data = unsafe { frame(hwnd) }.and_then(|f| {
        Some((
            f.state.upgrade()?,
            f.context.clone(),
            f.epoch,
            f.owner,
            f.selected.clone()?,
        ))
    });
    let Some((state, context, epoch, owner, selected)) = data else {
        return;
    };
    if !selected.current() || !same_target(&state, &context, epoch, owner) {
        status(hwnd, "选区已变化，请重新选中后重试");
        return;
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    unsafe {
        let Some(f) = frame(hwnd) else { return };
        f.cancelled.store(true, Ordering::Release);
        f.cancelled = Arc::clone(&cancelled);
        f.response = None;
        f.translation = None;
        f.preview = false;
        f.busy = true;
        f.task = task;
        f.scroll = 0;
        f.message = match task {
            AssistTask::GrammarCorrect => "正在修正语法…",
            AssistTask::CompleteSentence => "正在补完所选句子…",
        }
        .into();
        f.hide_at = None;
    }
    resize_notice(hwnd, false);
    let validation: ITfEditSession = ValidateTask {
        state: Arc::downgrade(&state),
        context: context.clone(),
        selected,
        epoch,
        owner,
        cancelled: Arc::clone(&cancelled),
        window: hwnd.0 as usize,
        task,
    }
    .into();
    // Always queue a fresh read transaction, including panel retries/completions.
    // No worker receives text until this later cookie revalidates the source.
    let result = unsafe {
        context.RequestEditSession(
            state.tid.load(Ordering::SeqCst),
            &validation,
            TF_ES_ASYNC | TF_ES_READ,
        )
    };
    if result.is_err() || result.is_ok_and(|session| session.is_err()) {
        if !cancelled.load(Ordering::Acquire) {
            status(hwnd, "应用拒绝验证选区，未发送文本");
        }
    }
}
#[implement(ITfEditSession)]
struct ValidateTask {
    state: Weak<TipState>,
    context: ITfContext,
    selected: SelectedText,
    epoch: u32,
    owner: HWND,
    cancelled: Arc<AtomicBool>,
    window: usize,
    task: AssistTask,
}
impl ITfEditSession_Impl for ValidateTask_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        guarded(|| {
            if self.cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            let Some(state) = self.state.upgrade() else {
                return Ok(());
            };
            let hwnd = HWND(self.window as *mut _);
            let result = if same_target(&state, &self.context, self.epoch, self.owner) {
                self.selected.with_verified(&self.context, ec, || {
                    if self.cancelled.load(Ordering::Acquire)
                        || !same_target(&state, &self.context, self.epoch, self.owner)
                    {
                        return Err(E_ABORT.into());
                    }
                    spawn_verified_task(
                        hwnd,
                        self.task,
                        &self.selected.original,
                        Arc::clone(&self.cancelled),
                    )
                })
            } else {
                Err(E_ABORT.into())
            };
            if result.is_err() && !self.cancelled.load(Ordering::Acquire) {
                status(hwnd, "选区或隐私属性已变化，未发送文本");
            }
            Ok(())
        })
    }
}
fn spawn_verified_task(
    hwnd: HWND,
    task: AssistTask,
    original: &[u16],
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err(E_ABORT.into());
    }
    let text = String::from_utf16(original).map_err(|_| windows_core::Error::from(E_INVALIDARG))?;
    let (tx, rx) = mpsc::channel();
    // SAFETY: validation and the notice both live on the same TSF apartment.
    let Some(f) = (unsafe { frame(hwnd) }) else {
        return Err(E_ABORT.into());
    };
    if !Arc::ptr_eq(&f.cancelled, &cancelled) {
        return Err(E_ABORT.into());
    }
    f.response = Some(rx);
    std::thread::Builder::new()
        .name("english-assist-edit".into())
        .spawn(move || {
            let operation = match task {
                AssistTask::GrammarCorrect => Operation::GrammarCorrect { text },
                AssistTask::CompleteSentence => Operation::CompleteSentence { text },
            };
            let result = retype_ai::client::run(operation, &cancelled);
            let _ = tx.send(result);
        })
        .map_err(|_| windows_core::Error::from(E_FAIL))?;
    Ok(())
}

/// Only consume preview controls while a live editing preview owns them.
pub(crate) fn handle_key(state: &TipState, vk: usize, modifiers: u8, test: bool) -> bool {
    let window = *lock(&state.translation_window);
    let Some(window) = window else { return false };
    let hwnd = HWND(window as *mut _);
    let is_escape = vk == 0x1b && modifiers == 0;
    let target = unsafe { frame(hwnd) }.and_then(|f| {
        (f.preview && !f.busy && !f.applying && !f.applied)
            .then(|| (f.context.clone(), f.epoch, f.owner, f.selected.clone()))
    });
    let can_apply = target.is_some_and(|(context, epoch, owner, selected)| {
        selected.as_ref().is_some_and(SelectedText::current)
            && same_target(state, &context, epoch, owner)
    });
    if is_escape {
        if !test {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        return true;
    }
    if vk == 0x0d && modifiers == 1 && can_apply {
        if !test {
            request_apply(hwnd);
        }
        return true;
    }
    false
}

#[cfg(test)]
/// Read to both context anchors in bounded chunks; never use IGNOREEND or a truncated prefix.
pub(crate) fn read(context: &ITfContext, ec: u32) -> std::result::Result<Vec<u16>, String> {
    let status = unsafe { context.GetStatus() };
    crate::settings_log::event(
        "launcher",
        "translation_context",
        0,
        format!(
            "static_flags={} dynamic_flags={} hresult={}",
            status.as_ref().map_or(0, |s| s.dwStaticFlags),
            status.as_ref().map_or(0, |s| s.dwDynamicFlags),
            status.as_ref().err().map_or(0, |e| e.code().0)
        ),
    );
    // SAFETY: ec is a granted read/write or read cookie for this context.
    unsafe {
        if context
            .GetStatus()
            .map_err(|_| "无法读取输入框状态")?
            .dwDynamicFlags
            & TS_SD_READONLY
            != 0
        {
            return Err("输入框为只读".into());
        }
        let range = context
            .GetStart(ec)
            .map_err(|_| "此应用不支持输入框全文读取")?;
        let end = context
            .GetEnd(ec)
            .map_err(|_| "此应用不支持输入框全文读取")?;
        range
            .ShiftEndToRange(ec, &end, TF_ANCHOR_END)
            .map_err(|_| "无法确定输入框全文范围")?;
        if !crate::edit::allowed_input_scope(context, ec, &range) {
            return Err("此输入框的密码或隐私属性不允许读取全文".into());
        }
        // ACP bounds confirm this is a complete text-store range rather than a composition fragment.
        let start_acp: ITfRangeACP = range.cast().map_err(|_| "此输入框无法确认全文范围")?;
        let mut start = 0;
        let mut length = 0;
        start_acp
            .GetExtent(&mut start, &mut length)
            .map_err(|_| "此输入框无法确认全文范围")?;
        if start != 0 || length < 0 || length as usize > MAX_TEXT {
            return Err("输入框范围不完整或文字过长".into());
        }
        let cursor = range.Clone().map_err(|_| "无法读取全文")?;
        let mut text = Vec::new();
        loop {
            let mut buffer = [0u16; 2048];
            let mut count = 0;
            cursor
                .GetText(ec, TF_TF_MOVESTART, &mut buffer, &mut count)
                .map_err(|_| "无法读取输入框全文")?;
            if count as usize > buffer.len() {
                return Err("应用返回的文字长度无效".into());
            }
            if count == 0 {
                break;
            }
            text.extend_from_slice(&buffer[..count as usize]);
            if text.len() > MAX_TEXT {
                return Err("输入框文字过长，原文保持不变".into());
            }
        }
        if text.len() != length as usize {
            return Err("应用未提供完整文字，原文保持不变".into());
        }
        validate_text(&text)?;
        Ok(text)
    }
}
fn validate_text(text: &[u16]) -> std::result::Result<(), String> {
    let text = String::from_utf16(text).map_err(|_| "输入框文字编码无效")?;
    if text.trim().is_empty() {
        return Err("输入框为空".into());
    }
    if text.contains(['\u{fffc}', '\0']) {
        return Err("输入框包含嵌入对象，暂不支持全文替换".into());
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn replace_verified(
    context: &ITfContext,
    ec: u32,
    original: &[u16],
    text: &str,
) -> std::result::Result<(), String> {
    replace_checked(context, ec, original, text, || true)
}
#[cfg(test)]
fn replace_checked(
    context: &ITfContext,
    ec: u32,
    original: &[u16],
    text: &str,
    current: impl Fn() -> bool,
) -> std::result::Result<(), String> {
    if read(context, ec)? != original {
        return Err("原文已变化，未替换".into());
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() > MAX_TEXT {
        return Err("译文过长，未替换".into());
    }
    validate_text(&units)?;
    // SAFETY: caller holds the verified context's write cookie; one range write, no pre-clear.
    unsafe {
        let start = context.GetStart(ec).map_err(|_| "无法读取全文范围")?;
        let end = context.GetEnd(ec).map_err(|_| "无法读取全文范围")?;
        start
            .ShiftEndToRange(ec, &end, TF_ANCHOR_END)
            .map_err(|_| "无法读取全文范围")?;
        if !current() {
            return Err("翻译已取消或输入框已切换，未替换".into());
        }
        start.SetText(ec, 0, &units).map_err(|error| {
            crate::settings_log::event(
                "launcher",
                "translation_replace_failed",
                0,
                format!("stage=set_text hresult={:08x}", error.code().0),
            );
            "应用拒绝替换，原文保持不变"
        })?;
        if start.Collapse(ec, TF_ANCHOR_END).is_err() {
            return Ok(());
        }
        let selection = TF_SELECTION {
            range: ManuallyDrop::new(Some(start)),
            style: TF_SELECTIONSTYLE {
                ase: TF_AE_NONE,
                fInterimChar: false.into(),
            },
        };
        let result = context.SetSelection(ec, std::slice::from_ref(&selection));
        let mut selection = selection;
        drop(ManuallyDrop::take(&mut selection.range));
        // The replacement succeeded even if a host refuses to move its caret.
        let _ = result;
    }
    Ok(())
}
pub(crate) fn same_target(state: &TipState, context: &ITfContext, epoch: u32, owner: HWND) -> bool {
    if !state.is_activated()
        || state.epoch.load(Ordering::SeqCst) != epoch
        || lock(&state.composition).is_some()
    {
        return false;
    }
    let Some(manager) = lock(&state.thread_mgr).clone() else {
        return false;
    };
    // SAFETY: read-only focus checks on the owning COM apartment.
    unsafe {
        let focused = manager
            .GetFocus()
            .and_then(|d| d.GetTop())
            .is_ok_and(|c| c == *context);
        let foreground = GetForegroundWindow();
        let root = GetAncestor(owner, GA_ROOT);
        focused && !owner.is_invalid() && !root.is_invalid() && foreground == root
    }
}
fn request_apply(hwnd: HWND) {
    let data = unsafe { frame(hwnd) }.and_then(|f| {
        if f.applying || f.applied {
            return None;
        }
        let state = f.state.upgrade()?;
        let text = f.translation.clone()?;
        f.applying = true;
        Some((
            state,
            f.context.clone(),
            f.selected.clone()?,
            text,
            f.epoch,
            f.owner,
            Arc::clone(&f.cancelled),
        ))
    });
    let Some((state, context, selected, text, epoch, owner, cancelled)) = data else {
        return;
    };
    if !selected.current() || !same_target(&state, &context, epoch, owner) {
        status(hwnd, "输入框已切换或正在输入，原文保持不变");
        return;
    }
    let session: ITfEditSession = Apply {
        state: Arc::downgrade(&state),
        context: context.clone(),
        selected,
        text,
        epoch,
        owner,
        cancelled,
        window: hwnd.0 as usize,
    }
    .into();
    // SAFETY: TSF schedules this object on the current context apartment; no worker holds COM.
    let result = unsafe {
        context.RequestEditSession(
            state.tid.load(Ordering::SeqCst),
            &session,
            TF_ES_ASYNCDONTCARE | TF_ES_READWRITE,
        )
    };
    let failure = match result {
        Ok(session) if session.is_ok() => None,
        Ok(session) => Some(("session_result", session.0)),
        Err(error) => Some(("request_edit_session", error.code().0)),
    };
    if let Some((stage, code)) = failure {
        crate::settings_log::event(
            "launcher",
            "translation_replace_failed",
            0,
            format!("stage={stage} hresult={code:08x}"),
        );
        status(hwnd, "应用拒绝修改，原文保持不变");
    }
}
#[implement(ITfEditSession)]
struct Apply {
    state: Weak<TipState>,
    context: ITfContext,
    selected: SelectedText,
    text: String,
    epoch: u32,
    owner: HWND,
    cancelled: Arc<AtomicBool>,
    window: usize,
}
impl ITfEditSession_Impl for Apply_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        guarded(|| {
            let hwnd = HWND(self.window as *mut _);
            if self.cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            let Some(state) = self.state.upgrade() else {
                return Ok(());
            };
            if !same_target(&state, &self.context, self.epoch, self.owner) {
                status(hwnd, "输入框已切换，原文保持不变");
                return Ok(());
            }
            let result = self.selected.replace(&self.context, ec, &self.text);
            if result.is_ok() {
                status(hwnd, "修改完成");
            } else if result.is_err() {
                status(hwnd, "原文已变化或应用拒绝修改，请重新选中后重试");
            }
            // Translation is not typing activity and does not enter personal word learning.
            Ok(())
        })
    }
}
unsafe fn frame<'a>(hwnd: HWND) -> Option<&'a mut Frame> {
    // SAFETY: GWLP_USERDATA is our exclusive Box<Frame>, accessed only on its window thread.
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Frame;
    unsafe { pointer.as_mut() }
}
fn status(hwnd: HWND, message: &str) {
    // SAFETY: called only on the window owner thread, outside any existing frame borrow.
    let preview = unsafe {
        if let Some(f) = frame(hwnd) {
            f.message = message.into();
            f.applying = false;
            f.busy = false;
            if message == "修改完成" {
                f.applied = true;
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            let preview = f.preview && f.translation.is_some() && !f.applied;
            f.hide_at = if preview {
                None
            } else {
                Some(
                    Instant::now()
                        + if message == "修改完成" {
                            Duration::from_millis(1400)
                        } else {
                            ERROR_NOTICE_DURATION
                        },
                )
            };
            preview
        } else {
            false
        }
    };
    resize_notice(hwnd, preview);
}
fn resize_notice(hwnd: HWND, preview: bool) {
    // SAFETY: resize the non-activating owned window within its monitor work area.
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let content = frame(hwnd).map(|f| (f.message.clone(), f.busy || f.applying));
        let width = if preview {
            STATUS_WIDTH * dpi / 96
        } else if let Some((message, loading)) = content {
            notice_width(hwnd, dpi, &message, loading)
        } else {
            STATUS_WIDTH * dpi / 96
        };
        let height = (if preview {
            PREVIEW_HEIGHT
        } else {
            STATUS_HEIGHT
        }) * dpi
            / 96;
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST), &mut info).as_bool() {
            return;
        }
        let top = rect.top.clamp(
            info.rcWork.top,
            (info.rcWork.bottom - height).max(info.rcWork.top),
        );
        let left = rect.left.clamp(
            info.rcWork.left,
            (info.rcWork.right - width).max(info.rcWork.left),
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            left,
            top,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        round_notice(hwnd, width, height, dpi);
    }
}
fn round_notice(hwnd: HWND, width: i32, height: i32, dpi: i32) {
    // SAFETY: The system owns a successfully assigned region; failed assignment
    // retains ownership here. This clips the popup's corners without a shadow.
    unsafe {
        let radius = 12 * dpi / 96;
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
        }
    }
}
fn notice_font(dpi: i32) -> HFONT {
    // SAFETY: Fixed font face; callers own and delete the returned GDI font.
    unsafe {
        CreateFontW(
            -12 * dpi / 96,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Microsoft YaHei"),
        )
    }
}
fn notice_width(hwnd: HWND, dpi: i32, message: &str, loading: bool) -> i32 {
    // SAFETY: Temporary owner-thread DC and font, with no frame borrow held.
    unsafe {
        let dc = GetDC(Some(hwnd));
        if dc.is_invalid() {
            return STATUS_WIDTH * dpi / 96;
        }
        let font = notice_font(dpi);
        let old = SelectObject(dc, font.into());
        let text: Vec<u16> = message.encode_utf16().collect();
        let mut size = SIZE::default();
        let measured = text.is_empty() || GetTextExtentPoint32W(dc, &text, &mut size).as_bool();
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
        ReleaseDC(Some(hwnd), dc);
        if !measured {
            return STATUS_WIDTH * dpi / 96;
        }
        (size.cx + (if loading { 30 } else { 10 } + 10) * dpi / 96)
            .clamp(100 * dpi / 96, STATUS_WIDTH * dpi / 96)
    }
}
fn create(mut data: Frame, context: &ITfContext, ec: u32) -> Result<HWND> {
    // SAFETY: this module owns window class, user data and all GDI resources.
    unsafe {
        let mut module = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(window_proc as *const () as *const u16),
            &mut module,
        )?;
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("Retype.Translation.1"),
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("Retype.Translation.1"),
            w!("English Assist 句子编辑"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            Some(data.owner),
            None,
            Some(module.into()),
            None,
        )?;
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let width = notice_width(hwnd, dpi, &data.message, data.busy);
        let height = STATUS_HEIGHT * dpi / 96;
        let mut rect = RECT::default();
        let mut selected = [TF_SELECTION::default()];
        let mut count = 0;
        let _ = context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selected, &mut count);
        let range = ManuallyDrop::take(&mut selected[0].range);
        let mut clipped = windows_core::BOOL(0);
        let anchored = range.as_ref().is_some_and(|r| {
            context
                .GetActiveView()
                .and_then(|v| v.GetTextExt(ec, r, &mut rect, &mut clipped))
                .is_ok()
        });
        if !anchored {
            let _ = GetWindowRect(data.owner, &mut rect);
        }
        let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let x = rect.left.clamp(
            info.rcWork.left,
            (info.rcWork.right - width).max(info.rcWork.left),
        );
        let y = (rect.bottom + 8).clamp(
            info.rcWork.top,
            (info.rcWork.bottom - height).max(info.rcWork.top),
        );
        data.hide_at = (!data.busy).then(|| Instant::now() + ERROR_NOTICE_DURATION);
        let pointer = Box::into_raw(Box::new(data));
        #[cfg(target_pointer_width = "64")]
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as isize);
        #[cfg(target_pointer_width = "32")]
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as i32);
        round_notice(hwnd, width, height, dpi);
        if let Err(error) = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) {
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
        if SetTimer(Some(hwnd), TIMER, 100, None) == 0 {
            let error = windows::core::Error::from_thread();
            crate::settings_log::event(
                "launcher",
                "translation_timer_failed",
                0,
                format!("hresult={:08x}", error.code().0),
            );
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
        Ok(hwnd)
    }
}
unsafe fn paint(hwnd: HWND) {
    // SAFETY: Paint/DC/bitmap lifetimes are paired, and selected objects are
    // restored before deletion. The window only receives a completed frame.
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let target = BeginPaint(hwnd, &mut ps);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let buffer = CreateCompatibleDC(Some(target));
        let bitmap = CreateCompatibleBitmap(target, rect.right.max(1), rect.bottom.max(1));
        let buffered = !buffer.is_invalid() && !bitmap.is_invalid();
        let previous_bitmap = if buffered {
            Some(SelectObject(buffer, bitmap.into()))
        } else {
            None
        };
        let dc = if buffered { buffer } else { target };
        let brush = CreateSolidBrush(COLORREF(0x00fafafa));
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let pad = 12 * dpi / 96;
        let bar = STATUS_HEIGHT * dpi / 96;
        let font = notice_font(dpi);
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00807870));
        if let Some(f) = frame(hwnd) {
            let loading = f.busy || f.applying;
            if loading {
                draw_spinner(
                    dc,
                    15 * dpi / 96,
                    bar / 2,
                    5 * dpi / 96,
                    f.spinner_phase,
                    dpi,
                );
            }
            if f.preview && !f.applied {
                let close_pen = CreatePen(PS_SOLID, (dpi / 96).max(1), COLORREF(0x008a7c6c));
                let old_pen = SelectObject(dc, close_pen.into());
                let center = rect.right - 20 * dpi / 96;
                let half = 4 * dpi / 96;
                let _ = MoveToEx(dc, center - half, bar / 2 - half, None);
                let _ = LineTo(dc, center + half, bar / 2 + half);
                let _ = MoveToEx(dc, center + half, bar / 2 - half, None);
                let _ = LineTo(dc, center - half, bar / 2 + half);
                SelectObject(dc, old_pen);
                let _ = DeleteObject(close_pen.into());
            }
            let mut heading = RECT {
                left: if loading {
                    30 * dpi / 96
                } else {
                    10 * dpi / 96
                },
                top: 0,
                right: rect.right - (if f.preview && !f.applied { 38 } else { 10 }) * dpi / 96,
                bottom: bar,
            };
            let mut message: Vec<u16> = f.message.encode_utf16().collect();
            if !message.is_empty() {
                DrawTextW(
                    dc,
                    &mut message,
                    &mut heading,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            let top = bar;
            let bottom = rect.bottom - 40 * dpi / 96;
            let saved = SaveDC(dc);
            IntersectClipRect(dc, pad, top, rect.right - pad, bottom);
            let mut body = RECT {
                left: pad,
                top: top - f.scroll,
                right: rect.right - pad,
                bottom: i32::MAX / 4,
            };
            let preview_text = f
                .translation
                .as_ref()
                .map(|text| {
                    format!(
                        "原文\n{}\n\n修改\n{text}",
                        String::from_utf16_lossy(&f.original)
                    )
                })
                .unwrap_or_default();
            let mut text: Vec<u16> = preview_text.encode_utf16().collect();
            let mut measured = RECT {
                left: pad,
                top: 0,
                right: rect.right - pad,
                bottom: 0,
            };
            // DrawTextW still dereferences its string pointer for a zero-length
            // slice. An empty Vec supplies dangling address 0x2, which crashes
            // the host while the progress/error window has no translation yet.
            f.max_scroll = 0;
            if f.preview && !f.applied && !text.is_empty() && bottom > top {
                DrawTextW(
                    dc,
                    &mut text,
                    &mut measured,
                    DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
                );
                f.max_scroll = (measured.bottom - (bottom - top)).max(0);
                DrawTextW(dc, &mut text, &mut body, DT_WORDBREAK | DT_NOPREFIX);
            }
            let _ = RestoreDC(dc, saved);
            if f.preview && !f.applied && !f.applying {
                for (left, right, label) in [
                    (12, 112, "修正语法"),
                    (116, 222, "补完此句"),
                    (242, 348, "应用修改"),
                ] {
                    let mut button = RECT {
                        left: left * dpi / 96,
                        right: right * dpi / 96,
                        top: bottom,
                        bottom: rect.bottom - 8 * dpi / 96,
                    };
                    DrawTextW(
                        dc,
                        &mut label.encode_utf16().collect::<Vec<_>>(),
                        &mut button,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                    );
                }
            }
        }
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
        if let Some(previous) = previous_bitmap {
            // BeginPaint clips this copy to the invalid region, so animation
            // frames never touch the text or the rest of the notice.
            let _ = BitBlt(
                target,
                0,
                0,
                rect.right,
                rect.bottom,
                Some(buffer),
                0,
                0,
                SRCCOPY,
            );
            SelectObject(buffer, previous);
        }
        if !bitmap.is_invalid() {
            let _ = DeleteObject(bitmap.into());
        }
        if !buffer.is_invalid() {
            let _ = DeleteDC(buffer);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

fn draw_spinner(dc: HDC, x: i32, y: i32, radius: i32, phase: u32, dpi: i32) {
    // SAFETY: Owner-thread paint DC; restore selected objects before deleting pens.
    unsafe {
        let brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        let track = CreatePen(PS_SOLID, (2 * dpi / 96).max(1), COLORREF(0x00e9e5df));
        let pen = SelectObject(dc, track.into());
        let _ = Ellipse(dc, x - radius, y - radius, x + radius, y + radius);
        let active = CreatePen(PS_SOLID, (dpi / 96).max(1), COLORREF(0x00a09080));
        SelectObject(dc, active.into());
        let angle = phase as f32 * std::f32::consts::TAU / 12.0;
        let end = angle + std::f32::consts::TAU * 0.7;
        let _ = Arc(
            dc,
            x - radius,
            y - radius,
            x + radius,
            y + radius,
            x + (radius as f32 * angle.cos()).round() as i32,
            y + (radius as f32 * angle.sin()).round() as i32,
            x + (radius as f32 * end.cos()).round() as i32,
            y + (radius as f32 * end.sin()).round() as i32,
        );
        SelectObject(dc, pen);
        SelectObject(dc, brush);
        let _ = DeleteObject(track.into());
        let _ = DeleteObject(active.into());
    }
}

fn spinner_dirty_rect(dpi: i32) -> RECT {
    RECT {
        left: 7 * dpi / 96,
        top: (STATUS_HEIGHT / 2 - 8) * dpi / 96,
        right: 23 * dpi / 96,
        bottom: (STATUS_HEIGHT / 2 + 8) * dpi / 96,
    }
}

#[cfg(test)]
pub(crate) fn paint_status_window_states(context: &ITfContext) -> Result<()> {
    // SAFETY: Isolated message-only window on the test COM apartment. It never
    // appears on the desktop or sends input; it exercises the real GDI painter.
    unsafe {
        let module = GetModuleHandleW(None)?;
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module.into(),
            lpszClassName: w!("Retype.Translation.Paint.Test"),
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("Retype.Translation.Paint.Test"),
            w!("paint test"),
            WS_OVERLAPPED,
            0,
            0,
            480,
            120,
            Some(HWND_MESSAGE),
            None,
            Some(module.into()),
            None,
        )?;
        let data = Frame {
            state: Weak::new(),
            context: context.clone(),
            epoch: 0,
            original: Vec::new(),
            selected: None,
            task: AssistTask::GrammarCorrect,
            response: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            translation: None,
            message: String::new(),
            scroll: 0,
            max_scroll: 0,
            owner: HWND::default(),
            applying: false,
            applied: false,
            busy: false,
            preview: false,
            spinner_phase: 0,
            hide_at: None,
        };
        let pointer = Box::into_raw(Box::new(data));
        #[cfg(target_pointer_width = "64")]
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as isize);
        #[cfg(target_pointer_width = "32")]
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as i32);
        let result = (|| -> Result<()> {
            let short = notice_width(hwnd, 96, "翻译中…", true);
            let long = notice_width(hwnd, 96, "输入框已切换或正在输入，原文保持不变", false);
            assert!(
                short < long,
                "Short notices should fit their text instead of occupying the full width"
            );
            for (message, text, height) in [
                ("翻译中…", None, STATUS_HEIGHT),
                ("读取失败", None, STATUS_HEIGHT),
                ("", Some(""), PREVIEW_HEIGHT),
                ("译文预览", Some("你好，这是一个翻译测试。"), PREVIEW_HEIGHT),
            ] {
                SetWindowPos(hwnd, None, 0, 0, 480, height, SWP_NOACTIVATE | SWP_NOZORDER)?;
                {
                    let data = frame(hwnd).ok_or(E_FAIL)?;
                    data.message = message.into();
                    data.translation = text.map(str::to_owned);
                    data.busy = message == "翻译中…";
                    data.preview = height == PREVIEW_HEIGHT;
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                paint(hwnd);
                if message == "翻译中…" {
                    // A lightweight notice has no close hit target at its right edge.
                    window_proc(hwnd, WM_LBUTTONUP, WPARAM(0), LPARAM((10 << 16) | 479));
                    assert!(frame(hwnd).is_some());
                    for phase in 1..=12 {
                        window_proc(hwnd, WM_TIMER, WPARAM(TIMER), LPARAM(0));
                        assert_eq!(frame(hwnd).ok_or(E_FAIL)?.spinner_phase, phase % 12);
                        // Message-only test windows have no visible update region.
                        // Verify the production dirty rectangle stays before the
                        // caption at common display scales while painting each phase.
                        for dpi in [96, 120, 144, 192] {
                            let updated = spinner_dirty_rect(dpi);
                            assert!(updated.right < 30 * dpi / 96);
                            assert!(updated.top >= 0);
                            assert!(updated.bottom <= STATUS_HEIGHT * dpi / 96);
                        }
                        paint(hwnd);
                    }
                }
            }
            // A stopped/error notice expires even when no worker response will
            // ever arrive (for example an empty field rejected during capture).
            {
                let data = frame(hwnd).ok_or(E_FAIL)?;
                data.message = "输入框为空".into();
                data.busy = false;
                data.applying = false;
                data.hide_at = Some(Instant::now() + ERROR_NOTICE_DURATION);
            }
            window_proc(hwnd, WM_TIMER, WPARAM(TIMER), LPARAM(0));
            assert!(
                IsWindow(Some(hwnd)).as_bool(),
                "Do not close before the deadline"
            );
            frame(hwnd).ok_or(E_FAIL)?.hide_at = Some(Instant::now() - Duration::from_millis(1));
            window_proc(hwnd, WM_TIMER, WPARAM(TIMER), LPARAM(0));
            assert!(
                !IsWindow(Some(hwnd)).as_bool(),
                "Expired errors must destroy the notice"
            );
            Ok(())
        })();
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        result
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: window messages execute on the owning TSF apartment.
        unsafe {
            match msg {
                WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
                WM_PAINT => {
                    paint(hwnd);
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_TIMER if wp.0 == TIMER => {
                    if frame(hwnd).is_some_and(|f| {
                        f.hide_at.is_some_and(|deadline| Instant::now() >= deadline)
                    }) {
                        let _ = DestroyWindow(hwnd);
                        return LRESULT(0);
                    }
                    let loading = frame(hwnd).is_some_and(|f| {
                        if f.busy || f.applying {
                            f.spinner_phase = (f.spinner_phase + 1) % 12;
                            true
                        } else {
                            false
                        }
                    });
                    if loading {
                        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                        let spinner = spinner_dirty_rect(dpi);
                        let _ = InvalidateRect(Some(hwnd), Some(&spinner), false);
                    }
                    let target = frame(hwnd).and_then(|f| {
                        if f.applied {
                            return None;
                        }
                        Some((
                            f.state.upgrade()?,
                            f.context.clone(),
                            f.epoch,
                            f.owner,
                            f.selected.clone()?,
                        ))
                    });
                    if target.is_some_and(|(state, context, epoch, owner, selected)| {
                        !selected.current() || !same_target(&state, &context, epoch, owner)
                    }) {
                        if let Some(f) = frame(hwnd) {
                            f.cancelled.store(true, Ordering::Release);
                            f.response = None;
                            f.selected = None;
                            f.preview = false;
                        }
                        status(hwnd, "选区已变化，请重新选中后重试");
                    }
                    let response = frame(hwnd).and_then(|f| f.response.as_ref()?.try_recv().ok());
                    if let Some(response) = response {
                        let apply = if let Some(f) = frame(hwnd) {
                            f.response = None;
                            f.busy = false;
                            match response {
                                Ok(Response::Assistance { text }) => {
                                    f.translation = Some(text);
                                    f.preview = true;
                                    f.message = match f.task {
                                        AssistTask::GrammarCorrect => {
                                            "语法预览 · Ctrl+Enter 应用 · Esc 取消"
                                        }
                                        AssistTask::CompleteSentence => {
                                            "补句预览 · Ctrl+Enter 应用 · Esc 取消"
                                        }
                                    }
                                    .into();
                                    false
                                }
                                Err(e) => {
                                    f.message = e;
                                    false
                                }
                                _ => {
                                    f.message = "翻译服务返回无效结果".into();
                                    false
                                }
                            }
                        } else {
                            false
                        };
                        if !apply {
                            let preview =
                                frame(hwnd).is_some_and(|f| f.preview && f.translation.is_some());
                            resize_notice(hwnd, preview);
                            if let Some(f) = frame(hwnd) {
                                f.hide_at =
                                    (!preview).then(|| Instant::now() + ERROR_NOTICE_DURATION);
                            }
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        if apply {
                            request_apply(hwnd);
                        }
                    }
                    LRESULT(0)
                }
                WM_MOUSEWHEEL => {
                    if let Some(f) = frame(hwnd) {
                        let delta = (wp.0 >> 16) as u16 as i16 as i32;
                        f.scroll = (f.scroll - delta / 120 * 48).clamp(0, f.max_scroll);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let x = (lp.0 & 0xffff) as u16 as i16 as i32;
                    let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32;
                    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                    if x >= rect.right - 38 * dpi / 96
                        && y < STATUS_HEIGHT * dpi / 96
                        && frame(hwnd).is_some_and(|f| f.preview && !f.applied)
                    {
                        let _ = DestroyWindow(hwnd);
                    } else if x >= 242 * dpi / 96
                        && y >= rect.bottom - 40 * dpi / 96
                        && frame(hwnd).is_some_and(|f| f.preview && !f.applied && !f.applying)
                    {
                        request_apply(hwnd);
                    } else if y >= rect.bottom - 40 * dpi / 96
                        && frame(hwnd)
                            .is_some_and(|f| f.preview && !f.applied && !f.applying && !f.busy)
                    {
                        if (12 * dpi / 96..112 * dpi / 96).contains(&x) {
                            start_task(hwnd, AssistTask::GrammarCorrect);
                        } else if (116 * dpi / 96..222 * dpi / 96).contains(&x) {
                            start_task(hwnd, AssistTask::CompleteSentence);
                        }
                    }
                    LRESULT(0)
                }
                WM_NCDESTROY => {
                    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Frame;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    if !pointer.is_null() {
                        let data = Box::from_raw(pointer);
                        if let Some(state) = data.state.upgrade() {
                            let mut window = lock(&state.translation_window);
                            if *window == Some(hwnd.0 as usize) {
                                *window = None;
                            }
                        }
                        drop(data);
                    }
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }));
    result.unwrap_or(LRESULT(0))
}
#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::{Com::CoTaskMemAlloc, Variant::VARIANT};
    #[implement(ITfInputScope)]
    struct DeclaredScopes(Vec<InputScope>);
    impl ITfInputScope_Impl for DeclaredScopes_Impl {
        fn GetInputScopes(&self, scopes: *mut *mut InputScope, count: *mut u32) -> Result<()> {
            if scopes.is_null() || count.is_null() {
                return Err(E_POINTER.into());
            }
            // SAFETY: caller supplies out parameters and releases the COM allocation.
            unsafe {
                *count = self.0.len() as u32;
                *scopes = std::ptr::null_mut();
                if !self.0.is_empty() {
                    *scopes = CoTaskMemAlloc(std::mem::size_of_val(self.0.as_slice())).cast();
                    if (*scopes).is_null() {
                        return Err(E_OUTOFMEMORY.into());
                    }
                    std::ptr::copy_nonoverlapping(self.0.as_ptr(), *scopes, self.0.len());
                }
            }
            Ok(())
        }
        fn GetPhrase(&self, _: *mut *mut windows_core::BSTR, _: *mut u32) -> Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn GetRegularExpression(&self) -> Result<windows_core::BSTR> {
            Err(E_NOTIMPL.into())
        }
        fn GetSRGS(&self) -> Result<windows_core::BSTR> {
            Err(E_NOTIMPL.into())
        }
        fn GetXML(&self) -> Result<windows_core::BSTR> {
            Err(E_NOTIMPL.into())
        }
    }
    #[test]
    fn scope_getter_contract_distinguishes_absence_errors_and_sensitive_arrays() -> Result<()> {
        let mut diagnostic = ScopeDiagnostic::default();
        for hr in [S_OK, S_FALSE] {
            assert_eq!(
                scope_value(hr, &VARIANT::default(), &mut diagnostic),
                ScopeValue::Absent
            );
        }
        for hr in [E_FAIL, E_NOTIMPL, E_INVALIDARG] {
            assert_eq!(
                scope_value(hr, &VARIANT::default(), &mut diagnostic),
                ScopeValue::Reject
            );
        }
        assert_eq!(
            scope_value(S_OK, &VARIANT::from(1i32), &mut diagnostic),
            ScopeValue::Reject
        );
        for scope in [
            IS_DEFAULT,
            IS_TEXT,
            IS_PASSWORD,
            IS_PRIVATE,
            IS_NUMERIC_PASSWORD,
            IS_NUMERIC_PIN,
            IS_ALPHANUMERIC_PIN,
            IS_ALPHANUMERIC_PIN_SET,
            InputScope(123456),
        ] {
            let object: ITfInputScope = DeclaredScopes(vec![IS_DEFAULT, scope]).into();
            let value = VARIANT::from(object.cast::<windows_core::IUnknown>()?);
            assert_eq!(
                scope_value(S_OK, &value, &mut diagnostic),
                if matches!(scope, IS_DEFAULT | IS_TEXT) {
                    ScopeValue::Public
                } else {
                    ScopeValue::Reject
                }
            );
            assert_eq!(
                scope_value(S_FALSE, &value, &mut diagnostic),
                ScopeValue::Reject
            );
        }
        let object: ITfInputScope = DeclaredScopes(Vec::new()).into();
        assert_eq!(
            scope_value(
                S_OK,
                &VARIANT::from(object.cast::<windows_core::IUnknown>()?),
                &mut diagnostic
            ),
            ScopeValue::Reject
        );
        Ok(())
    }
    #[test]
    fn scope_status_does_not_confuse_hidden_text_capability_with_field_privacy() {
        for static_flags in [0, TS_SS_NOHIDDENTEXT] {
            for dynamic_flags in [
                0,
                TS_SD_READONLY,
                TS_SD_LOADING,
                TS_SD_READONLY | TS_SD_LOADING,
            ] {
                assert_eq!(
                    scope_status_allows_editing(&TS_STATUS {
                        dwStaticFlags: static_flags,
                        dwDynamicFlags: dynamic_flags
                    }),
                    dynamic_flags == 0
                );
            }
        }
    }
    #[test]
    fn embedded_and_invalid_text_is_rejected() {
        assert!(validate_text(&[0xfffc]).is_err());
        assert!(validate_text(&[0xd800]).is_err());
        assert!(validate_text(&[]).is_err());
        assert!(validate_text(&"你好\nworld".encode_utf16().collect::<Vec<_>>()).is_ok());
    }
}
