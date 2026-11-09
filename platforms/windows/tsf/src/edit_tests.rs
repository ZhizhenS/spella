//! Real TSF integration tests with an in-memory ITextStoreACP host.
#![allow(
    unused_variables,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arc_with_non_send_sync
)]
use super::*;
use crate::tip::{lock, TipState};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;

use windows_core::{implement, Interface, Result, HRESULT};
#[derive(Default)]
struct Data {
    text: Vec<u16>,
    start: i32,
    end: i32,
    sink: Option<ITextStoreACPSink>,
    reject: bool,
    reject_write: bool,
    reject_selection_after: Option<u32>,
    defer: bool,
    hidden: bool,
    no_text_extent: bool,
    text_read_stop: Option<usize>,
    truncated_reads: usize,
    input_scope: Option<InputScope>,
    scope_segments: Vec<(i32, i32, Option<InputScope>)>,
    requested_scope_position: i32,
    scope_read_error: bool,
    scope_transition_error: bool,
    empty_scope_attribute: bool,
    scope_after_end: Option<InputScope>,
    requested_scope: bool,
    deferred_flags: Option<u32>,
    chromium: bool,
    composition_count: usize,
    committed_caret: Option<i32>,
}

#[implement(ITfInputScope)]
struct DeclaredScope(InputScope);
impl ITfInputScope_Impl for DeclaredScope_Impl {
    fn GetInputScopes(&self, scopes: *mut *mut InputScope, count: *mut u32) -> Result<()> {
        if scopes.is_null() || count.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: COM out parameters are checked, and the caller frees this CoTaskMem array.
        unsafe {
            let value = CoTaskMemAlloc(std::mem::size_of::<InputScope>()).cast::<InputScope>();
            if value.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            *value = self.0;
            *scopes = value;
            *count = 1;
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
fn declared_password_private_and_pin_scopes_never_learn() -> Result<()> {
    for scope in [
        IS_DEFAULT,
        IS_PASSWORD,
        IS_PRIVATE,
        IS_NUMERIC_PASSWORD,
        IS_NUMERIC_PIN,
        IS_ALPHANUMERIC_PIN,
        IS_ALPHANUMERIC_PIN_SET,
    ] {
        let object: ITfInputScope = DeclaredScope(scope).into();
        let value = VARIANT::from(object.cast::<windows_core::IUnknown>()?);
        assert_eq!(safe_input_scope(&value), scope == IS_DEFAULT);
    }
    assert!(safe_input_scope(&VARIANT::default()));
    assert!(!safe_input_scope(&VARIANT::from(1i32)));
    Ok(())
}
#[implement(ITextStoreACP, ITfContextOwnerCompositionSink)]
struct Store {
    data: Arc<Mutex<Data>>,
}
impl ITextStoreACP_Impl for Store_Impl {
    fn AdviseSink(
        &self,
        riid: *const windows_core::GUID,
        punk: windows_core::Ref<windows_core::IUnknown>,
        dwmask: u32,
    ) -> windows_core::Result<()> {
        lock(&self.data).sink = Some(punk.ok()?.cast()?);
        Ok(())
    }
    fn UnadviseSink(
        &self,
        punk: windows_core::Ref<windows_core::IUnknown>,
    ) -> windows_core::Result<()> {
        lock(&self.data).sink = None;
        Ok(())
    }
    fn RequestLock(&self, dwlockflags: u32) -> windows_core::Result<windows_core::HRESULT> {
        if lock(&self.data).reject {
            return Ok(E_ACCESSDENIED);
        }
        if lock(&self.data).defer {
            lock(&self.data).deferred_flags = Some(dwlockflags);
            return Ok(TS_S_ASYNC);
        }
        let sink = lock(&self.data).sink.clone().ok_or(E_FAIL)?;
        let initial_selection = {
            let mut host = lock(&self.data);
            host.committed_caret = None;
            (host.start, host.end)
        };
        unsafe {
            sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(dwlockflags))?;
        }
        {
            let mut host = lock(&self.data);
            if host.chromium && host.composition_count == 0 {
                // Chromium's CommitTextAndEndCompositionIfAny calls InsertText
                // with kMoveCursorAfterText, ignoring a collapsed SetSelection
                // during the same TSF lock. Selection-only IME edits are ignored too.
                if let Some(end) = host.committed_caret.take() {
                    host.start = end;
                    host.end = end;
                } else {
                    (host.start, host.end) = initial_selection;
                }
            }
            // The test transport forwards arrows only after the host commit;
            // never inject OS input into the user's active application in tests.
            for direction in crate::symbol_cursor::take_forwarded() {
                let step = match direction {
                    crate::symbol_cursor::Direction::Left => -1,
                    crate::symbol_cursor::Direction::Right => 1,
                };
                host.start = (host.start + step).clamp(0, host.text.len() as i32);
                host.end = host.start;
            }
        }
        Ok(HRESULT(0))
    }
    fn GetStatus(&self) -> windows_core::Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: if lock(&self.data).hidden {
                0
            } else {
                TS_SS_NOHIDDENTEXT
            },
        })
    }
    fn QueryInsert(
        &self,
        acpteststart: i32,
        acptestend: i32,
        cch: u32,
        pacpresultstart: *mut i32,
        pacpresultend: *mut i32,
    ) -> windows_core::Result<()> {
        unsafe {
            *pacpresultstart = acpteststart;
            *pacpresultend = acptestend;
        }
        Ok(())
    }
    fn GetSelection(
        &self,
        ulindex: u32,
        ulcount: u32,
        pselection: *mut TS_SELECTION_ACP,
        pcfetched: *mut u32,
    ) -> windows_core::Result<()> {
        let d = lock(&self.data);
        unsafe {
            *pselection = TS_SELECTION_ACP {
                acpStart: d.start,
                acpEnd: d.end,
                style: TS_SELECTIONSTYLE {
                    ase: TS_AE_END,
                    fInterimChar: BOOL(0),
                },
            };
            *pcfetched = 1;
        }
        Ok(())
    }
    fn SetSelection(
        &self,
        ulcount: u32,
        pselection: *const TS_SELECTION_ACP,
    ) -> windows_core::Result<()> {
        let mut d = lock(&self.data);
        if let Some(remaining) = d.reject_selection_after.as_mut() {
            if *remaining == 0 {
                d.reject_selection_after = None;
                return Err(E_ACCESSDENIED.into());
            }
            *remaining -= 1;
        }
        unsafe {
            d.start = (*pselection).acpStart;
            d.end = (*pselection).acpEnd;
        }
        Ok(())
    }
    fn GetText(
        &self,
        acpstart: i32,
        acpend: i32,
        pchplain: windows_core::PWSTR,
        cchplainreq: u32,
        pcchplainret: *mut u32,
        prgruninfo: *mut TS_RUNINFO,
        cruninforeq: u32,
        pcruninforet: *mut u32,
        pacpnext: *mut i32,
    ) -> windows_core::Result<()> {
        let mut d = lock(&self.data);
        let end = if acpend < 0 {
            d.text.len()
        } else {
            acpend as usize
        };
        let start = acpstart as usize;
        if start > end || end > d.text.len() {
            return Err(E_INVALIDARG.into());
        }
        // Simulate a host that returns a valid prefix, then a successful empty
        // read although its selection/document anchors still include more text.
        let readable_end = end.min(d.text_read_stop.unwrap_or(end));
        let available = readable_end.saturating_sub(start);
        let n = available.min(cchplainreq as usize);
        // Run-only requests still describe the available text. An empty read
        // has no TS_RUNINFO record and acpNext remains at the unread position.
        let referenced = if cchplainreq > 0 {
            n
        } else if cruninforeq > 0 {
            available
        } else {
            0
        };
        if d.text_read_stop.is_some() {
            d.truncated_reads += 1;
        }
        unsafe {
            if n > 0 {
                std::ptr::copy_nonoverlapping(d.text[start..].as_ptr(), pchplain.0, n);
            }
            if !pcchplainret.is_null() {
                *pcchplainret = n as u32;
            }
            if !pacpnext.is_null() {
                *pacpnext = (start + referenced) as i32;
            }
            if !pcruninforet.is_null() {
                *pcruninforet = if cruninforeq > 0 && referenced > 0 {
                    1
                } else {
                    0
                };
            }
            if cruninforeq > 0 && referenced > 0 {
                *prgruninfo = TS_RUNINFO {
                    uCount: referenced as u32,
                    r#type: TS_RT_PLAIN,
                };
            }
        }
        Ok(())
    }
    fn SetText(
        &self,
        dwflags: u32,
        acpstart: i32,
        acpend: i32,
        pchtext: &windows_core::PCWSTR,
        cch: u32,
    ) -> windows_core::Result<TS_TEXTCHANGE> {
        let mut d = lock(&self.data);
        if d.reject_write {
            return Err(E_ACCESSDENIED.into());
        }
        let text = if cch == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(pchtext.0, cch as usize) }
        };
        d.text
            .splice(acpstart as usize..acpend as usize, text.iter().copied());
        d.start = acpstart + cch as i32;
        d.end = d.start;
        d.committed_caret = Some(d.end);
        Ok(TS_TEXTCHANGE {
            acpStart: acpstart,
            acpOldEnd: acpend,
            acpNewEnd: d.end,
        })
    }
    fn GetFormattedText(
        &self,
        acpstart: i32,
        acpend: i32,
    ) -> windows_core::Result<windows::Win32::System::Com::IDataObject> {
        Err(E_NOTIMPL.into())
    }
    fn GetEmbedded(
        &self,
        acppos: i32,
        rguidservice: *const windows_core::GUID,
        riid: *const windows_core::GUID,
    ) -> windows_core::Result<windows_core::IUnknown> {
        Err(E_NOTIMPL.into())
    }
    fn QueryInsertEmbedded(
        &self,
        pguidservice: *const windows_core::GUID,
        pformatetc: *const windows::Win32::System::Com::FORMATETC,
    ) -> windows_core::Result<windows_core::BOOL> {
        Ok(false.into())
    }
    fn InsertEmbedded(
        &self,
        dwflags: u32,
        acpstart: i32,
        acpend: i32,
        pdataobject: windows_core::Ref<windows::Win32::System::Com::IDataObject>,
    ) -> windows_core::Result<TS_TEXTCHANGE> {
        Err(E_NOTIMPL.into())
    }
    fn InsertTextAtSelection(
        &self,
        dwflags: u32,
        pchtext: &windows_core::PCWSTR,
        cch: u32,
        pacpstart: *mut i32,
        pacpend: *mut i32,
        pchange: *mut TS_TEXTCHANGE,
    ) -> windows_core::Result<()> {
        let (start, end) = {
            let d = lock(&self.data);
            (d.start, d.end)
        };
        unsafe {
            if !pacpstart.is_null() {
                *pacpstart = start;
            }
            if !pacpend.is_null() {
                *pacpend = end;
            }
        }
        if dwflags & TS_IAS_QUERYONLY != 0 {
            return Ok(());
        }
        let change = self.SetText(0, start, end, pchtext, cch)?;
        unsafe {
            if !pchange.is_null() {
                *pchange = change;
            }
        }
        Ok(())
    }
    fn InsertEmbeddedAtSelection(
        &self,
        dwflags: u32,
        pdataobject: windows_core::Ref<windows::Win32::System::Com::IDataObject>,
        pacpstart: *mut i32,
        pacpend: *mut i32,
        pchange: *mut TS_TEXTCHANGE,
    ) -> windows_core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn RequestSupportedAttrs(
        &self,
        dwflags: u32,
        cfilterattrs: u32,
        pafilterattrs: *const windows_core::GUID,
    ) -> windows_core::Result<()> {
        lock(&self.data).requested_scope = cfilterattrs > 0
            && !pafilterattrs.is_null()
            && unsafe { std::slice::from_raw_parts(pafilterattrs, cfilterattrs as usize) }
                .contains(&GUID_PROP_INPUTSCOPE);
        Ok(())
    }
    fn RequestAttrsAtPosition(
        &self,
        acppos: i32,
        cfilterattrs: u32,
        pafilterattrs: *const windows_core::GUID,
        dwflags: u32,
    ) -> windows_core::Result<()> {
        lock(&self.data).requested_scope_position = acppos;
        self.RequestSupportedAttrs(dwflags, cfilterattrs, pafilterattrs)
    }
    fn RequestAttrsTransitioningAtPosition(
        &self,
        acppos: i32,
        cfilterattrs: u32,
        pafilterattrs: *const windows_core::GUID,
        dwflags: u32,
    ) -> windows_core::Result<()> {
        self.RequestSupportedAttrs(dwflags, cfilterattrs, pafilterattrs)
    }
    fn FindNextAttrTransition(
        &self,
        acpstart: i32,
        acphalt: i32,
        cfilterattrs: u32,
        pafilterattrs: *const windows_core::GUID,
        dwflags: u32,
        pacpnext: *mut i32,
        pffound: *mut windows_core::BOOL,
        plfoundoffset: *mut i32,
    ) -> windows_core::Result<()> {
        let data = lock(&self.data);
        if data.scope_transition_error {
            return Err(E_FAIL.into());
        }
        let halt = if acphalt < 0 {
            data.text.len() as i32
        } else {
            acphalt
        };
        let next = data
            .scope_segments
            .iter()
            .flat_map(|(start, end, _)| [*start, *end])
            .filter(|position| *position > acpstart && *position < halt)
            .min();
        unsafe {
            *pacpnext = next.unwrap_or(halt);
            *pffound = BOOL::from(next.is_some());
            *plfoundoffset = 0;
        }
        Ok(())
    }
    fn RetrieveRequestedAttrs(
        &self,
        ulcount: u32,
        paattrvals: *mut TS_ATTRVAL,
        pcfetched: *mut u32,
    ) -> windows_core::Result<()> {
        let (scope, explicit_empty) = {
            let mut data = lock(&self.data);
            if data.scope_read_error {
                return Err(E_FAIL.into());
            }
            let requested = std::mem::take(&mut data.requested_scope);
            let scope = if data.scope_segments.is_empty() {
                data.input_scope
            } else {
                data.scope_segments
                    .iter()
                    .find(|(start, end, _)| {
                        *start <= data.requested_scope_position
                            && data.requested_scope_position < *end
                    })
                    .and_then(|(_, _, scope)| *scope)
            };
            (
                scope.filter(|_| requested),
                requested && data.empty_scope_attribute,
            )
        };
        unsafe {
            *pcfetched = 0;
            if let Some(scope) = scope.filter(|_| ulcount > 0) {
                let object: ITfInputScope = DeclaredScope(scope).into();
                *paattrvals = TS_ATTRVAL {
                    idAttr: GUID_PROP_INPUTSCOPE,
                    dwOverlapId: 0,
                    varValue: VARIANT::from(object.cast::<windows_core::IUnknown>()?),
                };
                *pcfetched = 1;
            } else if explicit_empty && ulcount > 0 {
                *paattrvals = TS_ATTRVAL {
                    idAttr: GUID_PROP_INPUTSCOPE,
                    dwOverlapId: 0,
                    varValue: VARIANT::default(),
                };
                *pcfetched = 1;
            }
        }
        Ok(())
    }
    fn GetEndACP(&self) -> windows_core::Result<i32> {
        Ok(lock(&self.data).text.len() as i32)
    }
    fn GetActiveView(&self) -> windows_core::Result<u32> {
        Ok(0)
    }
    fn GetACPFromPoint(
        &self,
        vcview: u32,
        ptscreen: *const windows::Win32::Foundation::POINT,
        dwflags: u32,
    ) -> windows_core::Result<i32> {
        Err(E_NOTIMPL.into())
    }
    fn GetTextExt(
        &self,
        vcview: u32,
        acpstart: i32,
        acpend: i32,
        prc: *mut windows::Win32::Foundation::RECT,
        pfclipped: *mut windows_core::BOOL,
    ) -> windows_core::Result<()> {
        if lock(&self.data).no_text_extent {
            return Err(E_NOTIMPL.into());
        }
        unsafe {
            *prc = RECT {
                left: 10 + acpstart * 8,
                top: 10,
                right: 10 + acpend * 8,
                bottom: 30,
            };
            *pfclipped = BOOL(0);
        }
        Ok(())
    }
    fn GetScreenExt(&self, vcview: u32) -> windows_core::Result<windows::Win32::Foundation::RECT> {
        Ok(RECT {
            left: 0,
            top: 0,
            right: 500,
            bottom: 500,
        })
    }
    fn GetWnd(&self, vcview: u32) -> windows_core::Result<windows::Win32::Foundation::HWND> {
        Ok(HWND::default())
    }
}

