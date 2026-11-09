//! Small per-user preferences shared by the 32/64-bit TIPs.
use retype_types::PinyinScheme;
use windows::Win32::System::Registry::*;
use windows_core::{w, Result};

pub fn candidate_appearance() -> retype_types::CandidateAppearance {
    use retype_types::{CandidateAppearance, CandidateTheme};
    use std::time::{Duration, Instant};
    thread_local! {
        static APPEARANCE: std::cell::RefCell<Option<(Instant, CandidateAppearance)>> = const { std::cell::RefCell::new(None) };
    }
    APPEARANCE.with(|cache| {
        if let Some((when, value)) = *cache.borrow() {
            if when.elapsed() < Duration::from_millis(500) {
                return value;
            }
        }
        let defaults = CandidateAppearance::default();
        let mut theme = match read_dword(w!("CandidateTheme")) {
            1 => CandidateTheme::Light,
            2 => CandidateTheme::Dark,
            _ => CandidateTheme::System,
        };
        if theme == CandidateTheme::System {
            let mut light = 1u32;
            let mut bytes = 4;
            // SAFETY: Read only the Windows app-theme preference, with a DWORD output.
            unsafe {
                let _ = RegGetValueW(
                    HKEY_CURRENT_USER,
                    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                    w!("AppsUseLightTheme"),
                    RRF_RT_REG_DWORD,
                    None,
                    Some((&mut light as *mut u32).cast()),
                    Some(&mut bytes),
                );
            }
            theme = if light == 0 {
                CandidateTheme::Dark
            } else {
                CandidateTheme::Light
            };
        }
        let font = read_dword(w!("CandidateFontSize"));
        let width = read_dword(w!("CandidateWidth"));
        let appearance = CandidateAppearance {
            theme,
            font_size: if font == 0 { defaults.font_size } else { font },
            width: if width == 0 { defaults.width } else { width },
            compact: read_dword(w!("CandidateCompact")) != 0,
            show_meanings: read_dword(w!("CandidateHideMeanings")) == 0,
            show_part_of_speech: read_dword(w!("CandidateHidePartOfSpeech")) == 0,
            show_phonetics: read_dword(w!("CandidateShowPhonetics")) != 0,
            show_key_hints: read_dword(w!("CandidateHideKeyHints")) == 0,
        }
        .normalized();
        *cache.borrow_mut() = Some((Instant::now(), appearance));
        appearance
    })
}

pub fn scheme() -> PinyinScheme {
    let mut value = 0u32;
    let mut size = 4;
    // SAFETY: DWORD output buffer has the advertised size. Missing/invalid values use full pinyin.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\EnglishAssist"),
            w!("PinyinScheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if result.is_ok() && value == 1 {
        PinyinScheme::Flypy
    } else {
        PinyinScheme::Full
    }
}

pub fn english_options() -> (bool, bool) {
    // Disabled flags default to zero, so existing installations gain completion.
    (
        read_dword(w!("EnglishDisabled")) == 0,
        read_dword(w!("EnglishSpellingDisabled")) == 0,
    )
}

pub fn english_pinyin() -> bool {
    read_dword(w!("EnglishPinyinDisabled")) == 0
}

pub fn english_continuous() -> (bool, bool) {
    (
        read_dword(w!("EnglishSegmentationDisabled")) == 0,
        read_dword(w!("EnglishPhrasesDisabled")) == 0,
    )
}

pub fn manual_words_generation() -> u32 {
    read_dword(w!("ManualWordsGeneration"))
}

pub fn manual_words_path() -> Option<std::path::PathBuf> {
    let mut buffer = [0u16; 32768];
    let mut bytes = (buffer.len() * 2) as u32;
    // SAFETY: Read only the current user's explicitly published dictionary path.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\EnglishAssist"),
            w!("ManualWordsPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if result.is_err() || bytes < 2 || bytes as usize > buffer.len() * 2 {
        return None;
    }
    let path =
        std::path::PathBuf::from(String::from_utf16_lossy(&buffer[..bytes as usize / 2 - 1]));
    path.is_absolute().then_some(path)
}

pub fn symbol_completion() -> bool {
    #[cfg(test)]
    if let Some(enabled) = TEST_SYMBOL_COMPLETION.with(std::cell::Cell::get) {
        return enabled;
    }
    read_dword(w!("SymbolCompletionEnabled")) != 0
}

