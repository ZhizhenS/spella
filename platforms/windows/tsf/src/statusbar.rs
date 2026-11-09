//! Apartment-owned, non-activating input status bar. No keyboard hooks or text reads.
use crate::tip::TipState;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::*, Threading::GetCurrentThreadId},
    UI::{HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
use windows_core::{w, Result, PCWSTR};

const TIMER: usize = 1;
const POLL_MS: u32 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Caps,
    Space,
    Words,
    Pinyin,
    Segmentation,
    Settings,
    Hide,
}
const ACTIONS: [Action; 7] = [
    Action::Caps,
    Action::Space,
    Action::Words,
    Action::Pinyin,
    Action::Segmentation,
    Action::Settings,
    Action::Hide,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
impl Bounds {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    fn rect(self) -> RECT {
        RECT {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }
}
struct Layout {
    width: i32,
    height: i32,
    cells: [Bounds; 7],
    help: Bounds,
}
fn px(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32
}
fn layout(dpi: u32) -> Layout {
    let widths = [44, 78, 44, 44, 44, 44, 26];
    let mut left = 22;
    let cells = std::array::from_fn(|i| {
        let cell = Bounds {
            left: px(left, dpi),
            top: px(8, dpi),
            right: px(left + widths[i], dpi),
            bottom: px(36, dpi),
        };
        left += widths[i] + 3;
        cell
    });
    Layout {
        width: px(left + 5, dpi),
        height: px(62, dpi),
        cells,
        help: Bounds {
            left: px(22, dpi),
            top: px(39, dpi),
            right: px(left, dpi),
            bottom: px(57, dpi),
        },
    }
}
fn hit_test(layout: &Layout, x: i32, y: i32) -> Option<Action> {
    layout
        .cells
        .iter()
        .position(|cell| cell.contains(x, y))
        .map(|i| ACTIONS[i])
}
fn clamp_position(x: i32, y: i32, width: i32, height: i32, work: RECT) -> (i32, i32) {
    (
        x.clamp(work.left, (work.right - width).max(work.left)),
        y.clamp(work.top, (work.bottom - height).max(work.top)),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    caps: bool,
    spacing: bool,
    words: bool,
    pinyin: bool,
    segmentation: bool,
    dark: bool,
}
impl Snapshot {
    fn read() -> Self {
        Self {
            // SAFETY: Read only the live toggle bit; never install a keyboard hook.
            caps: unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) & 1 != 0 },
            spacing: crate::preferences::auto_space(),
            words: crate::preferences::english_options().0,
            pinyin: crate::preferences::english_pinyin(),
            segmentation: crate::preferences::english_continuous().0,
            dark: crate::preferences::candidate_appearance().theme
                == retype_types::CandidateTheme::Dark,
        }
    }
    fn label(self, action: Action) -> &'static str {
        match action {
            Action::Caps => {
                if self.caps {
                    "大写"
                } else {
                    "小写"
                }
            }
            Action::Space => {
                if self.spacing {
                    "空格 开"
                } else {
                    "空格 关"
                }
            }
            Action::Words => "补词",
            Action::Pinyin => "拼音",
            Action::Segmentation => "连写",
            Action::Settings => "设置",
            Action::Hide => "×",
        }
    }
    fn enabled(self, action: Action) -> bool {
        match action {
            Action::Caps => self.caps,
            Action::Space => self.spacing,
            Action::Words => self.words,
            Action::Pinyin => self.pinyin,
            Action::Segmentation => self.segmentation,
            Action::Settings | Action::Hide => false,
        }
    }
}
struct Frame {
    tip: Weak<TipState>,
    editable: HWND,
    snapshot: Snapshot,
    dpi: u32,
    drag: Option<(POINT, POINT)>,
    pressed: Option<Action>,
    error: bool,
    positioned: bool,
    saved_position: Option<(i32, i32)>,
    label_set: bool,
    deferred_until: Option<Instant>,
}

