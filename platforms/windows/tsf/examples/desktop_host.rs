//! Standalone, empty RichEdit host for installed-profile and typing acceptance.
#![windows_subsystem = "windows"]
#![allow(unsafe_code)]
use std::{
    cell::RefCell,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::*,
    System::{Com::*, LibraryLoader::*},
    UI::{TextServices::*, WindowsAndMessaging::*},
};
use windows_core::{w, Interface, Result, HRESULT, PCWSTR};

#[derive(Default)]
struct Host {
    passive: bool,
    manager: Option<ITfThreadMgr>,
    profiles: Option<ITfInputProcessorProfileMgr>,
    languages: Option<ITfInputProcessorProfiles>,
    original_language: Option<u16>,
    mode_item: Option<ITfLangBarItemButton>,
    requested: Option<Instant>,
    last_profile: Option<(i32, TF_INPUTPROCESSORPROFILE)>,
    last_context_status: Option<std::result::Result<(u32, u32), i32>>,
}
thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host::default());
    static LOG: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}
fn event(message: &str) {
    let path = LOG.with(|p| p.borrow().clone());
    if let Some(path) = path {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{message}");
        }
    }
}
fn text(window: HWND, value: &str) {
    let units: Vec<u16> = value.encode_utf16().chain([0]).collect();
    unsafe {
        let _ = SetWindowTextW(window, PCWSTR(units.as_ptr()));
    }
}
fn status(window: HWND, value: &str) {
    if let Ok(label) = unsafe { GetDlgItem(Some(window), 2) } {
        text(label, value);
    }
}
fn describe(profile: &TF_INPUTPROCESSORPROFILE) -> String {
    format!(
        "type={} lang={:04X} clsid={:?} profile={:?} category={:?} caps={:08X} flags={:08X}",
        profile.dwProfileType,
        profile.langid,
        profile.clsid,
        profile.guidProfile,
        profile.catid,
        profile.dwCaps,
        profile.dwFlags
    )
}
fn expected(profile: &TF_INPUTPROCESSORPROFILE) -> bool {
    profile.dwProfileType == TF_PROFILETYPE_INPUTPROCESSOR
        && profile.clsid == retype_ime::ids::CLSID_RETYPE_TIP
        && profile.guidProfile == retype_ime::ids::GUID_PROFILE_ENGLISH
        && profile.langid == retype_ime::ids::LANGID_EN_US
}
unsafe fn raw_profile(
    profiles: &ITfInputProcessorProfileMgr,
) -> (HRESULT, TF_INPUTPROCESSORPROFILE) {
    let mut value = TF_INPUTPROCESSORPROFILE::default();
    let hr = unsafe {
        (Interface::vtable(profiles).GetProfile)(
            profiles.as_raw(),
            TF_PROFILETYPE_INPUTPROCESSOR,
            retype_ime::ids::LANGID_EN_US,
            &retype_ime::ids::CLSID_RETYPE_TIP,
            &retype_ime::ids::GUID_PROFILE_ENGLISH,
            windows::Win32::UI::Input::KeyboardAndMouse::HKL::default(),
            &mut value,
        )
    };
    (hr, value)
}
fn request_activation(window: HWND) {
    if HOST.with(|h| h.borrow().passive) {
        // This mode observes only the user's normal system selection. Keep the
        // button and every startup path outside all language/profile writes.
        refresh(window);
        return;
    }
    let interfaces = HOST.with(|h| {
        let h = h.borrow();
        Some((
            h.manager.clone()?,
            h.profiles.clone()?,
            h.languages.clone()?,
        ))
    });
    let Some((manager, profiles, languages)) = interfaces else {
        return;
    };
    unsafe {
        if GetForegroundWindow() != window {
            status(window, "请先激活此窗口，再点击“重新验证激活”。");
            event("activation_not_requested: host is not foreground");
            return;
        }
        if let Ok(edit) = GetDlgItem(Some(window), 1) {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(edit));
        }
        let context = manager.GetFocus().and_then(|d| d.GetTop());
        let context_status = context
            .as_ref()
            .map_err(|error| error.code())
            .and_then(|context| {
                context
                    .GetStatus()
                    .map(|status| (status.dwStaticFlags, status.dwDynamicFlags))
                    .map_err(|error| error.code())
            });
        event(&format!(
            "focused_context_status_static_dynamic={context_status:?}"
        ));
        event(&format!(
            "focused_text_context={} thread_flags={:?}",
            context.is_ok(),
            manager
                .cast::<ITfThreadMgrEx>()
                .and_then(|ex| ex.GetActiveFlags())
        ));
        let enabled = languages.IsEnabledLanguageProfile(
            &retype_ime::ids::CLSID_RETYPE_TIP,
            retype_ime::ids::LANGID_EN_US,
            &retype_ime::ids::GUID_PROFILE_ENGLISH,
        );
        event(&format!("legacy_enabled={enabled:?}"));
        if let Err(error) = languages.ChangeCurrentLanguage(retype_ime::ids::LANGID_EN_US) {
            event(&format!(
                "change_language_error={:08X}",
                error.code().0 as u32
            ));
            status(window, "无法切换此进程到英语，请查看诊断日志。");
            return;
        }
        event(&format!(
            "current_language={:?}",
            languages.GetCurrentLanguage()
        ));
        let (hr, value) = raw_profile(&profiles);
        event(&format!(
            "get_profile_raw={:08X} {}",
            hr.0 as u32,
            describe(&value)
        ));
        // Preserve S_FALSE. The generated Result<()> binding otherwise hides it.
        let hr = (Interface::vtable(&profiles).ActivateProfile)(
            profiles.as_raw(),
            TF_PROFILETYPE_INPUTPROCESSOR,
            retype_ime::ids::LANGID_EN_US,
            &retype_ime::ids::CLSID_RETYPE_TIP,
            &retype_ime::ids::GUID_PROFILE_ENGLISH,
            windows::Win32::UI::Input::KeyboardAndMouse::HKL::default(),
            TF_IPPMF_FORPROCESS,
        );
        event(&format!(
            "activate_profile_raw={:08X} flags=FORPROCESS (no ENABLEPROFILE)",
            hr.0 as u32
        ));
        if hr != HRESULT(0) {
            status(
                window,
                &format!(
                    "Profile 激活请求未成功（0x{:08X}），当前不可判定输入法生效。",
                    hr.0 as u32
                ),
            );
            text(window, "English Assist P0 — 未激活");
            HOST.with(|h| h.borrow_mut().requested = None);
        } else {
            status(
                window,
                "激活请求已返回，正在核对实际 Profile；请勿把普通键盘输入当成补词成功。",
            );
            HOST.with(|h| h.borrow_mut().requested = Some(Instant::now()));
        }
    }
}
fn refresh(window: HWND) {
    let interfaces = HOST.with(|h| {
        let h = h.borrow();
        Some((h.manager.clone()?, h.profiles.clone()?))
    });
    let Some((manager, profiles)) = interfaces else {
        return;
    };
    unsafe {
        let mut active = TF_INPUTPROCESSORPROFILE::default();
        let hr = (Interface::vtable(&profiles).GetActiveProfile)(
            profiles.as_raw(),
            &GUID_TFCAT_TIP_KEYBOARD,
            &mut active,
        );
        let context_status = manager
            .GetFocus()
            .and_then(|document| document.GetTop())
            .and_then(|context| context.GetStatus())
            .map(|status| (status.dwStaticFlags, status.dwDynamicFlags))
            .map_err(|error| error.code().0);
        let changed = HOST.with(|h| {
            let mut h = h.borrow_mut();
            if h.last_profile == Some((hr.0, active))
                && h.last_context_status == Some(context_status)
            {
                false
            } else {
                h.last_profile = Some((hr.0, active));
                h.last_context_status = Some(context_status);
                true
            }
        });
        if changed {
            event(&format!(
                "active_profile_raw={:08X} {}",
                hr.0 as u32,
                describe(&active)
            ));
            event(&format!(
                "focused_context_status_static_dynamic={context_status:?}"
            ));
        }
        if HOST.with(|h| h.borrow().passive) {
            if changed {
                let selection = if hr != HRESULT(0) {
                    "其他键盘（读取失败）"
                } else if expected(&active) {
                    "英语助手"
                } else if active.dwProfileType == TF_PROFILETYPE_KEYBOARDLAYOUT
                    && active.langid == retype_ime::ids::LANGID_EN_US
                    && matches!(active.hkl.0 as usize as u32, 0x04090409 | 0x00000409)
                {
                    "美式键盘"
                } else {
                    "其他键盘"
                };
                text(window, &format!("系统键盘选择测试 — {selection}"));
                status(
                    window,
                    &format!("当前：{selection}；通过系统键盘列表切换，此窗口仅观察。"),
                );
            }
            return;
        }
        if hr == HRESULT(0) && expected(&active) {
            status(
                window,
                "英语 Profile 已确认激活；现在测试本地候选和按键提交。",
            );
            text(
                window,
                "English Assist P0 — 英语 Profile 已激活，待输入验收",
            );
            let absent = HOST.with(|h| h.borrow().mode_item.is_none());
            if absent {
                // A language-bar item is diagnostic only, not a prerequisite for
                // showing the real editing surface or checking the active profile.
                let item = manager
                    .cast::<ITfLangBarItemMgr>()
                    .and_then(|items| items.GetItem(&GUID_LBI_INPUTMODE))
                    .and_then(|item| item.cast::<ITfLangBarItemButton>());
                event(&format!("language_bar_item_available={}", item.is_ok()));
                if let Ok(item) = item {
                    HOST.with(|h| h.borrow_mut().mode_item = Some(item));
                }
            }
        } else {
            let timed_out = HOST.with(|h| {
                h.borrow()
                    .requested
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
            });
            if timed_out {
                status(
                    window,
                    "验证失败：当前仍是其他 Profile / 普通键盘；请查看诊断日志并重新验证。",
                );
                text(window, "English Assist P0 — Profile 未激活");
                HOST.with(|h| h.borrow_mut().requested = None);
                event("activation_timeout: expected CLSID/Profile was not active");
            }
        }
    }
}
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_COMMAND if wp.0 as u32 & 0xffff == 3 => {
                request_activation(window);
                return LRESULT(0);
            }
            WM_TIMER => {
                refresh(window);
                return LRESULT(0);
            }
            WM_SIZE => {
                let width = (lp.0 & 0xffff) as i32;
                let height = ((lp.0 >> 16) & 0xffff) as i32;
                if let Ok(edit) = GetDlgItem(Some(window), 1) {
                    let _ = MoveWindow(
                        edit,
                        12,
                        48,
                        (width - 24).max(1),
                        (height - 60).max(1),
                        true,
                    );
                }
                if let Ok(label) = GetDlgItem(Some(window), 2) {
                    let _ = MoveWindow(label, 12, 12, (width - 208).max(1), 26, true);
                }
                if let Ok(button) = GetDlgItem(Some(window), 3) {
                    let _ = MoveWindow(button, (width - 184).max(12), 8, 172, 32, true);
                }
            }
            WM_DESTROY => {
                let _ = KillTimer(Some(window), 1);
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(window, message, wp, lp)
    }
}
unsafe fn pump() {
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            if message.message == WM_QUIT {
                PostQuitMessage(message.wParam.0 as i32);
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
#[allow(clippy::manual_dangling_ptr)]
fn run(passive: bool) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let result = (|| -> Result<()> {
            let module = GetModuleHandleW(None)?;
            let _rich_edit = LoadLibraryW(w!("Msftedit.dll"))?;
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module.into(),
                lpszClassName: w!("EnglishAssistP0Host"),
                ..Default::default()
            });
            let window = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EnglishAssistP0Host"),
                if passive {
                    w!("系统键盘选择测试 — 正在读取")
                } else {
                    w!("English Assist P0 — 等待激活验证")
                },
                WS_OVERLAPPEDWINDOW,
                150,
                150,
                1050,
                550,
                None,
                None,
                Some(module.into()),
                None,
            )?;
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("正在建立真实 RichEdit 文本上下文…"),
                WS_CHILD | WS_VISIBLE,
                12,
                12,
                820,
                26,
                Some(window),
                Some(HMENU(2usize as *mut _)),
                Some(module.into()),
                None,
            )?;
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("BUTTON"),
                if passive {
                    w!("刷新系统选择")
                } else {
                    w!("重新验证激活")
                },
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                840,
                8,
                172,
                32,
                Some(window),
                Some(HMENU(3usize as *mut _)),
                Some(module.into()),
                None,
            )?;
            let edit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                w!("RICHEDIT50W"),
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32),
                12,
                48,
                1000,
                440,
                Some(window),
                Some(HMENU(1usize as *mut _)),
                Some(module.into()),
                None,
            )?;
            let manager: ITfThreadMgr =
                CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
            let tid = manager.Activate()?;
            let profiles: ITfInputProcessorProfileMgr =
                CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
            let languages: ITfInputProcessorProfiles = profiles.cast()?;
            // A passive host must not restore a saved language on exit either:
            // the user's selection while testing is theirs to retain.
            let original_language = if passive {
                None
            } else {
                languages.GetCurrentLanguage().ok()
            };
            HOST.with(|h| {
                let mut h = h.borrow_mut();
                h.passive = passive;
                h.manager = Some(manager.clone());
                h.profiles = Some(profiles);
                h.languages = Some(languages);
                h.original_language = original_language;
            });
            event(&format!(
                "thread_activated client_id={tid} passive={passive} original_language={original_language:?}"
            ));
            let _ = ShowWindow(window, SW_SHOW);
            let _ = SetForegroundWindow(window);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(edit));
            let until = Instant::now() + Duration::from_millis(250);
            while Instant::now() < until && IsWindow(Some(window)).as_bool() {
                pump();
                std::thread::sleep(Duration::from_millis(10));
            }
            if !IsWindow(Some(window)).as_bool() {
                return Ok(());
            }
            request_activation(window);
            SetTimer(Some(window), 1, 100, None);
            let keys: ITfKeystrokeMgr = manager.cast()?;
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let eaten = if message.message == WM_KEYDOWN {
                    let tested = keys.TestKeyDown(message.wParam, message.lParam);
                    let handled = tested
                        .as_ref()
                        .is_ok_and(|value| value.as_bool())
                        .then(|| keys.KeyDown(message.wParam, message.lParam));
                    event(&format!("key_down tested={tested:?} handled={handled:?}"));
                    handled
                        .as_ref()
                        .is_some_and(|result| result.as_ref().is_ok_and(|value| value.as_bool()))
                } else if message.message == WM_KEYUP {
                    let tested = keys.TestKeyUp(message.wParam, message.lParam);
                    let handled = tested
                        .as_ref()
                        .is_ok_and(|value| value.as_bool())
                        .then(|| keys.KeyUp(message.wParam, message.lParam));
                    event(&format!("key_up tested={tested:?} handled={handled:?}"));
                    handled
                        .as_ref()
                        .is_some_and(|result| result.as_ref().is_ok_and(|value| value.as_bool()))
                } else {
                    false
                };
                if eaten {
                    continue;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            Ok(())
        })();
        event(&format!(
            "host_result={:?}",
            result.as_ref().err().map(|e| e.code())
        ));
        let host = HOST.with(|h| std::mem::take(&mut *h.borrow_mut()));
        if !host.passive {
            if let (Some(languages), Some(language)) = (&host.languages, host.original_language) {
                let _ = languages.ChangeCurrentLanguage(language);
            }
        }
        if let Some(manager) = &host.manager {
            let _ = manager.Deactivate();
        }
        drop(host);
        CoUninitialize();
        result
    }
}
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut path = std::env::current_dir()
        .map_err(|_| E_FAIL)?
        .join("work/desktop-host-diagnostics.log");
    let mut passive = false;
    while let Some(arg) = args.next() {
        if arg == "--diagnostics" {
            path = args.next().map(PathBuf::from).ok_or(E_INVALIDARG)?;
        } else if arg == "--passive" {
            passive = true;
        } else {
            return Err(E_INVALIDARG.into());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| E_FAIL)?;
    }
    std::fs::File::create(&path).map_err(|_| E_FAIL)?;
    LOG.with(|p| *p.borrow_mut() = Some(path));
    event(&format!(
        "English Assist P0 host pid={} pointer_bits={} passive={passive}",
        std::process::id(),
        usize::BITS
    ));
    run(passive)
}