impl ITfContextOwnerCompositionSink_Impl for Store_Impl {
    fn OnStartComposition(&self, _: Ref<'_, ITfCompositionView>) -> Result<BOOL> {
        lock(&self.data).composition_count += 1;
        Ok(true.into())
    }
    fn OnUpdateComposition(
        &self,
        _: Ref<'_, ITfCompositionView>,
        _: Ref<'_, ITfRange>,
    ) -> Result<()> {
        Ok(())
    }
    fn OnEndComposition(&self, _: Ref<'_, ITfCompositionView>) -> Result<()> {
        let mut data = lock(&self.data);
        data.composition_count = data.composition_count.saturating_sub(1);
        if let Some(scope) = data.scope_after_end.take() {
            data.input_scope = Some(scope);
        }
        Ok(())
    }
}

#[implement(ITfUIElementSink)]
struct UiLessSink {
    manager: ITfUIElementMgr,
    candidates: Arc<Mutex<Vec<String>>>,
}
impl ITfUIElementSink_Impl for UiLessSink_Impl {
    fn BeginUIElement(&self, _id: u32, show: *mut BOOL) -> Result<()> {
        unsafe { *show = BOOL(0) };
        Ok(())
    }

    fn UpdateUIElement(&self, id: u32) -> Result<()> {
        let ui = unsafe { self.manager.GetUIElement(id)? };
        let list: ITfCandidateListUIElementBehavior = ui.cast()?;
        let integrated: ITfIntegratableCandidateListUIElement = ui.cast()?;
        assert_eq!(
            unsafe { integrated.GetSelectionStyle()? },
            STYLE_ACTIVE_SELECTION
        );
        let flags = unsafe { list.GetUpdatedFlags()? };
        if flags & (TF_CLUIE_COUNT | TF_CLUIE_STRING) != 0 {
            // Search/UI-less hosts may enumerate every candidate, not just the
            // visible page. Exercise that path against the production interface.
            let count = unsafe { list.GetCount()? };
            let mut words = Vec::with_capacity(count as usize);
            for index in 0..count {
                words.push(unsafe { list.GetString(index)? }.to_string());
            }
            lock(&self.candidates).extend(words);
        }
        Ok(())
    }

    fn EndUIElement(&self, _id: u32) -> Result<()> {
        Ok(())
    }
}
#[test]
fn real_tsf_composition_commit_cancel_and_passthrough() -> Result<()> {
    // SAFETY: Isolated COM apartment and text store; no system registration or desktop input changes.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let result = run_host();
        CoUninitialize();
        result
    }
}