#[cfg(test)]
thread_local! {
    static TEST_SYMBOL_COMPLETION: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Thread-local fixture preference: never reads or changes the user's registry.
#[cfg(test)]
pub(crate) struct SymbolCompletionForTest(Option<bool>);
#[cfg(test)]
impl Drop for SymbolCompletionForTest {
    fn drop(&mut self) {
        TEST_SYMBOL_COMPLETION.with(|value| value.set(self.0));
    }
}
#[cfg(test)]
pub(crate) fn symbol_completion_for_test(enabled: bool) -> SymbolCompletionForTest {
    SymbolCompletionForTest(TEST_SYMBOL_COMPLETION.with(|value| value.replace(Some(enabled))))
}

fn read_dword(name: windows_core::PCWSTR) -> u32 {
    let mut value = 0u32;
    let mut size = 4;
    // SAFETY: The output buffer is a DWORD and failures leave the default zero.
    unsafe {
        let _ = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\EnglishAssist"),
            name,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        );
    }
    value
}

pub fn pack_generation() -> u32 {
    read_dword(w!("DictionaryGeneration"))
}

pub fn enabled_packs() -> u32 {
    read_dword(w!("EnabledDictionaryPacks"))
}

pub fn pack_root() -> Option<std::path::PathBuf> {
    let mut buffer = [0u16; 32768];
    let mut bytes = (buffer.len() * 2) as u32;
    // SAFETY: Fixed UTF-16 output buffer, read-only HKCU value.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\EnglishAssist"),
            w!("DictionaryRoot"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if result.is_err() {
        return None;
    }
    let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    if length == 0 {
        None
    } else {
        Some(std::path::PathBuf::from(String::from_utf16_lossy(
            &buffer[..length],
        )))
    }
}
/// Open the installed settings app without loading a UI runtime into the TSF host.
pub fn open_settings() -> Result<()> {
    let request = crate::settings_log::request_id();
    let host = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    crate::settings_log::event(
        "launcher",
        "open_requested",
        request,
        format!("host={host}"),
    );
    // Process creation and authenticated pipe IO belong on a worker, never in
    // SearchHost's TSF callback. Queueing also keeps the menu responsive.
    std::thread::Builder::new()
        .name("retype-settings-open".into())
        .spawn(move || finish_open_settings(request))
        .map(|_| ())
        .map_err(|error| {
            windows_core::Error::from_hresult(windows_core::HRESULT::from_win32(
                error.raw_os_error().unwrap_or(8) as u32,
            ))
        })
}

fn finish_open_settings(request: u32) {
    let started = std::time::Instant::now();
    // A restricted child can sometimes be created successfully but inherit the
    // sandbox and then fail to initialize Settings. Delegate before attempting
    // either process creation or cross-integrity window reuse in that case.
    let restricted = retype_learning::transport::is_app_container().unwrap_or(true);
    crate::settings_log::event(
        "launcher",
        "launch_context",
        request,
        format!("app_container={restricted}"),
    );
    let direct = if restricted {
        Err(windows::Win32::Foundation::E_ACCESSDENIED.into())
    } else {
        open_settings_inner(request)
    };
    let result = direct.or_else(|direct_error| {
        crate::settings_log::event(
            "launcher",
            "broker_open_begin",
            request,
            format!("direct_hresult={}", direct_error.code().0),
        );
        retype_learning::settings::open(request).map_err(|error| {
            crate::settings_log::event(
                "launcher",
                "broker_open_failed",
                request,
                format!(
                    "kind={:?} win32={}",
                    error.kind(),
                    error.raw_os_error().unwrap_or(0)
                ),
            );
            windows_core::Error::from_hresult(windows_core::HRESULT::from_win32(
                error.raw_os_error().unwrap_or(5) as u32,
            ))
        })
    });
    crate::settings_log::event(
        "launcher",
        "open_return",
        request,
        format!(
            "success={} hresult={} elapsed_ms={}",
            result.is_ok(),
            result.as_ref().err().map_or(0, |e| e.code().0),
            started.elapsed().as_millis()
        ),
    );
    crate::settings_log::flush();
}
fn open_settings_inner(request: u32) -> Result<()> {
    let mut buffer = [0u16; 32768];
    let mut bytes = (buffer.len() * 2) as u32;
    // SAFETY: Fixed UTF-16 buffer; the x86 TIP must read the x64 installation view.
    unsafe {
        let read = RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("Software\\EnglishAssist"),
            w!("ActiveDir"),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        );
        crate::settings_log::event(
            "launcher",
            "active_dir_lookup",
            request,
            format!("win32={}", read.0),
        );
        read.ok()?;
    }
    let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    let directory = std::path::PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
    if length == 0 || !directory.is_absolute() {
        crate::settings_log::event(
            "launcher",
            "active_dir_invalid",
            request,
            "missing or non-absolute directory",
        );
        return Err(windows::Win32::Foundation::E_INVALIDARG.into());
    }
    let path = directory.join("settings").join("retype.exe");
    crate::settings_log::event(
        "launcher",
        "target_ready",
        request,
        format!("exists={}", path.is_file()),
    );
    let expected: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
    // Reuse a live settings window only when it belongs to the active install.
    // This avoids loading a second settings process merely to raise the first.
    unsafe {
        use windows::Win32::System::Threading::*;
        use windows::Win32::UI::WindowsAndMessaging::*;
        let mut previous = None;
        while let Ok(window) = FindWindowExW(
            None,
            previous,
            w!("EnglishAssist.Settings.Command"),
            w!("English Assist 英文助手设置"),
        ) {
            previous = Some(window);
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(process) => {
                    let mut executable = [0u16; 32768];
                    let mut len = executable.len() as u32;
                    let found = QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_WIN32,
                        windows_core::PWSTR(executable.as_mut_ptr()),
                        &mut len,
                    )
                    .is_ok();
                    let _ = windows::Win32::Foundation::CloseHandle(process);
                    let matches = found
                        && windows::Win32::Globalization::CompareStringOrdinal(
                            &expected,
                            &executable[..len as usize],
                            true,
                        ) == windows::Win32::Globalization::CSTR_EQUAL;
                    crate::settings_log::event(
                        "launcher",
                        "reuse_candidate",
                        request,
                        format!("target_pid={pid} image_query={found} path_match={matches}"),
                    );
                    if matches {
                        let allowed = AllowSetForegroundWindow(pid);
                        // Shared with the settings app; reactivate its retained window.
                        let posted = PostMessageW(
                            Some(window),
                            WM_APP + 29,
                            windows::Win32::Foundation::WPARAM(request as usize),
                            windows::Win32::Foundation::LPARAM(0),
                        );
                        crate::settings_log::event(
                            "launcher",
                            "reuse_post",
                            request,
                            format!(
                                "target_pid={pid} success={} foreground_allowed={} hresult={}",
                                posted.is_ok(),
                                allowed.is_ok(),
                                posted.as_ref().err().map_or(0, |e| e.code().0)
                            ),
                        );
                        if posted.is_ok() {
                            return Ok(());
                        }
                        break;
                    }
                }
                Err(error) => crate::settings_log::event(
                    "launcher",
                    "reuse_query_failed",
                    request,
                    format!("target_pid={pid} hresult={}", error.code().0),
                ),
            }
        }
    }
    crate::settings_log::event("launcher", "spawn_begin", request, "starting settings");
    let child = std::process::Command::new(path)
        .arg(format!("--request-id={request}"))
        .spawn()
        .map_err(|error| {
            crate::settings_log::event(
                "launcher",
                "spawn_failed",
                request,
                format!(
                    "kind={:?} win32={}",
                    error.kind(),
                    error.raw_os_error().unwrap_or(0)
                ),
            );
            windows_core::Error::from_hresult(windows_core::HRESULT::from_win32(
                error.raw_os_error().unwrap_or(1) as u32,
            ))
        })?;
    crate::settings_log::event(
        "launcher",
        "spawn_ok",
        request,
        format!("child_pid={}", child.id()),
    );
    let child_pid = child.id();
    if std::thread::Builder::new()
        .name("retype-settings-watch".into())
        .spawn(move || {
            let mut child = child;
            let started = std::time::Instant::now();
            match child.wait() {
                Ok(status) => crate::settings_log::event(
                    "launcher",
                    "child_exit",
                    request,
                    format!(
                        "child_pid={child_pid} code={} lifetime_ms={}",
                        status.code().unwrap_or(-1),
                        started.elapsed().as_millis()
                    ),
                ),
                Err(error) => crate::settings_log::event(
                    "launcher",
                    "child_wait_failed",
                    request,
                    format!(
                        "child_pid={child_pid} win32={}",
                        error.raw_os_error().unwrap_or(0)
                    ),
                ),
            }
            crate::settings_log::flush();
        })
        .is_err()
    {
        crate::settings_log::event(
            "launcher",
            "child_watch_failed",
            request,
            format!("child_pid={child_pid}"),
        );
    }
    Ok(())
}