pub(crate) struct StatusBar {
    window: HWND,
}
impl StatusBar {
    pub fn create(tip: &Arc<TipState>) -> Result<Self> {
        // SAFETY: The window and userdata belong only to the calling TSF apartment.
        unsafe {
            let mut module = HMODULE::default();
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                PCWSTR(window_proc as *const () as *const u16),
                &mut module,
            )?;
            RegisterClassW(&WNDCLASSW {
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(window_proc),
                hInstance: module.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: w!("EnglishAssist.StatusBar.1"),
                ..Default::default()
            });
            let window = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("EnglishAssist.StatusBar.1"),
                w!("英文助手输入状态"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(module.into()),
                None,
            )?;
            let frame = Box::new(Frame {
                tip: Arc::downgrade(tip),
                editable: HWND::default(),
                snapshot: Snapshot::read(),
                dpi: 96,
                drag: None,
                pressed: None,
                error: false,
                positioned: false,
                saved_position: None,
                label_set: false,
                deferred_until: None,
            });
            SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(frame) as _);
            Ok(Self { window })
        }
    }
    /// Called only after an edit session proved the focused input is not private.
    pub fn show(&self, editable: HWND) {
        // SAFETY: Own live HWND on its apartment; never changes foreground/focus.
        unsafe {
            if !crate::preferences::statusbar_visible() || !focused(editable) {
                self.hide();
                return;
            }
            let pointer = GetWindowLongPtrW(self.window, GWLP_USERDATA) as *mut Frame;
            if pointer.is_null() {
                return;
            }
            (*pointer).editable = editable;
            let dpi = GetDpiForWindow(editable).max(96);
            let geometry_changed = !(*pointer).positioned || (*pointer).dpi != dpi;
            (*pointer).dpi = dpi;
            let size = layout((*pointer).dpi);
            let monitor = MonitorFromWindow(editable, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return;
            }
            let mut current = RECT::default();
            let _ = GetWindowRect(self.window, &mut current);
            let stored = if read_dword(w!("StatusBarHasPosition")).unwrap_or(0) != 0 {
                read_dword(w!("StatusBarX"))
                    .zip(read_dword(w!("StatusBarY")))
                    .map(|(x, y)| (x as i32, y as i32))
            } else {
                None
            };
            let (x, y) = if (*pointer).positioned && stored == (*pointer).saved_position {
                (current.left, current.top)
            } else if let Some(position) = stored {
                position
            } else {
                (
                    info.rcWork.right - size.width - px(20, (*pointer).dpi),
                    info.rcWork.bottom - size.height - px(20, (*pointer).dpi),
                )
            };
            let (x, y) = clamp_position(x, y, size.width, size.height, info.rcWork);
            (*pointer).positioned = true;
            (*pointer).saved_position = stored;
            if current.left != x
                || current.top != y
                || geometry_changed
                || !IsWindowVisible(self.window).as_bool()
            {
                let _ = SetWindowPos(
                    self.window,
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    size.width,
                    size.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            if geometry_changed {
                let region = CreateRoundRectRgn(
                    0,
                    0,
                    size.width + 1,
                    size.height + 1,
                    px(16, (*pointer).dpi),
                    px(16, (*pointer).dpi),
                );
                if SetWindowRgn(self.window, Some(region), false) == 0 {
                    let _ = DeleteObject(region.into());
                }
            }
            SetTimer(Some(self.window), TIMER, POLL_MS, None);
            refresh(self.window);
        }
    }
    pub fn hide(&self) {
        // SAFETY: Hide this apartment's own surface; do not touch host windows.
        unsafe {
            hide_window(self.window);
        }
    }
    pub fn refresh(&self) {
        // SAFETY: Called on this surface's creating apartment.
        unsafe {
            refresh(self.window);
        }
    }
}
impl Drop for StatusBar {
    fn drop(&mut self) {
        // SAFETY: Only this instance owns the HWND; NCDESTROY frees the frame.
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }
}

fn read_dword(name: PCWSTR) -> Option<u32> {
    match crate::user_preferences::read_dword(name) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                hresult = error.code().0,
                "Nonvirtualized status preference read failed"
            );
            None
        }
    }
}
fn write_dword(name: PCWSTR, value: u32) -> Result<()> {
    crate::user_preferences::write_dword(name, value)
}
pub(crate) fn set_visible(visible: bool) -> Result<()> {
    write_dword(w!("StatusBarDisabled"), u32::from(!visible))
}
unsafe fn hide_window(window: HWND) {
    let _ = KillTimer(Some(window), TIMER);
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Frame;
    if !pointer.is_null() {
        (*pointer).drag = None;
        (*pointer).pressed = None;
    }
    if GetCapture() == window {
        let _ = ReleaseCapture();
    }
    let _ = ShowWindow(window, SW_HIDE);
}
unsafe fn focused(editable: HWND) -> bool {
    // SAFETY: All queries are read-only; an invalid/stale HWND simply fails the gate.
    !editable.is_invalid()
        && IsWindow(Some(editable)).as_bool()
        && GetWindowThreadProcessId(editable, None) == GetCurrentThreadId()
        && GetFocus() == editable
        && GetAncestor(editable, GA_ROOT) == GetForegroundWindow()
}
unsafe fn refresh(window: HWND) {
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Frame;
    if pointer.is_null() {
        return;
    }
    if (*pointer)
        .deferred_until
        .is_some_and(|until| Instant::now() >= until)
    {
        (*pointer).deferred_until = None;
        let _ = InvalidateRect(Some(window), None, false);
    }
    let snapshot = Snapshot::read();
    let changed = (*pointer).snapshot != snapshot || !(*pointer).label_set;
    if changed {
        (*pointer).snapshot = snapshot;
        let _ = InvalidateRect(Some(window), None, false);
    }
    if !changed {
        return;
    }
    (*pointer).label_set = true;
    let label = format!(
        "英文助手 · {} · {} · 补词{} · 拼音{} · 连写{} · Shift切换空格，Enter原样",
        snapshot.label(Action::Caps),
        snapshot.label(Action::Space),
        if snapshot.words { "开" } else { "关" },
        if snapshot.pinyin { "开" } else { "关" },
        if snapshot.segmentation { "开" } else { "关" }
    );
    let label: Vec<_> = label.encode_utf16().chain([0]).collect();
    let _ = SetWindowTextW(window, PCWSTR(label.as_ptr()));
}
unsafe fn activate(window: HWND, action: Action) {
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Frame;
    if pointer.is_null() {
        return;
    }
    let Some(tip) = (*pointer).tip.upgrade() else {
        return;
    };
    let editable = (*pointer).editable;
    let current = (*pointer).snapshot;
    if !tip.is_activated() || !focused(editable) {
        return;
    }
    // Copy all targets before calling TSF: a callback may destroy this surface.
    let result = match action {
        Action::Caps => {
            // Never turn a held modifier chord into an injected shortcut.
            if [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN, VK_CAPITAL]
                .into_iter()
                .any(|key| GetAsyncKeyState(i32::from(key.0)) < 0)
            {
                return;
            }
            let inputs = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_CAPITAL,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_CAPITAL,
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
            ];
            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            if sent == inputs.len() as u32 {
                Ok(())
            } else {
                if sent == 1 {
                    let _ = SendInput(&inputs[1..], std::mem::size_of::<INPUT>() as i32);
                }
                Err(E_ACCESSDENIED.into())
            }
        }
        Action::Space => tip.toggle_auto_space(),
        Action::Words => write_dword(w!("EnglishDisabled"), u32::from(current.words)),
        Action::Pinyin => write_dword(w!("EnglishPinyinDisabled"), u32::from(current.pinyin)),
        Action::Segmentation => write_dword(
            w!("EnglishSegmentationDisabled"),
            u32::from(current.segmentation),
        ),
        Action::Settings => crate::preferences::open_settings(),
        Action::Hide => set_visible(false),
    };
    if result.is_ok() {
        tip.sync_scheme();
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Frame;
    if pointer.is_null() {
        return;
    }
    (*pointer).error = result.is_err();
    (*pointer).deferred_until = (result.is_ok()
        && matches!(
            action,
            Action::Words | Action::Pinyin | Action::Segmentation
        ))
    .then(|| Instant::now() + Duration::from_secs(4));
    if action == Action::Hide && result.is_ok() {
        hide_window(window);
    } else {
        refresh(window);
        let _ = InvalidateRect(Some(window), None, false);
    }
}
unsafe fn paint(window: HWND, frame: &Frame) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(window, &mut ps);
    let size = layout(frame.dpi);
    let (background, text, muted, selected, accent) = if frame.snapshot.dark {
        (0x002b2522, 0x00f5f2ef, 0x00b8b1a9, 0x00513d26, 0x00facda0)
    } else {
        (0x00faf8f5, 0x00372b20, 0x00847467, 0x00f5e3cd, 0x00975013)
    };
    let brush = CreateSolidBrush(COLORREF(background));
    let mut whole = RECT {
        left: 0,
        top: 0,
        right: size.width,
        bottom: size.height,
    };
    FillRect(dc, &whole, brush);
    let font = CreateFontW(
        -px(13, frame.dpi),
        0,
        0,
        0,
        500,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        ANTIALIASED_QUALITY,
        0,
        w!("Microsoft YaHei UI"),
    );
    let old = SelectObject(dc, font.into());
    SetBkMode(dc, TRANSPARENT);
    for (i, action) in ACTIONS.into_iter().enumerate() {
        let enabled = frame.snapshot.enabled(action);
        let mut rect = size.cells[i].rect();
        if enabled {
            let cell_brush = CreateSolidBrush(COLORREF(selected));
            let old_brush = SelectObject(dc, cell_brush.into());
            let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
            let _ = RoundRect(
                dc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                px(10, frame.dpi),
                px(10, frame.dpi),
            );
            SelectObject(dc, old_pen);
            SelectObject(dc, old_brush);
            let _ = DeleteObject(cell_brush.into());
        }
        SetTextColor(dc, COLORREF(if enabled { accent } else { text }));
        let mut label: Vec<_> = frame.snapshot.label(action).encode_utf16().collect();
        DrawTextW(
            dc,
            &mut label,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    SetTextColor(dc, COLORREF(muted));
    let mut grip: Vec<_> = "⠿".encode_utf16().collect();
    let mut grip_rect = RECT {
        left: px(4, frame.dpi),
        top: px(8, frame.dpi),
        right: px(20, frame.dpi),
        bottom: px(36, frame.dpi),
    };
    DrawTextW(
        dc,
        &mut grip,
        &mut grip_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    let mut help: Vec<_> = if frame.error {
        "操作失败，请从开始菜单打开设置"
    } else if frame.deferred_until.is_some() {
        "补词/拼音/连写：下次组字生效 · 拖动移动"
    } else {
        "Shift 切换词前空格 · Enter 原样 · 拖动移动"
    }
    .encode_utf16()
    .collect();
    whole = size.help.rect();
    DrawTextW(
        dc,
        &mut help,
        &mut whole,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SelectObject(dc, old);
    let _ = DeleteObject(font.into());
    let _ = DeleteObject(brush.into());
    let _ = EndPaint(window, &ps);
}
unsafe extern "system" fn window_proc(window: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: USERDATA owns one Frame. No borrowed frame crosses reentrant TSF actions.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Frame;
        if msg == WM_MOUSEACTIVATE {
            return LRESULT(MA_NOACTIVATE as isize);
        }
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            let _ = KillTimer(Some(window), TIMER);
            if !pointer.is_null() {
                drop(Box::from_raw(pointer));
            }
        } else if !pointer.is_null() {
            match msg {
                WM_PAINT => {
                    paint(window, &*pointer);
                    return LRESULT(0);
                }
                WM_TIMER => {
                    let tip = (*pointer).tip.upgrade().filter(|tip| tip.is_activated());
                    if tip.is_none()
                        || !focused((*pointer).editable)
                        || !crate::preferences::statusbar_visible()
                    {
                        hide_window(window);
                    } else {
                        refresh(window);
                        // A same-HWND input-scope change still needs a fresh TSF safety gate.
                        // Copy the Arc first: the request may reenter and hide/destroy the bar.
                        if let Some(tip) = tip {
                            tip.refresh_statusbar();
                        }
                    }
                    return LRESULT(0);
                }
                WM_LBUTTONDOWN => {
                    let x = (lp.0 & 0xffff) as i16 as i32;
                    let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                    if !focused((*pointer).editable) {
                        return LRESULT(0);
                    }
                    let pressed = hit_test(&layout((*pointer).dpi), x, y);
                    SetCapture(window);
                    // Capture can synchronously reenter window procedures. Do
                    // not publish a drag until it returns with this frame live.
                    if GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Frame != pointer {
                        return LRESULT(0);
                    }
                    (*pointer).pressed = pressed;
                    (*pointer).drag = None;
                    if pressed.is_none() && GetCapture() == window && focused((*pointer).editable) {
                        let mut cursor = POINT::default();
                        let mut rect = RECT::default();
                        if GetCursorPos(&mut cursor).is_ok()
                            && GetWindowRect(window, &mut rect).is_ok()
                        {
                            (*pointer).drag = Some((
                                cursor,
                                POINT {
                                    x: rect.left,
                                    y: rect.top,
                                },
                            ));
                        }
                    }
                    return LRESULT(0);
                }
                WM_MOUSEMOVE => {
                    if let Some((start, origin)) = (*pointer).drag {
                        let mut cursor = POINT::default();
                        if GetCursorPos(&mut cursor).is_ok() {
                            let size = layout((*pointer).dpi);
                            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
                            let mut info = MONITORINFO {
                                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                                ..Default::default()
                            };
                            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                                let (x, y) = clamp_position(
                                    origin.x + cursor.x - start.x,
                                    origin.y + cursor.y - start.y,
                                    size.width,
                                    size.height,
                                    info.rcWork,
                                );
                                let _ = SetWindowPos(
                                    window,
                                    None,
                                    x,
                                    y,
                                    0,
                                    0,
                                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                                );
                            }
                        }
                    }
                    return LRESULT(0);
                }
                WM_LBUTTONUP => {
                    let pressed = (*pointer).pressed.take();
                    let dragging = (*pointer).drag.take().is_some();
                    if GetCapture() == window {
                        let _ = ReleaseCapture();
                    }
                    if dragging {
                        let mut rect = RECT::default();
                        if GetWindowRect(window, &mut rect).is_ok() {
                            let result = write_dword(w!("StatusBarHasPosition"), 0)
                                .and_then(|_| write_dword(w!("StatusBarX"), rect.left as u32))
                                .and_then(|_| write_dword(w!("StatusBarY"), rect.top as u32))
                                .and_then(|_| write_dword(w!("StatusBarHasPosition"), 1));
                            (*pointer).error = result.is_err();
                            if result.is_ok() {
                                (*pointer).saved_position = Some((rect.left, rect.top));
                            }
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    } else {
                        let x = (lp.0 & 0xffff) as i16 as i32;
                        let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                        if let Some(action) = pressed.filter(|action| {
                            Some(*action) == hit_test(&layout((*pointer).dpi), x, y)
                        }) {
                            activate(window, action);
                        }
                    }
                    return LRESULT(0);
                }
                WM_CAPTURECHANGED => {
                    if HWND(lp.0 as *mut core::ffi::c_void) != window {
                        (*pointer).drag = None;
                        (*pointer).pressed = None;
                    }
                }
                _ => {}
            }
        }
        DefWindowProcW(window, msg, wp, lp)
    }))
    .unwrap_or(LRESULT(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaled_buttons_have_unique_targets_and_help_is_a_drag_surface() {
        for dpi in [96, 120, 144, 192] {
            let size = layout(dpi);
            for (i, cell) in size.cells.iter().enumerate() {
                assert_eq!(
                    hit_test(
                        &size,
                        (cell.left + cell.right) / 2,
                        (cell.top + cell.bottom) / 2
                    ),
                    Some(ACTIONS[i])
                );
                assert!(cell.right <= size.width && cell.bottom <= size.height);
                assert_eq!(hit_test(&size, cell.right, cell.top), None);
            }
            assert_eq!(hit_test(&size, size.help.left, size.help.top), None);
            assert_eq!(hit_test(&size, -1, 10), None);
            assert_eq!(hit_test(&size, size.width, size.height), None);
        }
    }
    #[test]
    fn saved_position_is_clamped_on_negative_or_smaller_monitors() {
        let negative = RECT {
            left: -1920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        assert_eq!(clamp_position(-3000, 1000, 400, 62, negative), (-1920, 818));
        assert_eq!(clamp_position(200, -600, 400, 62, negative), (-400, -200));
        let tiny = RECT {
            left: 10,
            top: 20,
            right: 100,
            bottom: 50,
        };
        assert_eq!(clamp_position(200, -600, 400, 62, tiny), (10, 20));
        // DWORD persistence preserves signed desktop coordinates on roundtrip.
        assert_eq!((-1920_i32 as u32) as i32, -1920);
    }
}