#[test]
fn whole_word_selections_learn_in_normal_fields_with_or_without_hidden_text_support() -> Result<()>
{
    std::thread::spawn(|| -> Result<()> {
        // SAFETY: Isolated COM apartment and text store; no real input or user database.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let document = manager.CreateDocumentMgr()?;
                let data = Arc::new(Mutex::new(Data::default()));
                let store: ITextStoreACP = Store {
                    data: Arc::clone(&data),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                document.Push(&context)?;
                let state = TipState::new();
                state.tid.store(tid, Ordering::SeqCst);
                state.activated.store(true, Ordering::SeqCst);
                *lock(&state.thread_mgr) = Some(manager.clone());
                let session = crate::session::Session::start_with("Z:/retype-test/no-dict".into());
                let deadline = Instant::now() + std::time::Duration::from_secs(10);
                while !session.dict_ready() && Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                let (dict, _) = retype_dict::from_pairs([
                    ("啦去", "la qu", 10000.),
                    ("拉取", "la qu", 1347.),
                    ("拉", "la", 100.),
                    ("取", "qu", 100.),
                ]);
                session.dict.install(Arc::new(dict));
                session
                    .packs
                    .install(Arc::new(retype_dict::LayeredDict::new()));
                session.submit(InputEvent::SetPinyinScheme(
                    retype_types::PinyinScheme::Full,
                ));
                session.submit(InputEvent::SetEnglishOptions {
                    enabled: false,
                    spelling: false,
                });
                if !session.backend.with_kernel(|k| k.is_chinese()) {
                    session.submit(InputEvent::ToggleChinese);
                }
                *lock(&state.session) = Some(Arc::clone(&session));
                let choose = |number_key: bool| -> Result<()> {
                    for ch in "laqu".chars() {
                        request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
                    }
                    let render = session.backend.with_kernel(|k| k.render_state());
                    let index = render
                        .candidates
                        .iter()
                        .position(|c| c.text == "拉取")
                        .expect("whole word available");
                    assert!(index < 8);
                    let work = if number_key {
                        Work::Key(Key::Char((b'1' + index as u8) as char), Modifiers::NONE)
                    } else {
                        Work::Choose(index, render.gen)
                    };
                    request(&state, &context, work)?;
                    assert!(lock(&state.composition).is_none());
                    assert!(String::from_utf16_lossy(&lock(&data).text).ends_with("拉取"));
                    Ok(())
                };
                let count = || {
                    session
                        .user
                        .usage_snapshot()
                        .records
                        .iter()
                        .filter(|r| r.text == "拉取")
                        .map(|r| r.count)
                        .sum::<u64>()
                };
                // The host's capability flag is independent of this field's privacy.
                let mut expected = 0;
                for supports_hidden in [false, true] {
                    for scope in [Some(IS_DEFAULT), None] {
                        lock(&data).hidden = supports_hidden;
                        for choice in 0..5 {
                            lock(&data).input_scope = scope;
                            // Ending a composition can expose the next field's scope.
                            lock(&data).scope_after_end = Some(IS_PRIVATE);
                            choose(choice % 2 == 0)?;
                        }
                        expected += 5;
                        let deadline = Instant::now() + std::time::Duration::from_secs(2);
                        while count() < expected && Instant::now() < deadline {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        assert_eq!(
                            count(),
                            expected,
                            "supports_hidden={supports_hidden}, scope={scope:?}"
                        );
                        assert_eq!(session.user.usage_snapshot().tick, expected);
                    }
                }
                for scope in [
                    IS_PASSWORD,
                    IS_PRIVATE,
                    IS_NUMERIC_PASSWORD,
                    IS_NUMERIC_PIN,
                    IS_ALPHANUMERIC_PIN,
                    IS_ALPHANUMERIC_PIN_SET,
                ] {
                    lock(&data).input_scope = Some(scope);
                    // A normal caret after commit must not authorize a private source.
                    lock(&data).scope_after_end = Some(IS_DEFAULT);
                    choose(false)?;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
                assert_eq!(
                    count(),
                    expected,
                    "private fields must never add learning events"
                );
                state.hide();
                document.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}

/// Exercise the key sink, not only Work::Key, so a broad capability guard cannot
/// silently bypass valid English input before the edit-session privacy check.
#[test]
fn english_key_sink_composes_in_rich_text_and_keeps_private_fields_literal() -> Result<()> {
    std::thread::spawn(|| -> Result<()> {
        // SAFETY: Isolated STA and in-memory text store; no keys are injected into
        // user applications and no installed profile or user registry is changed.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let document = manager.CreateDocumentMgr()?;
                let data = Arc::new(Mutex::new(Data {
                    hidden: true, // Supports hidden text; this is NOT a private-field declaration.
                    no_text_extent: true,
                    input_scope: Some(IS_DEFAULT),
                    ..Default::default()
                }));
                let store: ITextStoreACP = Store {
                    data: Arc::clone(&data),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                document.Push(&context)?;
                manager.SetFocus(&document)?;
                assert_eq!(context.GetStatus()?.dwStaticFlags & TS_SS_NOHIDDENTEXT, 0);

                let state = TipState::new();
                state.tid.store(tid, Ordering::SeqCst);
                state.activated.store(true, Ordering::SeqCst);
                state.set_shortcuts_for_test(retype_ai::config::Shortcuts::default());
                *lock(&state.thread_mgr) = Some(manager.clone());
                let session = crate::session::Session::start_with("Z:/retype-test/no-dict".into());
                session.submit(InputEvent::SetEnglishOptions {
                    enabled: true,
                    spelling: true,
                });
                *lock(&state.session) = Some(Arc::clone(&session));
                crate::profiles::apply(&state, false)?;
                let keys: ITfKeyEventSink = crate::tip::KeyEventSink {
                    state: Arc::downgrade(&state),
                }
                .into();
                let lp = windows::Win32::Foundation::LPARAM(0);

                for vk in ['H', 'E', 'L'] {
                    let key = windows::Win32::Foundation::WPARAM(vk as usize);
                    assert!(keys.OnTestKeyDown(&context, key, lp)?.as_bool());
                    assert!(keys.OnKeyDown(&context, key, lp)?.as_bool());
                }
                assert_eq!(
                    session
                        .backend
                        .with_kernel(|k| k.composition_text())
                        .to_ascii_lowercase(),
                    "hel"
                );
                assert!(session.backend.with_kernel(|k| k
                    .render_state()
                    .candidates
                    .iter()
                    .any(|candidate| candidate.text.eq_ignore_ascii_case("hello"))));
                assert!(lock(&state.composition).is_some());
                request(&state, &context, Work::Finish(false))?;
                assert_eq!(
                    String::from_utf16_lossy(&lock(&data).text).to_ascii_lowercase(),
                    "hel"
                );

                for scope in [IS_PASSWORD, IS_PRIVATE, IS_NUMERIC_PIN] {
                    lock(&data).input_scope = Some(scope);
                    let before = lock(&data).text.len();
                    let key = windows::Win32::Foundation::WPARAM('S' as usize);
                    // TestKey may claim the letter before a read lock exists.
                    // The write session still checks scope, inserts it literally
                    // and never builds a candidate or learns the private text.
                    assert!(keys.OnTestKeyDown(&context, key, lp)?.as_bool());
                    assert!(keys.OnKeyDown(&context, key, lp)?.as_bool());
                    assert_eq!(lock(&data).text.len(), before + 1);
                    assert!(String::from_utf16_lossy(&lock(&data).text)
                        .to_ascii_lowercase()
                        .ends_with('s'));
                    assert!(!session
                        .backend
                        .with_kernel(|k| k.has_composition() || k.has_candidates()));
                    assert!(lock(&state.composition).is_none());
                    assert!(
                        !keys
                            .OnTestKeyDown(&context, windows::Win32::Foundation::WPARAM(0x08), lp)?
                            .as_bool(),
                        "private-field Backspace remains native"
                    );
                }

                // A host refusing the edit must receive the real key itself.
                lock(&data).input_scope = Some(IS_DEFAULT);
                lock(&data).reject = true;
                let before = lock(&data).text.clone();
                let letter = windows::Win32::Foundation::WPARAM('H' as usize);
                assert!(keys.OnTestKeyDown(&context, letter, lp)?.as_bool());
                assert!(!keys.OnKeyDown(&context, letter, lp)?.as_bool());
                assert_eq!(lock(&data).text, before);
                assert!(!session
                    .backend
                    .with_kernel(|k| k.has_composition() || k.has_candidates()));
                lock(&data).reject = false;

                // Blur/Finish must not bypass the no-learning policy that explicit
                // commits enforce. Three finishes would promote this novel spelling
                // if the old direct record_learning path were still present.
                let novel = "qzxvfixture";
                assert!(!session
                    .backend
                    .with_kernel(|k| k.english_dictionary().contains_word(novel)));
                for _ in 0..3 {
                    for ch in novel.chars() {
                        request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
                    }
                    request(&state, &context, Work::Finish(false))?;
                }
                for ch in novel.chars() {
                    request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
                }
                assert!(
                    !session.backend.with_kernel(|k| k
                        .render_state()
                        .candidates
                        .iter()
                        .any(|candidate| candidate.text == novel)),
                    "finishing a word must not add it to personal suggestions"
                );
                request(&state, &context, Work::Finish(false))?;
                state.deactivate()?;
                document.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}

type TranslationReadResult = std::result::Result<Vec<u16>, String>;
#[implement(ITfEditSession)]
struct TranslationProbe {
    context: ITfContext,
    expected: Option<Vec<u16>>,
    output: Arc<Mutex<Option<TranslationReadResult>>>,
}
impl ITfEditSession_Impl for TranslationProbe_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let result = if let Some(expected) = &self.expected {
            crate::translation::replace_verified(
                &self.context,
                ec,
                expected,
                "完整译文\ntranslated",
            )
            .and_then(|_| crate::translation::read(&self.context, ec))
        } else {
            crate::translation::read(&self.context, ec)
        };
        *lock(&self.output) = Some(result);
        Ok(())
    }
}
#[test]
fn translation_reads_all_text_and_refuses_stale_or_rejected_replacements() -> Result<()> {
    let thread = std::thread::spawn(|| -> Result<()> {
        // SAFETY: isolated COM text store, never installs or reads a real application.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let outcome = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let document = manager.CreateDocumentMgr()?;
                let original: Vec<u16> =
                    "你好🙂\nA paragraph ".repeat(400).encode_utf16().collect();
                let data = Arc::new(Mutex::new(Data {
                    text: original.clone(),
                    start: 3,
                    end: 3,
                    ..Default::default()
                }));
                let store: ITextStoreACP = Store {
                    data: Arc::clone(&data),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                document.Push(&context)?;
                crate::translation::paint_status_window_states(&context)?;
                let probe =
                    |expected: Option<Vec<u16>>| -> Result<std::result::Result<Vec<u16>, String>> {
                        let output = Arc::new(Mutex::new(None));
                        let session: ITfEditSession = TranslationProbe {
                            context: context.clone(),
                            expected,
                            output: Arc::clone(&output),
                        }
                        .into();
                        context
                            .RequestEditSession(tid, &session, TF_ES_SYNC | TF_ES_READWRITE)?
                            .ok()?;
                        let result = lock(&output).take().ok_or(E_FAIL)?;
                        Ok(result)
                    };
                assert_eq!(probe(None)?.as_ref().ok(), Some(&original));
                let wrong: Vec<u16> = "older text".encode_utf16().collect();
                assert!(probe(Some(wrong))?.is_err());
                assert_eq!(lock(&data).text, original);
                lock(&data).reject_write = true;
                assert!(probe(Some(original.clone()))?.is_err());
                assert_eq!(lock(&data).text, original);
                lock(&data).reject_write = false;
                let translated: Vec<u16> = "完整译文\ntranslated".encode_utf16().collect();
                assert_eq!(probe(Some(original))?.as_ref().ok(), Some(&translated));
                assert_eq!(lock(&data).text, translated);
                lock(&data).hidden = true;
                // Absence of NOHIDDENTEXT means the host supports hidden text,
                // not that this ordinary text is private or unreadable.
                assert_eq!(probe(None)?.as_ref().ok(), Some(&translated));
                for scope in [IS_PASSWORD, IS_PRIVATE, IS_NUMERIC_PIN] {
                    lock(&data).input_scope = Some(scope);
                    assert!(probe(None)?.is_err());
                    assert_eq!(lock(&data).text, translated);
                }
                lock(&data).input_scope = Some(IS_DEFAULT);
                assert_eq!(probe(None)?.as_ref().ok(), Some(&translated));
                document.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            outcome
        }
    });
    thread.join().map_err(|_| E_FAIL)?
}
fn run_host() -> Result<()> {
    unsafe {
        let manager: ITfThreadMgr =
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
        let tid = manager.Activate()?;
        let document = manager.CreateDocumentMgr()?;
        let data = Arc::new(Mutex::new(Data {
            input_scope: Some(IS_DEFAULT),
            ..Default::default()
        }));
        let store: ITextStoreACP = Store {
            data: Arc::clone(&data),
        }
        .into();
        let mut context = None;
        let mut cookie = 0;
        document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
        let context = context.ok_or(E_FAIL)?;
        document.Push(&context)?;
        manager.SetFocus(&document)?;
        let seen_candidates = Arc::new(Mutex::new(Vec::new()));
        let ui_manager: ITfUIElementMgr = manager.cast()?;
        let ui_sink: ITfUIElementSink = UiLessSink {
            manager: ui_manager,
            candidates: Arc::clone(&seen_candidates),
        }
        .into();
        let source: ITfSource = manager.cast()?;
        let _ui_cookie = source.AdviseSink(&ITfUIElementSink::IID, &ui_sink)?;
        lock(&data).no_text_extent = true;
        let state = TipState::new();
        state.tid.store(tid, Ordering::SeqCst);
        state.activated.store(true, Ordering::SeqCst);
        *lock(&state.thread_mgr) = Some(manager.clone());
        check_candidate_getters(&state, &manager, &context)?;
        // Known deterministic dictionary; no background loader races.
        let session =
            crate::session::Session::start_with(std::path::PathBuf::from("Z:/retype-test/no-dict"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !session.dict_ready() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (dict, _) = retype_dict::from_pairs([
            ("脚", "jiao", 100.0),
            ("脚注", "jiao zhu", 1000.0),
            ("你好", "ni hao", 10000.0),
            ("你", "ni", 100.0),
            ("好", "hao", 100.0),
        ]);
        session.dict.install(Arc::new(dict));
        // Optional packs are themselves layered, including when none are enabled.
        session
            .packs
            .install(Arc::new(retype_dict::LayeredDict::new()));
        session.user.boost(
            &retype_dict::annotate::parse_pinyin("jiao zhu").unwrap(),
            "脚注",
            1.0,
        );
        session.submit(InputEvent::SetPinyinScheme(
            retype_types::PinyinScheme::Flypy,
        ));
        *lock(&state.session) = Some(Arc::clone(&session));
        // A newly focused host can restore a closed keyboard compartment after
        // activation. Receiving key focus must make the selected entry usable.
        let bridge = crate::langbar::ModeBridge::attach(&state, &manager)?;
        let compartment = bridge.compartment.clone();
        *lock(&state.mode_bridge) = Some(bridge);
        compartment.SetValue(tid, &VARIANT::from(0i32))?;
        assert!(!state.wants(Key::Char('n'), Modifiers::NONE));
        let keys: ITfKeyEventSink = crate::tip::KeyEventSink {
            state: Arc::downgrade(&state),
        }
        .into();
        keys.OnSetFocus(true)?;
        assert!(
            state.wants(Key::Char('n'), Modifiers::NONE),
            "first focus must accept Chinese input without switching the input method"
        );
        // Focus reconciliation preserves a deliberate Shift-to-ASCII choice.
        session.submit(InputEvent::ToggleChinese);
        keys.OnSetFocus(true)?;
        assert!(!session.backend.with_kernel(|k| k.is_chinese()));
        session.submit(InputEvent::ToggleChinese);
        session.submit(InputEvent::SetPinyinScheme(
            retype_types::PinyinScheme::Full,
        ));
        for ch in "NIHAO".chars() {
            let key = windows::Win32::Foundation::WPARAM(ch as usize);
            assert!(keys
                .OnTestKeyDown(&context, key, windows::Win32::Foundation::LPARAM(0))?
                .as_bool());
            assert!(keys
                .OnKeyDown(&context, key, windows::Win32::Foundation::LPARAM(0))?
                .as_bool());
        }
        assert!(lock(&seen_candidates).iter().any(|word| word == "你好"));
        request(&state, &context, Work::Key(Key::Escape, Modifiers::NONE))?;
        session.submit(InputEvent::SetPinyinScheme(
            retype_types::PinyinScheme::Flypy,
        ));
        for (index, ch) in "jnvu".chars().enumerate() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
            assert_eq!(
                String::from_utf16_lossy(&lock(&data).text),
                &"jnvu"[..index + 1],
                "every Flypy keystroke must reach the host document"
            );
        }
        assert!(lock(&seen_candidates).iter().any(|word| word == "脚注"));
        request(&state, &context, Work::Key(Key::Escape, Modifiers::NONE))?;
        session.submit(InputEvent::SetPinyinScheme(
            retype_types::PinyinScheme::Full,
        ));
        *lock(&state.session) = Some(session);
        let _ = stats::take_test_input();
        for ch in "nihao".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        request(&state, &context, Work::Key(Key::Backspace, Modifiers::NONE))?;
        request(&state, &context, Work::Key(Key::Char('o'), Modifiers::NONE))?;
        assert_eq!(stats::take_test_input().total(true).inputs, 0);
        assert!(lock(&seen_candidates).iter().any(|word| word == "你好"));
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "nihao");
        assert!(lock(&state.composition).is_some());
        let candidate_list = lock(&state.window)
            .as_ref()
            .and_then(crate::candidate::CandidateWindow::element_for_test)
            .ok_or(E_FAIL)?;
        let integrated: ITfIntegratableCandidateListUIElement = candidate_list.cast()?;
        assert!(integrated.ShowCandidateNumbers()?.as_bool());
        candidate_list.SetSelection(0)?;
        // Finalize synchronously edits and replaces candidate data. Getter
        // read locks must have been released before this host reentry.
        candidate_list.Finalize()?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好");
        assert!(lock(&state.composition).is_none());
        let measured = stats::take_test_input();
        assert_eq!(
            (
                measured.total(true).inputs,
                measured.total(true).corrected,
                measured.total(true).units
            ),
            (1, 1, 2)
        );
        // Deferred writes must not touch the engine/document before the host grants its lock.
        lock(&data).defer = true;
        request(&state, &context, Work::Key(Key::Char('n'), Modifiers::NONE))?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好");
        assert!(!state
            .session()
            .unwrap()
            .backend
            .with_kernel(|k| k.has_composition()));
        let flags = lock(&data).deferred_flags.take().expect("deferred lock");
        lock(&data).defer = false;
        let sink = lock(&data).sink.clone().expect("advised sink");
        sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags))?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好n");
        request(&state, &context, Work::Key(Key::Escape, Modifiers::NONE))?;
        assert_eq!(state.pending.load(Ordering::SeqCst), 0);
        // Focus loss invalidates a deferred key before it can edit the old document.
        lock(&data).defer = true;
        request(&state, &context, Work::Key(Key::Char('n'), Modifiers::NONE))?;
        state.epoch.fetch_add(1, Ordering::SeqCst);
        let flags = lock(&data).deferred_flags.take().expect("deferred lock");
        lock(&data).defer = false;
        sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags))?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好");
        assert_eq!(state.pending.load(Ordering::SeqCst), 0);
        // A rejected edit request must leave both the document and engine untouched.
        lock(&data).reject = true;
        assert!(request(&state, &context, Work::Key(Key::Char('a'), Modifiers::NONE)).is_err());
        assert_eq!(state.pending.load(Ordering::SeqCst), 0);
        assert!(!state
            .session()
            .unwrap()
            .backend
            .with_kernel(|k| k.has_composition()));
        lock(&data).reject = false;
        for ch in "ni".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        // A host can queue several caret-layout notifications while its read
        // lock is pending. They need one refresh, not multiple full drawings.
        lock(&data).defer = true;
        request(&state, &context, Work::Refresh)?;
        let refresh_count = state.pending.load(Ordering::SeqCst);
        assert!(lock(&state.refresh_pending).is_some());
        request(&state, &context, Work::Refresh)?;
        assert_eq!(state.pending.load(Ordering::SeqCst), refresh_count);
        let flags = lock(&data).deferred_flags.take().expect("deferred refresh");
        lock(&data).defer = false;
        sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags))?;
        assert!(lock(&state.refresh_pending).is_none());
        assert_eq!(state.pending.load(Ordering::SeqCst), 0);
        request(&state, &context, Work::Key(Key::Escape, Modifiers::NONE))?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好");
        for ch in "nihao".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        request(&state, &context, Work::Key(Key::Char('.'), Modifiers::NONE))?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好你好。");
        for ch in "ni".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        request(&state, &context, Work::Key(Key::Char('0'), Modifiers::NONE))?;
        assert_eq!(String::from_utf16_lossy(&lock(&data).text), "你好你好。ni0");
        assert!(lock(&state.composition).is_none());
        let session = state.session().expect("active session");
        session.submit(InputEvent::SetPinyinScheme(
            retype_types::PinyinScheme::Flypy,
        ));
        for ch in "nihc".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        let generation = session.backend.with_kernel(|k| k.generation());
        request(
            &state,
            &context,
            Work::Choose(0, generation.wrapping_sub(1)),
        )?;
        assert_eq!(
            String::from_utf16_lossy(&lock(&data).text),
            "你好你好。ni0nihc"
        );
        request(&state, &context, Work::Choose(0, generation))?;
        assert_eq!(
            String::from_utf16_lossy(&lock(&data).text),
            "你好你好。ni0你好"
        );
        let _ = stats::take_test_words();
        assert!(state.wants(Key::Char('H'), Modifiers::SHIFT));
        for ch in "Hello".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        request(&state, &context, Work::Key(Key::Char('1'), Modifiers::NONE))?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with("你好Hello"));
        assert!(session.backend.with_kernel(|k| k.is_chinese()));
        assert_eq!(
            stats::take_test_words(),
            1,
            "mixed candidate counts as English"
        );
        request(&state, &context, Work::Toggle)?;
        assert!(!session.backend.with_kernel(|k| k.is_chinese()));
        let _ = stats::take_test_words();
        for ch in "hel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(session.backend.with_kernel(|k| k
            .render_state()
            .candidates
            .iter()
            .any(|c| c.text == "hello")));
        request(&state, &context, Work::Key(Key::Space, Modifiers::NONE))?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with("hel "));
        assert_eq!(
            stats::take_test_words(),
            1,
            "literal word + space counts once"
        );
        for ch in "hel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        let first = session
            .backend
            .with_kernel(|k| k.render_state().candidates[0].text.clone());
        let english_list = lock(&state.window)
            .as_ref()
            .and_then(crate::candidate::CandidateWindow::element_for_test)
            .ok_or(E_FAIL)?;
        let integrated: ITfIntegratableCandidateListUIElement = english_list.cast()?;
        assert!(integrated.ShowCandidateNumbers()?.as_bool());
        request(&state, &context, Work::Key(Key::Tab, Modifiers::NONE))?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with(&first));
        assert_eq!(
            stats::take_test_words(),
            1,
            "accepting a candidate without a trailing space counts one word"
        );
        // UI-less hosts display the Chinese/POS comment, but accepting the
        // displayed row must submit only its original English commit text.
        let before_annotated = String::from_utf16_lossy(&lock(&data).text);
        for ch in "hel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        let mut annotated = session.backend.with_kernel(|k| k.render_state());
        let expected_word = annotated.candidates[0].text.clone();
        annotated.candidates[0].language = retype_types::CandidateLanguage::English;
        annotated.candidates[0].comment = "n. 测试释义".into();
        let layout = crate::popup::measure(&annotated, None, None);
        let mut window = lock(&state.window).take().ok_or(E_FAIL)?;
        window.update(&state, &manager, &context, &annotated, None, &layout)?;
        let list = window.element_for_test().ok_or(E_FAIL)?;
        *lock(&state.window) = Some(window);
        assert!(list.GetString(0)?.to_string().contains("测试释义"));
        list.SetSelection(0)?;
        list.Finalize()?;
        assert_eq!(
            String::from_utf16_lossy(&lock(&data).text),
            format!("{before_annotated}{expected_word}"),
            "Finalize must not write a number, POS label or Chinese meaning"
        );
        assert!(lock(&state.composition).is_none());
        let _ = stats::take_test_words();
        // A segmented row exposes only the unfinished suffix, while Finalize
        // replaces the complete owned composition once with its complete text.
        let before_segmented = String::from_utf16_lossy(&lock(&data).text);
        for ch in "qzxvfixture".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        let mut segmented_candidate =
            retype_types::Candidate::new("I am trying to", retype_types::CandidateSource::Local);
        segmented_candidate.language = retype_types::CandidateLanguage::English;
        segmented_candidate.display_text = Some("trying to".into());
        segmented_candidate.comment = "v. 尝试".into();
        segmented_candidate.consumed = "qzxvfixture".len();
        let generation = session.backend.with_kernel(|kernel| kernel.generation());
        session.submit(InputEvent::EnglishCompleted {
            gen: generation,
            candidates: vec![segmented_candidate],
        });
        let segmented = session.backend.with_kernel(|kernel| kernel.render_state());
        let segmented_index = segmented
            .candidates
            .iter()
            .position(|candidate| candidate.text == "I am trying to")
            .ok_or(E_FAIL)?;
        let layout = crate::popup::measure(&segmented, None, None);
        let mut window = lock(&state.window).take().ok_or(E_FAIL)?;
        window.update(&state, &manager, &context, &segmented, None, &layout)?;
        let list = window.element_for_test().ok_or(E_FAIL)?;
        *lock(&state.window) = Some(window);
        let shown = list.GetString(segmented_index as u32)?.to_string();
        assert!(shown.starts_with("trying to"));
        assert!(!shown.contains("I am"));
        list.SetSelection(segmented_index as u32)?;
        list.Finalize()?;
        assert_eq!(
            String::from_utf16_lossy(&lock(&data).text),
            format!("{before_segmented}I am trying to")
        );
        assert!(lock(&state.composition).is_none());
        let _ = stats::take_test_words();
        // 1-5 choose an existing visible row; other digits (and missing rows)
        // remain literal in the complete TSF edit transaction.
        for digit in '0'..='9' {
            for ch in "art".chars() {
                request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
            }
            let render = session.backend.with_kernel(|k| k.render_state());
            let expected = if ('1'..='5').contains(&digit) {
                render
                    .visible()
                    .get(digit as usize - '1' as usize)
                    .map(|candidate| candidate.text.clone())
                    .unwrap_or_else(|| format!("art{digit}"))
            } else {
                format!("art{digit}")
            };
            request(
                &state,
                &context,
                Work::Key(Key::Char(digit), Modifiers::NONE),
            )?;
            assert!(String::from_utf16_lossy(&lock(&data).text).ends_with(&expected));
            request(&state, &context, Work::Key(Key::Space, Modifiers::NONE))?;
        }
        for ch in "art".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(state.wants(Key::PageDown, Modifiers::NONE));
        assert!(state.wants(Key::Char('+'), Modifiers::SHIFT));
        let text_before_page = lock(&data).text.clone();
        request(&state, &context, Work::Key(Key::Char('='), Modifiers::NONE))?;
        assert!(session
            .backend
            .with_kernel(|k| k.render_state().page_start > 0));
        assert_eq!(lock(&data).text, text_before_page);
        request(
            &state,
            &context,
            Work::Key(Key::Char('_'), Modifiers::SHIFT),
        )?;
        assert_eq!(
            session.backend.with_kernel(|k| k.render_state().page_start),
            0
        );
        assert_eq!(lock(&data).text, text_before_page);
        request(&state, &context, Work::Finish(false))?;
        for ch in "qzxvfixture".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(!state.wants(Key::PageDown, Modifiers::NONE));
        assert!(!state.wants(Key::Char('+'), Modifiers::SHIFT));
        request(
            &state,
            &context,
            Work::Boundary(Key::Char('+'), Modifiers::SHIFT),
        )?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with("qzxvfixture"));
        let _ = stats::take_test_words();
        let before = String::from_utf16_lossy(&lock(&data).text);
        for ch in "hel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(!state.wants(Key::Enter, Modifiers::NONE));
        request(&state, &context, Work::Key(Key::Down, Modifiers::NONE))?;
        let first = session
            .backend
            .with_kernel(|k| k.render_state().candidates[0].text.clone());
        assert!(state.wants(Key::Enter, Modifiers::NONE));
        request(&state, &context, Work::Key(Key::Enter, Modifiers::NONE))?;
        assert_eq!(
            String::from_utf16_lossy(&lock(&data).text),
            format!("{before}{first}"),
            "explicit Enter accepts the word without inserting a newline"
        );
        assert!(!state.wants(Key::Enter, Modifiers::NONE));
        assert!(lock(&state.composition).is_none());
        let _ = stats::take_test_words();
        lock(&seen_candidates).clear();
        for ch in "hellp".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !lock(&seen_candidates)
            .iter()
            .any(|word| word.split_whitespace().next() == Some("hello"))
            && std::time::Instant::now() < deadline
        {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            lock(&seen_candidates)
                .iter()
                .any(|word| word.split_whitespace().next() == Some("hello")),
            "background spelling suggestions must reach UI-less hosts without another key"
        );
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with("hellp"));
        request(&state, &context, Work::Key(Key::Space, Modifiers::NONE))?;
        for ch in "word".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(!state.wants(Key::Enter, Modifiers::NONE));
        request(
            &state,
            &context,
            Work::Boundary(Key::Enter, Modifiers::NONE),
        )?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with("word"));
        assert!(lock(&state.composition).is_none());
        assert_eq!(
            stats::take_test_words(),
            2,
            "spelling and Enter commits both count words"
        );
        for ch in "cancel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        request(&state, &context, Work::Key(Key::Escape, Modifiers::NONE))?;
        assert_eq!(
            stats::take_test_words(),
            0,
            "cancelled composition contributes no word"
        );
        lock(&data).input_scope = Some(IS_PASSWORD);
        request(&state, &context, Work::Key(Key::Char('s'), Modifiers::NONE))?;
        assert!(lock(&state.composition).is_none());
        assert!(!session.backend.with_kernel(|k| k.has_candidates()));
        lock(&data).input_scope = Some(IS_DEFAULT);
        request(&state, &context, Work::Toggle)?;
        assert!(session.backend.with_kernel(|k| k.is_chinese()));
        crate::profiles::apply(&state, true)?;
        let keys: ITfKeyEventSink = crate::tip::KeyEventSink {
            state: Arc::downgrade(&state),
        }
        .into();
        let shift = WPARAM(0x10);
        let lp = LPARAM(0);
        // The product leaves bare Shift to the host by default. This fixture
        // must not inherit a user's saved binding or assume the upstream Shift
        // default that the English-only product deliberately removed.
        state.set_shortcuts_for_test(retype_ai::config::Shortcuts::default());
        assert!(!keys.OnTestKeyDown(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyDown(&context, shift, lp)?.as_bool());
        assert!(!keys.OnTestKeyUp(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyUp(&context, shift, lp)?.as_bool());
        assert!(session.backend.with_kernel(|k| k.is_chinese()));
        // Keep the legacy Chinese/ASCII routing covered as an explicitly
        // configured fixture; it is never enabled by the production default.
        state.set_shortcuts_for_test(retype_ai::config::Shortcuts {
            mode: retype_ai::config::Shortcut::SHIFT,
            ..Default::default()
        });
        let tap_shift = || -> Result<()> {
            assert!(keys.OnTestKeyDown(&context, shift, lp)?.as_bool());
            assert!(!keys.OnKeyDown(&context, shift, lp)?.as_bool());
            assert!(keys.OnTestKeyUp(&context, shift, lp)?.as_bool());
            assert!(!keys.OnKeyUp(&context, shift, lp)?.as_bool());
            Ok(())
        };
        tap_shift()?;
        assert!(!session.backend.with_kernel(|k| k.is_chinese()));
        assert!(!session
            .backend
            .with_kernel(|k| k.english_prediction_enabled()));
        let before = lock(&data).text.len();
        for ch in "HEL".chars() {
            assert!(keys.OnKeyDown(&context, WPARAM(ch as usize), lp)?.as_bool());
            assert!(!session
                .backend
                .with_kernel(|k| k.has_composition() || k.has_candidates()));
        }
        assert_eq!(
            lock(&data).text.len(),
            before + 3,
            "Shift English inserts each letter without composing"
        );
        assert!(keys.OnTestKeyDown(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyDown(&context, shift, lp)?.as_bool());
        let _ = keys.OnTestKeyDown(&context, WPARAM('A' as usize), lp)?;
        assert!(keys.OnTestKeyUp(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyUp(&context, shift, lp)?.as_bool());
        assert!(!session.backend.with_kernel(|k| k.is_chinese()));
        tap_shift()?;
        assert!(session.backend.with_kernel(|k| k.is_chinese()));
        // Only selecting the independent English profile enables prediction.
        crate::profiles::apply(&state, false)?;
        assert!(!keys.OnTestKeyDown(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyDown(&context, shift, lp)?.as_bool());
        assert!(!keys.OnTestKeyUp(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyUp(&context, shift, lp)?.as_bool());
        assert!(!session.backend.with_kernel(|k| k.is_chinese()));
        session.submit(InputEvent::SetEnglishOptions {
            enabled: true,
            spelling: true,
        });
        for ch in "hel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(session.backend.with_kernel(|k| k
            .render_state()
            .candidates
            .iter()
            .any(|c| c.text == "hello")));
        let before_toggle = session.backend.with_kernel(|k| k.render_state());
        let text_before_toggle = lock(&data).text.clone();
        assert!(!keys.OnKeyDown(&context, shift, lp)?.as_bool());
        assert!(!keys.OnKeyUp(&context, shift, lp)?.as_bool());
        request(&state, &context, Work::Toggle)?;
        request(&state, &context, Work::SetChinese(true))?;
        assert!(!session.backend.with_kernel(|k| k.is_chinese()));
        let after_toggle = session.backend.with_kernel(|k| k.render_state());
        // Background spelling results can extend candidates during these calls.
        assert_eq!(after_toggle.gen, before_toggle.gen);
        assert_eq!(after_toggle.composition, "hel");
        assert!(after_toggle.candidates.iter().any(|c| c.text == "hello"));
        assert_eq!(lock(&data).text, text_before_toggle);
        assert!(lock(&state.composition).is_some());
        crate::profiles::apply(&state, true)?;
        assert!(lock(&state.composition).is_none());
        tap_shift()?;
        assert!(!session
            .backend
            .with_kernel(|k| k.is_chinese() || k.english_prediction_enabled()));
        session.submit(InputEvent::SetEnglishOptions {
            enabled: false,
            spelling: false,
        });
        let text_before = lock(&data).text.len();
        let letter = WPARAM('Z' as usize);
        assert!(keys.OnTestKeyDown(&context, letter, lp)?.as_bool());
        assert!(keys.OnKeyDown(&context, letter, lp)?.as_bool());
        assert_eq!(lock(&data).text.len(), text_before + 1);
        // Printable English goes through a synchronous edit so it can only be
        // counted after actual insertion, while rejected hosts retain their key.
        request(&state, &context, Work::Direct('A'))?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with('A'));
        lock(&data).reject = true;
        assert!(request(&state, &context, Work::Direct('B')).is_err());
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with('A'));
        lock(&data).reject = false;
        lock(&data).defer = true;
        assert!(request(&state, &context, Work::Direct('C')).is_err());
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with('A'));
        let flags = lock(&data)
            .deferred_flags
            .take()
            .expect("deferred English lock");
        lock(&data).defer = false;
        let sink = lock(&data).sink.clone().expect("advised sink");
        sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags))?;
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with('A'));
        assert_eq!(state.pending.load(Ordering::SeqCst), 0);
        lock(&data).hidden = true;
        assert!(!keys.OnTestKeyDown(&context, letter, lp)?.as_bool());
        request(&state, &context, Work::SetChinese(true))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !session
            .user
            .snapshot()
            .iter()
            .any(|entry| entry.1 == "你好")
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let learned_before = session.user.snapshot();
        assert!(
            learned_before.iter().any(|entry| entry.1 == "你好"),
            "normal TSF text fields must learn successful commits"
        );
        lock(&data).input_scope = Some(IS_PRIVATE);
        for ch in "nihao".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        request(&state, &context, Work::Key(Key::Space, Modifiers::NONE))?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            session.user.snapshot(),
            learned_before,
            "private TSF contexts must not learn"
        );
        lock(&data).hidden = false;
        lock(&data).input_scope = Some(IS_DEFAULT);
        for ch in "nihao".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        lock(&data).reject_write = true;
        let _ = request(&state, &context, Work::Key(Key::Space, Modifiers::NONE));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            session.user.snapshot(),
            learned_before,
            "failed host writes must not learn"
        );
        lock(&data).reject_write = false;
        // Native popup reuse must update its click generation even when only
        // the composition changes and its visible candidate pixels stay equal.
        session.submit(InputEvent::SetEnglishOptions {
            enabled: false,
            spelling: false,
        });
        source.UnadviseSink(_ui_cookie)?;
        lock(&data).no_text_extent = false;
        session.submit(InputEvent::SetPinyinScheme(
            retype_types::PinyinScheme::Full,
        ));
        for ch in "ni".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        let window = lock(&state.window)
            .as_ref()
            .and_then(|w| w.native_handle())
            .expect("native popup");
        assert!(!crate::popup::paint_pending_for_test(window));
        // Change pixels without moving/resizing the window, and deliberately
        // do not pump the application's queue. Present must finish the ready
        // frame even while input or layout messages would outrank WM_PAINT.
        let actual = session.backend.with_kernel(|k| k.render_state());
        let mut changed = actual.clone();
        changed.candidates[0].text = "您好".into();
        let layout = crate::popup::measure(&changed, None, None);
        let update = crate::popup::update(window, &state, &context, &changed, &layout);
        assert!(!update.metrics.reused);
        assert!(crate::popup::paint_pending_for_test(window));
        crate::popup::present(window);
        assert!(!crate::popup::paint_pending_for_test(window));
        let layout = crate::popup::measure(&actual, None, None);
        crate::popup::update(window, &state, &context, &actual, &layout);
        crate::popup::present(window);
        let (before, old_generation) = crate::popup::frame_for_test(window);
        request(
            &state,
            &context,
            Work::Key(Key::Char('\''), Modifiers::NONE),
        )?;
        let (after, generation) = crate::popup::frame_for_test(window);
        assert!(Arc::ptr_eq(&before, &after));
        assert_ne!(old_generation, generation);
        assert_eq!(generation, session.backend.with_kernel(|k| k.generation()));
        let center = after.geometry.cells[0].center();
        let click = LPARAM(((center.y as i32) << 16 | center.x as i32) as isize);
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            window,
            windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONUP,
            Some(WPARAM(0)),
            Some(click),
        );
        assert!(String::from_utf16_lossy(&lock(&data).text).ends_with('你'));
        assert!(lock(&state.composition).is_none());
        // Symbol pairing is off in the product by default. Cover that mode
        // explicitly, then opt this fixture into pairing for the retained
        // nesting/deletion/Chromium regressions, without changing user settings.
        {
            let _disabled = crate::preferences::symbol_completion_for_test(false);
            let before = String::from_utf16_lossy(&lock(&data).text);
            request(&state, &context, Work::Key(Key::Char('('), Modifiers::NONE))?;
            assert_eq!(
                String::from_utf16_lossy(&lock(&data).text),
                format!("{before}（")
            );
            assert!(lock(&state.symbol_pairs).is_empty());
            request(&state, &context, Work::Key(Key::Char(')'), Modifiers::NONE))?;
        }
        let _enabled = crate::preferences::symbol_completion_for_test(true);
        check_symbol_completion(&state, &context, &data)?;
        // Run the same nesting, deletion, closer-skip and Pinyin/English cases
        // against a host that applies Chromium's post-lock caret behavior.
        request(&state, &context, Work::Toggle)?;
        {
            let mut host = lock(&data);
            host.start = host.text.len() as i32;
            host.end = host.start;
            host.chromium = true;
        }
        // Native SetSelection succeeds but the commit subsequently overrides it:
        // reproduce the original Edge/Paseo failure without the compatibility transport.
        let before = lock(&data).text.len() as i32;
        request(&state, &context, Work::Key(Key::Char('('), Modifiers::NONE))?;
        assert_eq!(lock(&data).start, before + 2);
        lock(&state.symbol_pairs).clear();
        crate::symbol_cursor::emulate_chromium(true);
        let outcome = check_symbol_completion(&state, &context, &data);
        crate::symbol_cursor::emulate_chromium(false);
        outcome?;
        // Native profile notifications must use a host edit transaction when
        // switching away from an unfinished English word.
        session.submit(InputEvent::SetEnglishOptions {
            enabled: true,
            spelling: true,
        });
        crate::profiles::apply(&state, false)?;
        let original = {
            let mut host = lock(&data);
            host.start = host.text.len() as i32;
            host.end = host.start;
            host.chromium = false;
            String::from_utf16_lossy(&host.text)
        };
        for ch in "hel".chars() {
            request(&state, &context, Work::Key(Key::Char(ch), Modifiers::NONE))?;
        }
        assert!(lock(&state.composition).is_some());
        crate::profiles::apply(&state, true)?;
        assert!(session.backend.with_kernel(|k| k.is_chinese()));
        assert!(lock(&state.composition).is_none());
        assert_eq!(
            String::from_utf16_lossy(&lock(&data).text),
            format!("{original}hel")
        );
        state.deactivate()?;
        document.Pop(TF_POPF_ALL)?;
        manager.Deactivate()?;
        Ok(())
    }
}

