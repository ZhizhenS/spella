//! Local word-prefix spacing. No remembered document text or trailing whitespace.
use crate::tip::{lock, TipState};
use std::{mem::ManuallyDrop, sync::Arc};
use windows::{
    core::Result,
    Win32::{
        Foundation::HWND,
        System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
        UI::{Input::KeyboardAndMouse::GetFocus, TextServices::*, WindowsAndMessaging::*},
    },
};

pub(crate) fn selection(context: &ITfContext, ec: u32) -> Result<ITfRange> {
    // SAFETY: caller holds this context's current edit cookie; consume COM output once.
    unsafe {
        let mut selections = [TF_SELECTION::default()];
        let mut count = 0;
        let result = context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selections, &mut count);
        let range = ManuallyDrop::take(&mut selections[0].range);
        result?;
        range
            .filter(|_| count == 1)
            .ok_or_else(|| windows::Win32::Foundation::E_FAIL.into())
    }
}

pub(crate) fn allowed(
    state: &TipState,
    context: &ITfContext,
    ec: u32,
    range: &ITfRange,
    selected: &ITfRange,
) -> bool {
    let manager = lock(&state.thread_mgr).clone();
    crate::translation::local_public_scope(context, ec, range, selected, manager.as_ref())
}

/// A status bar may be shown only for a fresh connected editable field. This is
/// metadata-only: neither the current selection nor its neighbors are read.
pub(crate) fn safe_focus(state: &TipState, context: &ITfContext, ec: u32) -> Option<HWND> {
    if state
        .session()
        .is_some_and(|session| session.backend.with_kernel(|kernel| kernel.is_chinese()))
    {
        return None;
    }
    let compartment = lock(&state.mode_bridge)
        .as_ref()
        .map(|bridge| bridge.compartment.clone());
    if compartment.is_some_and(|compartment| {
        // SAFETY: Read this service's current apartment-local open state only.
        unsafe { compartment.GetValue() }
            .ok()
            .and_then(|value| i32::try_from(&value).ok())
            == Some(0)
    }) {
        return None;
    }
    let selected = selection(context, ec).ok()?;
    if !allowed(state, context, ec, &selected, &selected) {
        return None;
    }
    let manager = lock(&state.thread_mgr).clone()?;
    // SAFETY: metadata reads on this same apartment; never synthesize window input.
    unsafe {
        if !manager
            .GetFocus()
            .and_then(|document| document.GetTop())
            .is_ok_and(|top| top == *context)
        {
            return None;
        }
        let focus = GetFocus();
        let mut pid = 0;
        if focus.is_invalid()
            || GetWindowThreadProcessId(focus, Some(&mut pid)) != GetCurrentThreadId()
            || pid != GetCurrentProcessId()
            || !IsWindowVisible(focus).as_bool()
            || !windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(focus).as_bool()
            || GetForegroundWindow() != GetAncestor(focus, GA_ROOT)
        {
            return None;
        }
        let view = context
            .GetActiveView()
            .and_then(|view| view.GetWnd())
            .ok()?;
        (view == focus || IsChild(view, focus).as_bool()).then_some(focus)
    }
}

pub(crate) fn decorate(state: &Arc<TipState>, context: &ITfContext, ec: u32, text: &str) -> String {
    let original = || text.to_owned();
    if !text
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return original();
    }
    let Ok(selected) = selection(context, ec) else {
        return original();
    };
    let composition = lock(&state.composition).clone();
    // SAFETY: read ranges only with this context's write cookie. A clone owns
    // its anchors; the document selection and composition are never moved.
    unsafe {
        let target = match composition {
            Some(composition) if composition.context == *context => match composition.range() {
                Ok(range) => range,
                Err(_) => return original(),
            },
            Some(_) => return original(),
            None => selected.clone(),
        };
        if !allowed(state, context, ec, &target, &selected) {
            return original();
        }
        let Ok(left) = target.Clone() else {
            return original();
        };
        if left.Collapse(ec, TF_ANCHOR_START).is_err() {
            return original();
        }
        let mut moved = 0;
        if left
            .ShiftStart(ec, -1, &mut moved, std::ptr::null())
            .is_err()
            || moved != -1
        {
            return original();
        }
        // Validate the neighboring range *before* its first GetText. A private
        // span beside an ordinary caret never becomes a surrounding-text read.
        if !allowed(state, context, ec, &left, &selected) {
            return original();
        }
        let mut utf16 = [0u16; 1];
        let mut count = 0;
        if left.GetText(ec, 0, &mut utf16, &mut count).is_err() || count != (-moved) as u32 {
            return original();
        }
        // Only the immediately preceding ASCII unit affects spacing. Surrogates
        // cannot be ASCII; an emoji immediately before a word is left unchanged.
        let previous = utf16[count as usize - 1];
        let needs =
            char::from_u32(previous as u32).is_some_and(|c| needs_prefix(&c.to_string(), text));
        if needs {
            format!(" {text}")
        } else {
            original()
        }
    }
}

fn needs_prefix(left: &str, text: &str) -> bool {
    if !text
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return false;
    }
    let Some(previous) = left.chars().next_back() else {
        return false;
    };
    previous.is_ascii_alphanumeric()
        || matches!(
            previous,
            ')' | ']' | '}' | ',' | '.' | ';' | ':' | '!' | '?'
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_only_preserves_whitespace_punctuation_and_connectors() {
        for left in ["i", "am", "42", ")", "]", "}", ".", ",", ";", ":", "!", "?"] {
            assert!(needs_prefix(left, "robot"), "{left:?}");
        }
        for left in [
            "", " ", "\t", "\n", "\r\n", "(", "[", "{", "'", "’", "\"", "-", "/", "\\", "_", "@",
        ] {
            assert!(!needs_prefix(left, "robot"), "{left:?}");
        }
        for text in ["", " robot", ".", ",", "'s", "-enter", "_value"] {
            assert!(!needs_prefix("word", text), "{text:?}");
        }
    }
}
