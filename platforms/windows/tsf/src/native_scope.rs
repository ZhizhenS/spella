//! Explicit AI action compatibility for verified native plain RichEdit selections.
//! This is independent evidence, never a claim that a failed InputScope is IS_DEFAULT.
use std::{
    ffi::c_void,
    mem::{size_of, ManuallyDrop},
};
use windows::{
    core::{Interface, HRESULT},
    Win32::{
        Foundation::*,
        System::{
            Threading::{GetCurrentProcessId, GetCurrentThreadId},
            Variant::VT_EMPTY,
        },
        UI::{
            Controls::{RichEdit::*, EM_GETPASSWORDCHAR},
            Input::KeyboardAndMouse::IsWindowEnabled,
            TextServices::*,
            WindowsAndMessaging::*,
        },
    },
};

pub(crate) fn eligible(value_hr: i32, variant: u16) -> bool {
    value_hr == E_FAIL.0 && variant == VT_EMPTY.0
}
#[derive(Default, Debug)]
struct Diagnostic {
    stage: &'static str,
    hr: Option<i32>,
    focus: usize,
    view: usize,
    class: u8,
    style: u32,
    ex_style: u32,
    context_matches: bool,
    same_thread_process: bool,
    visible: bool,
    enabled: bool,
    view_related: bool,
    foreground_matches: bool,
    password_char: Option<usize>,
    tom_window: Option<i64>,
    tsf_start: Option<i32>,
    tsf_end: Option<i32>,
    tom_start: Option<i32>,
    tom_end: Option<i32>,
    story: Option<i32>,
    selection_type: Option<i32>,
    can_edit: Option<i32>,
    hidden: Option<i32>,
    protected: Option<i32>,
}
#[derive(Clone)]
struct Evidence {
    context_matches: bool,
    same_thread_process: bool,
    visible: bool,
    enabled: bool,
    view_related: bool,
    foreground_matches: bool,
    class: u8,
    style: u32,
    password_char: usize,
    tom_window_matches: bool,
    tsf_start: i32,
    tsf_end: i32,
    tom_start: i32,
    tom_end: i32,
    story: i32,
    selection_type: i32,
    can_edit: i32,
    hidden: i32,
    protected: i32,
}
impl Evidence {
    fn permits(&self) -> bool {
        self.context_matches
            && self.same_thread_process
            && self.visible
            && self.enabled
            && self.view_related
            && self.foreground_matches
            && self.class != 0
            && self.style & ((ES_PASSWORD | ES_READONLY) as u32) == 0
            && self.password_char == 0
            && self.tom_window_matches
            && self.tsf_start >= 0
            && self.tsf_end > self.tsf_start
            && self.tom_start == self.tsf_start
            && self.tom_end == self.tsf_end
            && self.story == tomMainTextStory.0
            && self.selection_type == tomSelectionNormal.0
            && self.can_edit == tomTrue.0
            && self.hidden == tomFalse.0
            && self.protected == tomFalse.0
    }
}
fn class_category(class: &[u16]) -> u8 {
    // Known control types only, never application names. Class alone grants nothing.
    let Ok(class) = String::from_utf16(class) else {
        return 0;
    };
    match class.to_ascii_lowercase().as_str() {
        "richedit" | "richedit20w" | "richedit20a" => 1,
        "richedit50w" | "richedit60w" => 2,
        "richeditd2d" => 3,
        "richeditd2dpt" => 4,
        _ => 0,
    }
}
type ScalarGetter = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;
unsafe fn window_style(
    window: HWND,
    index: WINDOW_LONG_PTR_INDEX,
    diagnostic: &mut Diagnostic,
) -> Option<u32> {
    // SAFETY: this is a verified same-thread window. Zero is a valid style, so
    // distinguish it from an API failure instead of granting absent flags.
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let style = GetWindowLongPtrW(window, index);
        let error = GetLastError();
        if style == 0 && error != WIN32_ERROR(0) {
            diagnostic.hr = Some(HRESULT::from_win32(error.0).0);
            return None;
        }
        Some(style as u32)
    }
}
unsafe fn scalar(
    object: *mut c_void,
    getter: ScalarGetter,
    stage: &'static str,
    diagnostic: &mut Diagnostic,
) -> Option<i32> {
    diagnostic.stage = stage;
    let mut value = tomUndefined.0;
    // SAFETY: caller supplies the matching generated COM vtable getter/interface.
    let hr = unsafe { getter(object, &mut value) };
    diagnostic.hr = Some(hr.0);
    (hr == S_OK && value != tomUndefined.0).then_some(value)
}
pub(crate) fn verify(
    manager: Option<&ITfThreadMgr>,
    context: &ITfContext,
    ec: u32,
    range: &ITfRange,
) -> bool {
    let mut diagnostic = Diagnostic::default();
    // SAFETY: caller is the TSF apartment holding a fresh read/write cookie.
    let allowed = unsafe { collect(manager, context, ec, range, &mut diagnostic) }
        .is_some_and(|evidence| evidence.permits());
    #[cfg(not(test))]
    crate::settings_log::event(
        "launcher",
        "ai_native_scope_check",
        0,
        format!("allowed={allowed} {diagnostic:?}"),
    );
    allowed
}
unsafe fn collect(
    manager: Option<&ITfThreadMgr>,
    context: &ITfContext,
    ec: u32,
    range: &ITfRange,
    diagnostic: &mut Diagnostic,
) -> Option<Evidence> {
    // SAFETY: all HWND/COM operations below are metadata reads on this same
    // apartment. The one pointer-bearing message is sent only after same-PID/TID
    // verification to the actual focused RichEdit; it is never sent cross-process.
    unsafe {
        diagnostic.stage = "focused_context";
        diagnostic.context_matches = manager?
            .GetFocus()
            .and_then(|document| document.GetTop())
            .is_ok_and(|focused| focused == *context);
        if !diagnostic.context_matches {
            return None;
        }
        diagnostic.stage = "gui_focus";
        let thread = GetCurrentThreadId();
        let mut gui = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if let Err(error) = GetGUIThreadInfo(thread, &mut gui) {
            diagnostic.hr = Some(error.code().0);
            return None;
        }
        let focus = gui.hwndFocus;
        diagnostic.focus = focus.0 as usize;
        if focus.is_invalid() {
            return None;
        }
        let mut process = 0;
        diagnostic.same_thread_process = GetWindowThreadProcessId(focus, Some(&mut process))
            == thread
            && process == GetCurrentProcessId();
        if !diagnostic.same_thread_process {
            return None;
        }
        diagnostic.stage = "visible_enabled";
        diagnostic.visible = IsWindowVisible(focus).as_bool();
        diagnostic.enabled = IsWindowEnabled(focus).as_bool();
        if !diagnostic.visible || !diagnostic.enabled {
            return None;
        }
        diagnostic.stage = "view_relationship";
        let view = context
            .GetActiveView()
            .and_then(|view| view.GetWnd())
            .ok()?;
        diagnostic.view = view.0 as usize;
        diagnostic.view_related =
            !view.is_invalid() && (view == focus || IsChild(view, focus).as_bool());
        let root = GetAncestor(focus, GA_ROOT);
        diagnostic.foreground_matches = !root.is_invalid() && GetForegroundWindow() == root;
        if !diagnostic.view_related || !diagnostic.foreground_matches {
            return None;
        }
        diagnostic.stage = "native_control";
        let mut class = [0u16; 128];
        let count = GetClassNameW(focus, &mut class);
        diagnostic.class = if count > 0 && (count as usize) < class.len() {
            class_category(&class[..count as usize])
        } else {
            0
        };
        if diagnostic.class == 0 {
            return None;
        }
        diagnostic.stage = "native_style";
        diagnostic.style = window_style(focus, GWL_STYLE, diagnostic)?;
        diagnostic.stage = "native_ex_style";
        diagnostic.ex_style = window_style(focus, GWL_EXSTYLE, diagnostic)?;
        if diagnostic.style & ((ES_PASSWORD | ES_READONLY) as u32) != 0 {
            return None;
        }
        diagnostic.stage = "password_character";
        let mut password = 0;
        if SendMessageTimeoutW(
            focus,
            EM_GETPASSWORDCHAR,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            100,
            Some(&mut password),
        )
        .0 == 0
        {
            return None;
        }
        diagnostic.password_char = Some(password);
        if password != 0 {
            return None;
        }
        diagnostic.stage = "native_ole";
        let mut raw: *mut c_void = std::ptr::null_mut();
        let mut result = 0;
        let sent = SendMessageTimeoutW(
            focus,
            EM_GETOLEINTERFACE,
            WPARAM(0),
            LPARAM((&mut raw as *mut *mut c_void) as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            100,
            Some(&mut result),
        );
        let ole = (!raw.is_null()).then(|| IRichEditOle::from_raw(raw));
        let ole = ole.filter(|_| sent.0 != 0 && result != 0)?;
        diagnostic.stage = "tom2_query";
        let document: ITextDocument2 = match ole.cast::<ITextDocument2>() {
            Ok(document) => document,
            Err(error) => {
                diagnostic.hr = Some(error.code().0);
                return None;
            }
        };
        diagnostic.stage = "tom_window";
        let mut window: i64 = 0;
        let hr = (document.vtable().GetWindow)(document.as_raw(), &mut window);
        diagnostic.hr = Some(hr.0);
        diagnostic.tom_window = Some(window);
        let window_matches = window == focus.0 as isize as i64;
        if hr != S_OK || !window_matches {
            return None;
        }
        diagnostic.stage = "tom_selection";
        let mut raw = std::ptr::null_mut();
        let hr = (document.vtable().base__.GetSelection)(document.as_raw(), &mut raw);
        diagnostic.hr = Some(hr.0);
        let selected = (!raw.is_null()).then(|| ITextSelection::from_raw(raw));
        let selected = selected.filter(|_| hr == S_OK)?;
        let selected_range: ITextRange = selected.cast().ok()?;
        let start = scalar(
            selected_range.as_raw(),
            selected_range.vtable().GetStart,
            "tom_start",
            diagnostic,
        )?;
        diagnostic.tom_start = Some(start);
        let end = scalar(
            selected_range.as_raw(),
            selected_range.vtable().GetEnd,
            "tom_end",
            diagnostic,
        )?;
        diagnostic.tom_end = Some(end);
        diagnostic.stage = "tsf_acp";
        let mut selection = [TF_SELECTION::default()];
        let mut fetched = 0;
        let result = context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched);
        let current_range = ManuallyDrop::take(&mut selection[0].range);
        if let Err(error) = result {
            diagnostic.hr = Some(error.code().0);
            return None;
        }
        let current_range = current_range.filter(|_| fetched == 1)?;
        if current_range.CompareStart(ec, range, TF_ANCHOR_START).ok() != Some(0)
            || current_range.CompareEnd(ec, range, TF_ANCHOR_END).ok() != Some(0)
        {
            return None;
        }
        let acp: ITfRangeACP = range.cast().ok()?;
        let mut tsf_start = 0;
        let mut length = 0;
        let hr = (acp.vtable().GetExtent)(acp.as_raw(), &mut tsf_start, &mut length);
        diagnostic.hr = Some(hr.0);
        let tsf_end = tsf_start.checked_add(length)?;
        diagnostic.tsf_start = Some(tsf_start);
        diagnostic.tsf_end = Some(tsf_end);
        if hr != S_OK || length <= 0 || start != tsf_start || end != tsf_end {
            return None;
        }
        let story = scalar(
            selected_range.as_raw(),
            selected_range.vtable().GetStoryType,
            "tom_story",
            diagnostic,
        )?;
        diagnostic.story = Some(story);
        let selection_type = scalar(
            selected.as_raw(),
            selected.vtable().GetType,
            "tom_selection_type",
            diagnostic,
        )?;
        diagnostic.selection_type = Some(selection_type);
        let can_edit = scalar(
            selected_range.as_raw(),
            selected_range.vtable().CanEdit,
            "tom_can_edit",
            diagnostic,
        )?;
        diagnostic.can_edit = Some(can_edit);
        diagnostic.stage = "tom_font";
        let mut raw = std::ptr::null_mut();
        let hr = (selected_range.vtable().GetFont)(selected_range.as_raw(), &mut raw);
        diagnostic.hr = Some(hr.0);
        let font = (!raw.is_null()).then(|| ITextFont::from_raw(raw));
        let font = font.filter(|_| hr == S_OK)?;
        let hidden = scalar(
            font.as_raw(),
            font.vtable().GetHidden,
            "tom_hidden",
            diagnostic,
        )?;
        diagnostic.hidden = Some(hidden);
        let protected = scalar(
            font.as_raw(),
            font.vtable().GetProtected,
            "tom_protected",
            diagnostic,
        )?;
        diagnostic.protected = Some(protected);
        // The COM getters may reenter their host. Recheck identity after them,
        // before this evidence can authorize reading/dispatching the selection.
        diagnostic.stage = "final_focus";
        let mut current = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut current).is_err()
            || current.hwndFocus != focus
            || GetForegroundWindow() != root
            || !manager?
                .GetFocus()
                .and_then(|document| document.GetTop())
                .is_ok_and(|focused| focused == *context)
        {
            return None;
        }
        diagnostic.stage = "verified_native_plain_selection";
        Some(Evidence {
            context_matches: diagnostic.context_matches,
            same_thread_process: diagnostic.same_thread_process,
            visible: diagnostic.visible,
            enabled: diagnostic.enabled,
            view_related: diagnostic.view_related,
            foreground_matches: diagnostic.foreground_matches,
            class: diagnostic.class,
            style: diagnostic.style,
            password_char: password,
            tom_window_matches: window_matches,
            tsf_start,
            tsf_end,
            tom_start: start,
            tom_end: end,
            story,
            selection_type,
            can_edit,
            hidden,
            protected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_failed_empty_scope_enters_native_policy() {
        assert!(eligible(E_FAIL.0, VT_EMPTY.0));
        for (hr, vt) in [
            (S_OK.0, VT_EMPTY.0),
            (S_FALSE.0, VT_EMPTY.0),
            (E_NOTIMPL.0, VT_EMPTY.0),
            (E_FAIL.0, 13),
        ] {
            assert!(!eligible(hr, vt));
        }
    }
    #[test]
    fn native_plain_policy_requires_all_independent_identity_and_format_evidence() {
        let valid = Evidence {
            context_matches: true,
            same_thread_process: true,
            visible: true,
            enabled: true,
            view_related: true,
            foreground_matches: true,
            class: 4,
            style: 0,
            password_char: 0,
            tom_window_matches: true,
            tsf_start: 3,
            tsf_end: 30,
            tom_start: 3,
            tom_end: 30,
            story: tomMainTextStory.0,
            selection_type: tomSelectionNormal.0,
            can_edit: tomTrue.0,
            hidden: tomFalse.0,
            protected: tomFalse.0,
        };
        assert!(valid.permits());
        let changes: Vec<Box<dyn Fn(&mut Evidence)>> = vec![
            Box::new(|e| e.context_matches = false),
            Box::new(|e| e.same_thread_process = false),
            Box::new(|e| e.visible = false),
            Box::new(|e| e.enabled = false),
            Box::new(|e| e.view_related = false),
            Box::new(|e| e.foreground_matches = false),
            Box::new(|e| e.class = 0),
            Box::new(|e| e.style = ES_PASSWORD as u32),
            Box::new(|e| e.style = ES_READONLY as u32),
            Box::new(|e| e.password_char = 42),
            Box::new(|e| e.tom_window_matches = false),
            Box::new(|e| e.tsf_start = -1),
            Box::new(|e| e.tsf_end = e.tsf_start),
            Box::new(|e| e.tom_start += 1),
            Box::new(|e| e.tom_end -= 1),
            Box::new(|e| e.story = 0),
            Box::new(|e| e.selection_type = tomSelectionBlock.0),
            Box::new(|e| e.can_edit = tomFalse.0),
            Box::new(|e| e.hidden = tomTrue.0),
            Box::new(|e| e.hidden = tomUndefined.0),
            Box::new(|e| e.protected = tomTrue.0),
            Box::new(|e| e.protected = tomUndefined.0),
        ];
        for change in changes {
            let mut evidence = valid.clone();
            change(&mut evidence);
            assert!(!evidence.permits());
        }
    }
    #[test]
    fn known_control_class_is_only_a_category_and_unknown_names_are_not_admitted() {
        assert_eq!(
            class_category(&"RichEditD2DPT".encode_utf16().collect::<Vec<_>>()),
            4
        );
        assert_eq!(
            class_category(&"RICHEDIT50W".encode_utf16().collect::<Vec<_>>()),
            2
        );
        assert_eq!(
            class_category(&"Notepad".encode_utf16().collect::<Vec<_>>()),
            0
        );
        assert_eq!(
            class_category(&"Edit".encode_utf16().collect::<Vec<_>>()),
            0
        );
        assert_eq!(class_category(&[0xd800]), 0);
    }
    #[test]
    fn tom_scalar_s_false_cannot_be_mistaken_for_a_successful_value() {
        unsafe extern "system" fn false_getter(_: *mut c_void, value: *mut i32) -> HRESULT {
            unsafe {
                *value = tomTrue.0;
            }
            S_FALSE
        }
        let mut diagnostic = Diagnostic::default();
        // SAFETY: synthetic getter ignores the object and writes its valid out pointer.
        assert_eq!(
            unsafe {
                scalar(
                    std::ptr::null_mut(),
                    false_getter,
                    "fixture",
                    &mut diagnostic,
                )
            },
            None
        );
        assert_eq!(diagnostic.hr, Some(S_FALSE.0));
        unsafe extern "system" fn unwritten_getter(_: *mut c_void, _: *mut i32) -> HRESULT {
            S_OK
        }
        // SAFETY: synthetic getter ignores both pointers; an untouched sentinel is rejected.
        assert_eq!(
            unsafe {
                scalar(
                    std::ptr::null_mut(),
                    unwritten_getter,
                    "fixture",
                    &mut diagnostic,
                )
            },
            None
        );
    }
}