fn check_symbol_completion(
    state: &Arc<TipState>,
    context: &ITfContext,
    data: &Arc<Mutex<Data>>,
) -> Result<()> {
    let original = String::from_utf16_lossy(&lock(data).text);
    let original_len = lock(data).text.len() as i32;
    let type_key = |c| request(state, context, Work::Key(Key::Char(c), Modifiers::NONE));
    type_key('(')?;
    assert_eq!(
        String::from_utf16_lossy(&lock(data).text),
        format!("{original}（）")
    );
    assert_eq!(lock(data).start, original_len + 1);
    assert_eq!(lock(&state.symbol_pairs).len(), 1);
    // Empty pairs delete together, through a synchronous native transaction.
    request(state, context, Work::PairBackspace)?;
    assert_eq!(String::from_utf16_lossy(&lock(data).text), original);
    type_key('(')?;
    type_key('[')?;
    for c in "nihao".chars() {
        type_key(c)?;
    }
    type_key(']')?;
    assert_eq!(
        String::from_utf16_lossy(&lock(data).text),
        format!("{original}（【你好】）")
    );
    assert_eq!(lock(&state.symbol_pairs).len(), 1);
    // A nonempty pair must pass ordinary Backspace to the app, retaining closer ownership.
    assert!(request(state, context, Work::PairBackspace).is_err());
    assert_eq!(lock(&state.symbol_pairs).len(), 1);
    type_key(')')?;
    assert_eq!(
        String::from_utf16_lossy(&lock(data).text),
        format!("{original}（【你好】）")
    );
    assert!(lock(&state.symbol_pairs).is_empty());
    type_key('"')?;
    for c in "nihao".chars() {
        type_key(c)?;
    }
    type_key('"')?;
    let (actual, start, end, chromium) = {
        let host = lock(data);
        (
            String::from_utf16_lossy(&host.text),
            host.start,
            host.end,
            host.chromium,
        )
    };
    assert!(
        actual.ends_with("“你好”"),
        "quoted commit: {actual:?}, selection={start}..{end}, chromium={chromium}"
    );
    request(state, context, Work::Toggle)?;
    type_key('(')?;
    for c in "don't".chars() {
        type_key(c)?;
    }
    type_key(')')?;
    assert!(String::from_utf16_lossy(&lock(data).text).ends_with("(don't)"));
    assert!(lock(&state.symbol_pairs).is_empty());
    type_key('\'')?;
    request(state, context, Work::PairBackspace)?;
    assert!(String::from_utf16_lossy(&lock(data).text).ends_with("(don't)"));
    // An existing user selection follows the original replacement behavior.
    {
        let mut host = lock(data);
        host.start = host.end - 7;
    }
    type_key('[')?;
    assert!(String::from_utf16_lossy(&lock(data).text).ends_with('['));
    assert!(lock(&state.symbol_pairs).is_empty());
    // An editor rejecting the repositioning must keep only the opener.
    lock(data).reject_selection_after = Some(1);
    type_key('(')?;
    assert!(String::from_utf16_lossy(&lock(data).text).ends_with("[("));
    assert!(lock(&state.symbol_pairs).is_empty());
    // Once a closer already exists, do not duplicate it or adopt it.
    type_key(')')?;
    {
        let mut host = lock(data);
        host.start -= 1;
        host.end = host.start;
    }
    type_key('(')?;
    assert!(String::from_utf16_lossy(&lock(data).text).ends_with("[(()"));
    assert!(lock(&state.symbol_pairs).is_empty());
    assert!(request(state, context, Work::PairBackspace).is_err());
    Ok(())
}

fn check_candidate_getters(
    tip: &Arc<TipState>,
    manager: &ITfThreadMgr,
    context: &ITfContext,
) -> Result<()> {
    use retype_types::{Candidate, CandidateSource, RenderState};
    use std::{hint::black_box, time::Instant};
    use windows_core::BSTR;

    let mut render = RenderState {
        gen: 7,
        composition: "ss".into(),
        candidates: (0..512)
            .map(|i| {
                let mut candidate = Candidate::new(format!("候选{i}"), CandidateSource::Local);
                candidate.comment = "song".into();
                candidate.syllables = vec![1];
                candidate.consumed = 2;
                candidate
            })
            .collect(),
        page_size: 8,
        page_starts: (0..512).step_by(8).collect(),
        ..Default::default()
    };
    let layout = crate::popup::measure(&render, None, None);
    let mut window = crate::candidate::CandidateWindow::default();
    let metrics = window.update(tip, manager, context, &render, None, &layout)?;
    assert!(
        window.native_handle().is_none(),
        "UI-less host must own the UI"
    );
    assert!(
        metrics.string_calls >= 512,
        "count host enumeration inside notification"
    );
    let list = window.element_for_test().ok_or(E_FAIL)?;
    // SAFETY: Isolated, live COM apartment with an in-memory TSF host.
    unsafe {
        assert_eq!(list.GetCount()?, 512);
        assert_eq!(list.GetString(511)?.to_string(), "候选511");
        assert_eq!(list.GetString(512).unwrap_err().code(), E_INVALIDARG);
        let mut count = 0;
        let mut indexes = [0; 64];
        list.GetPageIndex(&mut indexes, &mut count)?;
        assert_eq!(count, 64);
        assert_eq!(indexes[63], 504);
        assert_eq!(list.GetCurrentPage()?, 0);
        // Repeated reads must retain a notification's flags.
        let initial_flags = list.GetUpdatedFlags()?;
        assert_ne!(initial_flags & TF_CLUIE_DOCUMENTMGR, 0);
        assert_eq!(list.GetUpdatedFlags()?, initial_flags);

        render.gen += 1;
        render.candidates[0].consumed = 1;
        render.candidates[0].score = 100.0;
        let metadata_only = window.update(tip, manager, context, &render, None, &layout)?;
        assert_eq!(metadata_only.updated_flags, 0);
        assert_eq!(metadata_only.string_calls, 0);
        assert_eq!(list.GetUpdatedFlags()?, initial_flags);
        list.SetSelection(7)?;
        assert_eq!(list.GetSelection()?, 7);
        render.gen += 1;
        let reset_selection = window.update(tip, manager, context, &render, None, &layout)?;
        assert_eq!(reset_selection.updated_flags, TF_CLUIE_SELECTION);
        assert_eq!(reset_selection.string_calls, 0);
        assert_eq!(list.GetSelection()?, 0);

        render.page_start = 8;
        render.selected = 8;
        let page_only = window.update(tip, manager, context, &render, None, &layout)?;
        assert_eq!(
            page_only.updated_flags,
            TF_CLUIE_SELECTION | TF_CLUIE_CURRENTPAGE
        );
        assert_eq!(page_only.string_calls, 0);
        assert_eq!(list.GetCurrentPage()?, 1);
        assert_eq!(list.GetSelection()?, 8);
        render.candidates[511].text = "最后一条".into();
        let text_only = window.update(tip, manager, context, &render, None, &layout)?;
        assert_eq!(text_only.updated_flags, TF_CLUIE_STRING);
        assert!(text_only.string_calls >= 512);
        assert_eq!(list.GetString(511)?.to_string(), "最后一条");

        // Compare equivalent full enumeration, including BSTR allocations, using
        // real candidate metadata. This is not an application latency guarantee.
        if std::env::var_os("RETYPE_BENCH_CANDIDATES").is_some() {
            let mut before = Vec::new();
            let mut after = Vec::new();
            for _ in 0..11 {
                let start = Instant::now();
                for i in 0..512 {
                    let cloned = black_box(&render).clone();
                    black_box(BSTR::from(cloned.candidates[i].text.as_str()));
                }
                before.push(start.elapsed().as_micros());
                let start = Instant::now();
                for i in 0..512 {
                    black_box(list.GetString(i)?);
                }
                after.push(start.elapsed().as_micros());
            }
            before.sort();
            after.sort();
            eprintln!("512-candidate enumeration median: previous full clone {} us, indexed COM getters {} us (no real application scheduling)", before[5], after[5]);
        }

        window.hide();
        render.candidates[0].text = "新的列表".into();
        let restarted = window.update(tip, manager, context, &render, None, &layout)?;
        assert_ne!(restarted.updated_flags & TF_CLUIE_DOCUMENTMGR, 0);
        assert_eq!(
            list.GetString(0)?.to_string(),
            "候选0",
            "retained element stays isolated after EndUIElement"
        );
        assert_eq!(
            window
                .element_for_test()
                .ok_or(E_FAIL)?
                .GetString(0)?
                .to_string(),
            "新的列表"
        );

        let other_document = manager.CreateDocumentMgr()?;
        let other_store: ITextStoreACP = Store {
            data: Arc::new(Mutex::new(Data::default())),
        }
        .into();
        let mut other_context = None;
        let mut cookie = 0;
        other_document.CreateContext(
            tip.tid.load(Ordering::SeqCst),
            0,
            &other_store,
            &mut other_context,
            &mut cookie,
        )?;
        let other_context = other_context.ok_or(E_FAIL)?;
        other_document.Push(&other_context)?;
        let switched = window.update(tip, manager, &other_context, &render, None, &layout)?;
        assert_ne!(switched.updated_flags & TF_CLUIE_DOCUMENTMGR, 0);
        assert_eq!(
            window.element_for_test().ok_or(E_FAIL)?.GetDocumentMgr()?,
            other_document
        );
        window.hide();
        other_document.Pop(TF_POPF_ALL)?;
    }
    window.hide();
    Ok(())
}

#[derive(Clone, Copy)]
enum AssistProbeWork {
    Capture,
    Preflight,
    Mutate(&'static str),
    Apply,
}
#[implement(ITfEditSession)]
struct AssistSelectionProbe {
    context: ITfContext,
    saved: Arc<Mutex<Option<crate::translation::SelectedText>>>,
    work: AssistProbeWork,
    applied: Arc<Mutex<Option<bool>>>,
}
impl ITfEditSession_Impl for AssistSelectionProbe_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        match self.work {
            AssistProbeWork::Capture => {
                *lock(&self.saved) = Some(
                    crate::translation::SelectedText::capture(&self.context, ec)
                        .map_err(|_| E_FAIL)?,
                );
            }
            AssistProbeWork::Preflight => {
                let snapshot = lock(&self.saved).clone().ok_or(E_FAIL)?;
                let mut dispatched = false;
                let _ = snapshot.with_verified(&self.context, ec, || {
                    dispatched = true;
                    Ok(())
                });
                *lock(&self.applied) = Some(dispatched);
            }
            AssistProbeWork::Mutate(text) => unsafe {
                let mut selections = [TF_SELECTION::default()];
                let mut count = 0;
                self.context
                    .GetSelection(ec, TF_DEFAULT_SELECTION, &mut selections, &mut count)?;
                let range = ManuallyDrop::take(&mut selections[0].range).ok_or(E_FAIL)?;
                range.SetText(ec, 0, &text.encode_utf16().collect::<Vec<_>>())?;
                let selected = TF_SELECTION {
                    range: ManuallyDrop::new(Some(range)),
                    style: TF_SELECTIONSTYLE {
                        ase: TF_AE_NONE,
                        fInterimChar: false.into(),
                    },
                };
                self.context
                    .SetSelection(ec, std::slice::from_ref(&selected))?;
                let _ = ManuallyDrop::into_inner(selected.range);
            },
            AssistProbeWork::Apply => {
                let snapshot = lock(&self.saved).clone().ok_or(E_FAIL)?;
                *lock(&self.applied) = Some(snapshot.replace(&self.context, ec, "FIX").is_ok());
            }
        }
        Ok(())
    }
}
#[test]
fn assistance_selection_refuses_change_then_undo_and_keeps_neighbor_text() -> Result<()> {
    std::thread::spawn(|| -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let document = manager.CreateDocumentMgr()?;
                let original: Vec<u16> = "prefix OLD suffix".encode_utf16().collect();
                let data = Arc::new(Mutex::new(Data {
                    text: original.clone(),
                    start: 7,
                    end: 10,
                    input_scope: Some(IS_DEFAULT),
                    ..Default::default()
                }));
                let store: ITextStoreACP = Store {
                    data: Arc::clone(&data),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                document.Push(&context)?;
                let saved = Arc::new(Mutex::new(None));
                let applied = Arc::new(Mutex::new(None));
                let run = |work| -> Result<()> {
                    let session: ITfEditSession = AssistSelectionProbe {
                        context: context.clone(),
                        saved: Arc::clone(&saved),
                        work,
                        applied: Arc::clone(&applied),
                    }
                    .into();
                    context
                        .RequestEditSession(tid, &session, TF_ES_SYNC | TF_ES_READWRITE)?
                        .ok()
                };
                for scope in [
                    None,
                    Some(IS_PASSWORD),
                    Some(IS_PRIVATE),
                    Some(IS_NUMERIC_PIN),
                ] {
                    lock(&data).input_scope = scope;
                    assert!(run(AssistProbeWork::Capture).is_err());
                    assert_eq!(lock(&data).text, original);
                }
                lock(&data).input_scope = Some(IS_DEFAULT);
                run(AssistProbeWork::Capture)?;
                assert!(lock(&saved).as_ref().is_some_and(|s| s.current()));
                run(AssistProbeWork::Preflight)?;
                assert_eq!(lock(&applied).take(), Some(true));
                run(AssistProbeWork::Mutate("NEW"))?;
                run(AssistProbeWork::Mutate("OLD"))?;
                assert_eq!(lock(&data).text, original);
                assert!(
                    !lock(&saved).as_ref().is_some_and(|s| s.current()),
                    "undo must not revive an old AI request"
                );
                run(AssistProbeWork::Preflight)?;
                assert_eq!(
                    lock(&applied).take(),
                    Some(false),
                    "stale source must not dispatch a request"
                );
                run(AssistProbeWork::Apply)?;
                assert_eq!(lock(&applied).take(), Some(false));
                assert_eq!(lock(&data).text, original);
                run(AssistProbeWork::Capture)?;
                run(AssistProbeWork::Apply)?;
                assert_eq!(lock(&applied).take(), Some(true));
                assert_eq!(
                    String::from_utf16_lossy(&lock(&data).text),
                    "prefix FIX suffix"
                );
                {
                    let mut data = lock(&data);
                    data.start = 7;
                    data.end = 10;
                }
                let sink = lock(&data).sink.clone();
                if let Some(sink) = sink {
                    sink.OnSelectionChange()?;
                }
                run(AssistProbeWork::Capture)?;
                lock(&data).input_scope = Some(IS_PRIVATE);
                let sink = lock(&data).sink.clone();
                if let Some(sink) = sink {
                    sink.OnAttrsChange(0, original.len() as i32, &[GUID_PROP_INPUTSCOPE])?;
                }
                run(AssistProbeWork::Preflight)?;
                assert_eq!(
                    lock(&applied).take(),
                    Some(false),
                    "scope-only privacy change must not dispatch a request"
                );
                run(AssistProbeWork::Apply)?;
                assert_eq!(lock(&applied).take(), Some(false));
                assert_eq!(
                    String::from_utf16_lossy(&lock(&data).text),
                    "prefix FIX suffix"
                );
                lock(&saved).take();
                document.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}

#[test]
fn assistance_scope_defaults_require_a_complete_nonsensitive_partition() -> Result<()> {
    std::thread::spawn(|| -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                for (segments, read_error, transition_error, allowed) in [
                    (Vec::new(), false, false, true),
                    (
                        vec![(0, 2, None), (2, 5, Some(IS_PASSWORD)), (5, 9, None)],
                        false,
                        false,
                        false,
                    ),
                    (
                        vec![(0, 2, None), (2, 5, Some(IS_PRIVATE)), (5, 9, None)],
                        false,
                        false,
                        false,
                    ),
                    (
                        vec![(0, 2, None), (2, 5, Some(IS_NUMERIC_PIN)), (5, 9, None)],
                        false,
                        false,
                        false,
                    ),
                    (
                        vec![(0, 2, None), (2, 5, Some(InputScope(123456))), (5, 9, None)],
                        false,
                        false,
                        false,
                    ),
                    (Vec::new(), true, false, false),
                    (Vec::new(), false, true, false),
                ] {
                    // Each fresh context begins with its actual attribute map;
                    // no stale TSF attribute cache or omitted notification is relied on.
                    let document = manager.CreateDocumentMgr()?;
                    let data = Arc::new(Mutex::new(Data {
                        text: "ordinary!".encode_utf16().collect(),
                        start: 0,
                        end: 9,
                        hidden: true,
                        scope_segments: segments,
                        empty_scope_attribute: true,
                        scope_read_error: read_error,
                        scope_transition_error: transition_error,
                        ..Default::default()
                    }));
                    let store: ITextStoreACP = Store {
                        data: Arc::clone(&data),
                    }
                    .into();
                    let mut context = None;
                    let mut cookie = 0;
                    document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                    let context = context.ok_or(E_FAIL)?;
                    document.Push(&context)?;
                    let saved = Arc::new(Mutex::new(None));
                    let session: ITfEditSession = AssistSelectionProbe {
                        context: context.clone(),
                        saved: Arc::clone(&saved),
                        work: AssistProbeWork::Capture,
                        applied: Arc::new(Mutex::new(None)),
                    }
                    .into();
                    let captured = context
                        .RequestEditSession(tid, &session, TF_ES_SYNC | TF_ES_READWRITE)
                        .is_ok_and(|result| result.is_ok());
                    assert_eq!(
                        captured, allowed,
                        "read_error={read_error}, transition_error={transition_error}"
                    );
                    assert_eq!(String::from_utf16_lossy(&lock(&data).text), "ordinary!");
                    lock(&saved).take();
                    document.Pop(TF_POPF_ALL)?;
                }
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}
#[test]
fn assistance_selection_rejects_a_fresh_truncated_text_store() -> Result<()> {
    std::thread::spawn(|| -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let document = manager.CreateDocumentMgr()?;
                let original: Vec<u16> = "prefix OLD suffix".encode_utf16().collect();
                // Set the host's read limitation before TSF can cache any text.
                let data = Arc::new(Mutex::new(Data {
                    text: original.clone(),
                    start: 7,
                    end: 10,
                    input_scope: Some(IS_DEFAULT),
                    text_read_stop: Some(8),
                    ..Default::default()
                }));
                let store: ITextStoreACP = Store {
                    data: Arc::clone(&data),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                document.Push(&context)?;
                let saved = Arc::new(Mutex::new(None));
                let session: ITfEditSession = AssistSelectionProbe {
                    context: context.clone(),
                    saved: Arc::clone(&saved),
                    work: AssistProbeWork::Capture,
                    applied: Arc::new(Mutex::new(None)),
                }
                .into();
                // Rejected reads may be returned as the outer request error or
                // as the edit-session HRESULT; neither may produce a snapshot.
                let result = context
                    .RequestEditSession(tid, &session, TF_ES_SYNC | TF_ES_READWRITE)
                    .and_then(|status| status.ok());
                assert!(
                    result.is_err(),
                    "a prefix must not authorize replacing the entire selected range"
                );
                assert!(
                    lock(&data).truncated_reads > 0,
                    "fixture must exercise the truncated host, not an earlier full-text cache"
                );
                assert!(lock(&saved).is_none());
                assert_eq!(lock(&data).text, original);
                document.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}

#[test]
fn assistance_focus_events_close_notices_without_a_composition_or_timer_poll() -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows_core::w;
    std::thread::spawn(|| -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let first = manager.CreateDocumentMgr()?;
                let second = manager.CreateDocumentMgr()?;
                let store: ITextStoreACP = Store {
                    data: Arc::new(Mutex::new(Data::default())),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                first.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                first.Push(&context)?;
                let state = TipState::new();
                let events = crate::tip::focus_sink_for_test(&state);
                let thread_events: ITfThreadFocusSink = events.cast()?;
                let key_events: ITfKeyEventSink = crate::tip::KeyEventSink {
                    state: Arc::downgrade(&state),
                }
                .into();
                for action in 0..5 {
                    assert!(lock(&state.composition).is_none());
                    // Real hidden windows, no timer, input injection, application
                    // activation or network. Only the focus callback can close them.
                    let notice = || -> Result<HWND> {
                        CreateWindowExW(
                            WINDOW_EX_STYLE::default(),
                            w!("STATIC"),
                            w!("assist focus test"),
                            WS_OVERLAPPED,
                            0,
                            0,
                            1,
                            1,
                            Some(HWND_MESSAGE),
                            None,
                            None,
                            None,
                        )
                    };
                    let ai = notice()?;
                    let voice = notice()?;
                    *lock(&state.translation_window) = Some(ai.0 as usize);
                    *lock(&state.voice_window) = Some(voice.0 as usize);
                    match action {
                        0 => {
                            // Redundant focus notification must keep the request.
                            events.OnSetFocus(&first, &first)?;
                            assert!(IsWindow(Some(ai)).as_bool());
                            // No message pump between A -> B -> A.
                            events.OnSetFocus(&second, &first)?;
                            events.OnSetFocus(&first, &second)?;
                        }
                        1 => events.OnPushContext(&context)?,
                        2 => events.OnPopContext(&context)?,
                        3 => thread_events.OnKillThreadFocus()?,
                        _ => key_events.OnSetFocus(false)?,
                    }
                    assert!(lock(&state.translation_window).is_none());
                    assert!(lock(&state.voice_window).is_none());
                    assert!(!IsWindow(Some(ai)).as_bool());
                    assert!(!IsWindow(Some(voice)).as_bool());
                }
                first.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}

type VoiceSelection = Arc<Mutex<Option<(ITfRange, Vec<u16>)>>>;
#[implement(ITfEditSession)]
struct VoiceProbe {
    context: ITfContext,
    saved: VoiceSelection,
    capture: bool,
    output: Arc<Mutex<Option<bool>>>,
}
impl ITfEditSession_Impl for VoiceProbe_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        if self.capture {
            unsafe {
                let mut selected = [TF_SELECTION::default()];
                let mut count = 0;
                self.context
                    .GetSelection(ec, TF_DEFAULT_SELECTION, &mut selected, &mut count)?;
                let range = ManuallyDrop::take(&mut selected[0].range).ok_or(E_FAIL)?;
                let mut text = [0u16; 64];
                let mut read = 0;
                range.GetText(ec, 0, &mut text, &mut read)?;
                *lock(&self.saved) = Some((range, text[..read as usize].to_vec()));
            }
        } else {
            let (range, text) = lock(&self.saved).clone().ok_or(E_FAIL)?;
            *lock(&self.output) = Some(
                crate::voice::replace_verified(&self.context, &range, ec, &text, "语音内容")
                    .is_ok(),
            );
        }
        Ok(())
    }
}
#[test]
fn voice_writes_only_the_original_selection_and_refuses_private_changed_or_rejected_targets(
) -> Result<()> {
    std::thread::spawn(|| -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let result = (|| -> Result<()> {
                let manager: ITfThreadMgr =
                    CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
                let tid = manager.Activate()?;
                let document = manager.CreateDocumentMgr()?;
                let original = "prefix OLD suffix".encode_utf16().collect::<Vec<_>>();
                let data = Arc::new(Mutex::new(Data {
                    text: original.clone(),
                    start: 7,
                    end: 10,
                    input_scope: Some(IS_DEFAULT),
                    ..Default::default()
                }));
                let store: ITextStoreACP = Store {
                    data: Arc::clone(&data),
                }
                .into();
                let mut context = None;
                let mut cookie = 0;
                document.CreateContext(tid, 0, &store, &mut context, &mut cookie)?;
                let context = context.ok_or(E_FAIL)?;
                document.Push(&context)?;
                let saved = Arc::new(Mutex::new(None));
                let output = Arc::new(Mutex::new(None));
                let probe = |capture: bool| -> Result<bool> {
                    let edit: ITfEditSession = VoiceProbe {
                        context: context.clone(),
                        saved: Arc::clone(&saved),
                        capture,
                        output: Arc::clone(&output),
                    }
                    .into();
                    context
                        .RequestEditSession(tid, &edit, TF_ES_SYNC | TF_ES_READWRITE)?
                        .ok()?;
                    Ok(lock(&output).take().unwrap_or(false))
                };
                probe(true)?;
                for scope in [IS_PASSWORD, IS_PRIVATE, IS_NUMERIC_PIN] {
                    lock(&data).input_scope = Some(scope);
                    assert!(!probe(false)?);
                    assert_eq!(lock(&data).text, original);
                }
                lock(&data).input_scope = Some(IS_DEFAULT);
                {
                    let mut d = lock(&data);
                    d.start = 0;
                    d.end = 0;
                }
                assert!(!probe(false)?);
                {
                    let mut d = lock(&data);
                    d.start = 7;
                    d.end = 10;
                    d.text[7] = b'N' as u16;
                }
                let sink = lock(&data).sink.clone().ok_or(E_FAIL)?;
                sink.OnTextChange(
                    TEXT_STORE_TEXT_CHANGE_FLAGS(0),
                    &TS_TEXTCHANGE {
                        acpStart: 7,
                        acpOldEnd: 8,
                        acpNewEnd: 8,
                    },
                )?;
                sink.OnSelectionChange()?;
                assert!(!probe(false)?);
                lock(&data).text = original.clone();
                sink.OnTextChange(
                    TEXT_STORE_TEXT_CHANGE_FLAGS(0),
                    &TS_TEXTCHANGE {
                        acpStart: 7,
                        acpOldEnd: 8,
                        acpNewEnd: 8,
                    },
                )?;
                lock(&data).reject_write = true;
                assert!(!probe(false)?);
                assert_eq!(lock(&data).text, original);
                lock(&data).reject_write = false;
                assert!(probe(false)?);
                assert_eq!(
                    String::from_utf16_lossy(&lock(&data).text),
                    "prefix 语音内容 suffix"
                );
                document.Pop(TF_POPF_ALL)?;
                manager.Deactivate()?;
                Ok(())
            })();
            CoUninitialize();
            result
        }
    })
    .join()
    .map_err(|_| E_FAIL)?
}
