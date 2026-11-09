use crate::{
    backend::{self, Event, Task},
    instance,
    statistics::{self, Period},
};
use eframe::egui::{self, Color32, FontFamily, RichText, Stroke, Vec2};
use raw_window_handle::HasWindowHandle;
#[cfg(test)]
use retype_types::PinyinScheme;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, Sender},
    Arc,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(crate) const INK: Color32 = Color32::from_rgb(20, 43, 59);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(19, 143, 150);
pub(crate) const ACCENT_SOFT: Color32 = Color32::from_rgb(232, 246, 246);
pub(crate) const ONLINE: Color32 = Color32::from_rgb(31, 168, 102);
pub(crate) const DANGER: Color32 = Color32::from_rgb(209, 61, 61);
pub(crate) const MUTED: Color32 = Color32::from_rgb(101, 119, 129);
const BACKGROUND: Color32 = Color32::from_rgb(246, 249, 250);
pub(crate) const BORDER: Color32 = Color32::from_rgb(227, 233, 236);

type AppResult<T> = backend::Result<T>;

struct Options {
    request_id: u32,
    renderer: eframe::Renderer,
    screenshot: Option<PathBuf>,
    exit_after: Option<Duration>,
    updates: bool,
    page: Page,
    idle_exit: Duration,
    hide_after: Option<Duration>,
}

impl Options {
    fn parse() -> AppResult<Self> {
        let mut options = Self {
            request_id: crate::settings_log::request_id(),
            renderer: eframe::Renderer::Glow,
            screenshot: None,
            exit_after: None,
            updates: false,
            page: Page::General,
            idle_exit: Duration::ZERO,
            hide_after: None,
        };
        for argument in std::env::args().skip(1) {
            if let Some(value) = argument.strip_prefix("--request-id=") {
                options.request_id = value.parse()?;
            } else if let Some(value) = argument.strip_prefix("--renderer=") {
                options.renderer = match value {
                    "glow" => eframe::Renderer::Glow,
                    #[cfg(feature = "wgpu")]
                    "wgpu" => eframe::Renderer::Wgpu,
                    _ => return Err("renderer must be glow or wgpu".into()),
                };
            } else if let Some(value) = argument.strip_prefix("--screenshot=") {
                options.screenshot = Some(value.into());
            } else if let Some(value) = argument.strip_prefix("--exit-after-ms=") {
                options.exit_after = Some(Duration::from_millis(value.parse()?));
            } else if let Some(value) = argument.strip_prefix("--idle-exit-ms=") {
                options.idle_exit = Duration::from_millis(value.parse()?);
            } else if let Some(value) = argument.strip_prefix("--hide-after-ms=") {
                options.hide_after = Some(Duration::from_millis(value.parse()?));
            } else if argument == "--updates" {
                options.updates = true;
                options.page = Page::About;
            } else if let Some(value) = argument.strip_prefix("--page=") {
                options.page = match value {
                    "general" => Page::General,
                    "appearance" => Page::Appearance,
                    "input" => Page::Input,
                    "dictionary" => Page::Dictionary,
                    "statistics" => Page::Statistics,
                    "providers" => Page::Providers,
                    "translation" => Page::Translation,
                    "voice" => Page::Voice,
                    "shortcuts" => Page::Shortcuts,
                    "cloud" => Page::Cloud,
                    "lan" => Page::Lan,
                    "about" => Page::About,
                    _ => return Err("Unknown settings page".into()),
                };
            } else if argument.starts_with("--open-reason=") || argument.starts_with("--opened-at=")
            {
                // Ignore legacy launcher diagnostics from already-loaded older TIPs.
            } else {
                return Err(format!("unknown argument: {argument}").into());
            }
        }
        options.page = english_page(options.page);
        options.updates = false;
        Ok(options)
    }
}

#[cfg(test)]
fn read_scheme_at(key: &str) -> AppResult<PinyinScheme> {
    Ok(if backend::dword_at(key, "PinyinScheme")? == Some(1) {
        PinyinScheme::Flypy
    } else {
        PinyinScheme::Full
    })
}
#[cfg(test)]
fn write_scheme_at(key: &str, scheme: PinyinScheme) -> AppResult<()> {
    backend::set_at(
        key,
        "PinyinScheme",
        u32::from(scheme == PinyinScheme::Flypy),
    )
}
fn logo() -> AppResult<egui::ColorImage> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(include_bytes!(
        "../../../assets/logo.png"
    )))
    .read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("logo must be an 8-bit RGBA PNG".into());
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &pixels[..info.buffer_size()],
    ))
}

pub(crate) fn chinese_fonts(ctx: &egui::Context) -> AppResult<()> {
    let root = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    for name in ["msyh.ttc", "msjh.ttc", "simhei.ttf", "simsun.ttc"] {
        let Ok(bytes) = std::fs::read(root.join("Fonts").join(name)) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "windows-chinese".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "windows-chinese".into());
        }
        ctx.set_fonts(fonts);
        return Ok(());
    }
    Err("No Windows Chinese font is available".into())
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Page {
    General,
    Input,
    Appearance,
    Dictionary,
    Providers,
    Translation,
    Voice,
    Shortcuts,
    Statistics,
    Cloud,
    Lan,
    About,
}

// Unsupported legacy pages cannot start sync, microphone or updater services.
fn english_page(page: Page) -> Page {
    match page {
        Page::Voice | Page::Statistics | Page::Cloud | Page::Lan => Page::General,
        Page::Translation => Page::Providers,
        other => other,
    }
}
struct SettingsApp {
    open_request: u32,
    open_started: Instant,
    frame_logged: bool,
    ai: crate::ai::AiPages,
    cloud: Option<crate::cloud::CloudPage>,
    lan: Option<crate::lan::LanPage>,
    options: Options,
    started: Instant,
    page: Page,
    preferences: backend::Preferences,
    packs: Vec<backend::Pack>,
    installed_packs: Vec<bool>,
    busy_pack: Option<usize>,
    base_status: Option<retype_resources::Status>,
    base_busy: bool,
    base_progress: f32,
    base_message: String,
    pack_progress: f32,
    pack_can_cancel: bool,
    cancel_download: Arc<AtomicBool>,
    statistics: Option<statistics::Snapshot>,
    statistics_version: Option<String>,
    input_selection: [Option<usize>; 2],
    period: Period,
    statistics_pending: bool,
    statistics_refresh: Instant,
    tasks: Sender<Task>,
    events: Receiver<Event>,
    offer: Option<backend::Offer>,
    update_message: String,
    updating: bool,
    installing: bool,
    hidden_since: Option<Instant>,
    instance: instance::Window,
    error: Option<String>,
    logo: egui::TextureHandle,
    screenshot_requested: bool,
    manual_words: Vec<backend::ManualWord>,
    word_input: String,
    word_comment: String,
    word_filter: String,
    editing_word: Option<usize>,
    word_status: String,
    confirm_clear_words: bool,
}

impl SettingsApp {
    fn select_page(&mut self, page: Page) {
        self.ai.cancel_capture();
        let page = english_page(page);
        self.page = page;
        match page {
            Page::Dictionary => {
                match backend::load_manual_words() {
                    Ok(words) => self.manual_words = words,
                    Err(error) => self.manual_words_error(format!("读取个人词库失败：{error}")),
                }
                if !self.base_busy {
                    let _ = self.tasks.send(Task::BaseStatus);
                }
            }
            Page::Cloud => {
                if self.cloud.is_none() {
                    self.cloud = Some(crate::cloud::CloudPage::new());
                }
            }
            Page::Lan => {
                if self.lan.is_none() {
                    self.lan = Some(crate::lan::LanPage::new());
                }
            }
            Page::Statistics if !self.statistics_pending => {
                self.statistics_pending = true;
                let _ = self.tasks.send(Task::Statistics);
            }
            _ => {}
        }
    }

    fn hide(&mut self, ctx: &egui::Context) {
        crate::settings_log::event(
            "app",
            "hide_requested",
            self.open_request,
            "close button or preview timer",
        );
        if let Err(error) = self.ai.flush() {
            crate::settings_log::event(
                "app",
                "hide_failed",
                self.open_request,
                "preferences flush failed; window kept visible",
            );
            self.error = Some(error);
            return;
        }
        if let Some(cloud) = &mut self.cloud {
            if let Err(error) = cloud.flush_for_close() {
                self.error = Some(error);
                return;
            }
        }
        if self.options.idle_exit.is_zero()
            && self.busy_pack.is_none()
            && !self.base_busy
            && !self.updating
            && !self.installing
        {
            crate::settings_log::event(
                "app",
                "close_exit_requested",
                self.open_request,
                "preferences saved; no pending download or update",
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.hidden_since = Some(Instant::now());
        self.instance
            .idle_timer(self.options.idle_exit.max(Duration::from_secs(1)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        crate::settings_log::event(
            "app",
            "hidden",
            self.open_request,
            format!("idle_exit_ms={}", self.options.idle_exit.as_millis()),
        );
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn lifecycle(&mut self, ctx: &egui::Context) {
        self.ai.tick(ctx);
        if let Some(cloud) = &mut self.cloud {
            if cloud.tick(ctx) {
                if let Ok(prefs) = backend::preferences() {
                    self.preferences = prefs;
                }
                self.ai.refresh_from_disk();
                let _ = self.tasks.send(Task::Statistics);
                let _ = self.tasks.send(Task::Packs);
            }
        }
        let requests = std::mem::take(
            &mut *self
                .instance
                .requests
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        for request in requests {
            let updates = request.updates;
            self.open_request = request.id;
            self.open_started = request.received;
            self.frame_logged = false;
            crate::settings_log::event(
                "app",
                "show_dispatch",
                self.open_request,
                format!("updates={updates}"),
            );
            if self.hidden_since.is_some() {
                self.statistics_version = None;
                self.input_selection = [None; 2];
            }
            self.hidden_since = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if let Ok(preferences) = backend::preferences() {
                self.preferences = preferences;
            }
            crate::settings_log::event(
                "app",
                "show_preferences_ready",
                self.open_request,
                "reload complete",
            );
            if updates {
                self.select_page(Page::About);
                self.check_update();
            } else {
                self.select_page(self.page);
            }
        }
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::BaseProgress(progress) => self.base_progress = progress,
                Event::BaseStatus(result) => match result {
                    Ok(state) => self.base_status = Some(state),
                    Err(error) => self.error = Some(error),
                },
                Event::BaseDone(result) => {
                    self.base_busy = false;
                    match result {
                        Ok(state) => {
                            self.base_status = Some(state);
                            self.base_message.clear();
                        }
                        Err(error) => self.base_message = error,
                    }
                }
                Event::Packs(installed) => self.installed_packs = installed,
                Event::PackProgress(progress) => self.pack_progress = progress,
                Event::PackDone(result) => {
                    self.busy_pack = None;
                    if let Err(error) = result {
                        if !self.cancel_download.load(Ordering::Relaxed) {
                            self.error = Some(error);
                        }
                    }
                    if let Ok(prefs) = backend::preferences() {
                        self.preferences = prefs;
                    }
                    let _ = self.tasks.send(Task::Packs);
                }
                Event::Statistics(snapshot) => {
                    self.statistics = Some(*snapshot);
                    self.statistics_pending = false;
                    self.statistics_refresh = Instant::now();
                }
                Event::Checked(result) => {
                    self.updating = false;
                    match result {
                        Ok(offer) => {
                            self.update_message = if offer.available {
                                if offer.installable {
                                    format!(
                                        "发现新版本 {}。",
                                        offer.version.as_deref().unwrap_or("")
                                    )
                                } else {
                                    "发现新版本，但缺少安装包或校验文件。".into()
                                }
                            } else {
                                "已是最新版本。".into()
                            };
                            self.offer = Some(offer);
                        }
                        Err(error) => self.update_message = format!("检查失败：{error}"),
                    }
                }
                Event::UpdateStage(message) => self.update_message = message,
                Event::Installed(result) => {
                    self.installing = false;
                    self.update_message = match result {
                        Ok(message) => {
                            self.offer = None;
                            message
                        }
                        Err(error) => format!("更新未完成：{error}"),
                    };
                    if let Ok(prefs) = backend::preferences() {
                        self.preferences = prefs;
                    }
                }
                Event::Skipped(result) => {
                    self.update_message = match result {
                        Ok(()) => "已跳过此版本的自动提醒。".into(),
                        Err(error) => format!("保存失败：{error}"),
                    }
                }
                Event::Error(error) => {
                    self.error = Some(error);
                    self.statistics_pending = false;
                    self.statistics_refresh = Instant::now();
                }
            }
        }
        if let Some(hidden) = self.hidden_since {
            if hidden.elapsed() >= self.options.idle_exit
                && self.busy_pack.is_none()
                && !self.base_busy
                && !self.updating
                && !self.installing
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.page == Page::Statistics {
            if !self.statistics_pending
                && self.statistics_refresh.elapsed() >= Duration::from_secs(5)
            {
                self.statistics_pending = true;
                let _ = self.tasks.send(Task::Statistics);
            }
            ctx.request_repaint_after(Duration::from_secs(5));
        }
    }

    fn check_update(&mut self) {
        self.update_message = "此开发版请使用 English Assist 安装包更新。".into();
    }

    fn open(&mut self, destination: &str) {
        if let Err(error) = backend::open(destination) {
            self.error = Some(error.to_string());
        }
    }

    fn base_resources_ui(&mut self, ui: &mut egui::Ui) {
        let Some(status) = &self.base_status else {
            return;
        };
        if status.ready {
            return;
        }
        let size = status.download_bytes as f64 / (1024.0 * 1024.0);
        let title = if status.base.is_some() {
            "补全基础词库"
        } else {
            "基础词库"
        };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.base_busy {
                        if icon_button(ui, Icon::Close, "取消下载", false, false, true) {
                            self.cancel_download.store(true, Ordering::Relaxed);
                        }
                    } else {
                        if ui
                            .button("导入")
                            .on_hover_text("从本地文件导入词库包")
                            .clicked()
                        {
                            match backend::choose_resource_bundle() {
                                Ok(Some(path)) => self.install_base(Some(path)),
                                Ok(None) => {}
                                Err(e) => self.base_message = e.to_string(),
                            }
                        }
                        if primary_button(ui, &format!("下载 · {size:.1} MB"), true) {
                            self.install_base(None);
                        }
                    }
                });
            });
            if self.base_busy {
                ui.add_space(6.0);
                ui.add(egui::ProgressBar::new(self.base_progress).show_percentage());
            }
            if !self.base_message.is_empty() {
                ui.label(RichText::new(&self.base_message).size(13.0).color(MUTED));
            }
        });
        ui.add_space(14.0);
    }
    fn install_base(&mut self, source: Option<PathBuf>) {
        if self.busy_pack.is_some() {
            return;
        }
        self.base_busy = true;
        self.base_progress = 0.0;
        self.base_message.clear();
        self.cancel_download.store(false, Ordering::Relaxed);
        let _ = self
            .tasks
            .send(Task::BaseInstall(source, Arc::clone(&self.cancel_download)));
    }

    #[allow(dead_code)] // Retained upstream page, not reachable in the English-only product.
    fn dictionary_page(&mut self, ui: &mut egui::Ui) {
        self.base_resources_ui(ui);
        if self.installed_packs.is_empty() {
            ui.spinner();
            ui.label("正在读取词库…");
        }
        for index in 0..self.packs.len() {
            let pack = self.packs[index].clone();
            let installed = self.installed_packs.get(index).copied().unwrap_or(false);
            let enabled = installed && self.preferences.pack_mask & (1 << index) != 0;
            let busy = self.busy_pack == Some(index);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    let info_width = (ui.available_width() - 140.0).max(120.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(info_width, 54.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_width(info_width);
                            ui.label(RichText::new(&pack.title).strong());
                            ui.label(
                                RichText::new(format!(
                                    "{} · {:.1} MB",
                                    pack.description,
                                    pack.bytes as f32 / 1048576.0
                                ))
                                .size(13.0)
                                .color(MUTED),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if busy && self.pack_can_cancel {
                            if !self.cancel_download.load(Ordering::Relaxed)
                                && ui.button("取消").clicked()
                            {
                                self.cancel_download.store(true, Ordering::Relaxed);
                            }
                        } else if !busy {
                            let mut next_enabled = enabled;
                            let changed = ui
                                .add_enabled_ui(
                                    self.busy_pack.is_none()
                                        && !self.base_busy
                                        && !self.installed_packs.is_empty(),
                                    |ui| {
                                        toggle(
                                            ui,
                                            &mut next_enabled,
                                            &format!("启用{}", pack.title),
                                        )
                                    },
                                )
                                .inner;
                            if changed {
                                self.busy_pack = Some(index);
                                self.pack_progress = 0.0;
                                self.pack_can_cancel = !enabled && !installed;
                                self.cancel_download = Arc::new(AtomicBool::new(false));
                                self.error = None;
                                let _ = self.tasks.send(Task::SetPack(
                                    index,
                                    next_enabled,
                                    Arc::clone(&self.cancel_download),
                                ));
                            }
                        }
                    });
                });
                if busy {
                    ui.add(egui::ProgressBar::new(self.pack_progress).show_percentage());
                    ui.label(
                        RichText::new(if self.cancel_download.load(Ordering::Relaxed) {
                            "正在取消…"
                        } else if self.pack_progress >= 1.0 {
                            "正在校验和转换词库…"
                        } else {
                            "正在下载…"
                        })
                        .size(12.0)
                        .color(MUTED),
                    );
                }
            });
            ui.add_space(8.0);
        }
        ui.hyperlink_to("词库来源与许可", "https://github.com/amzxyz/rime-wanxiang");
    }

    fn statistics_page(&mut self, ui: &mut egui::Ui) {
        use chrono::Datelike;
        let filter_before = (self.period, self.statistics_version.clone());
        let Some(snapshot) = self.statistics.clone() else {
            ui.spinner();
            ui.label("正在读取统计…");
            return;
        };
        ui.horizontal(|ui| {
            ui.label("版本");
            egui::ComboBox::from_id_salt("statistics-version")
                .selected_text(
                    self.statistics_version
                        .as_deref()
                        .map(version_label)
                        .unwrap_or("全部版本"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.statistics_version, None, "全部版本");
                    for version in snapshot.by_version.keys().rev() {
                        ui.selectable_value(
                            &mut self.statistics_version,
                            Some(version.clone()),
                            version_label(version),
                        );
                    }
                });
        });
        let empty = statistics::Snapshot::default();
        let snapshot = match &self.statistics_version {
            Some(version) => snapshot.by_version.get(version).unwrap_or(&empty),
            None => &snapshot,
        };
        ui.label(RichText::new("桌面端统计").color(MUTED));
        if snapshot.voice.chinese > 0 || snapshot.voice.english_words > 0 {
            card(ui, |ui| {
                ui.label("累计语音输入");
                ui.label(format!(
                    "中文 {} 字 · 英文 {} 词",
                    snapshot.voice.chinese, snapshot.voice.english_words
                ));
            });
        }
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            for (column, title, counts) in [
                (0, "今日输入", snapshot.today),
                (1, "累计输入", snapshot.total),
            ] {
                card(&mut columns[column], |ui| {
                    ui.label(title);
                    ui.label(
                        RichText::new(format!(
                            "中文 {} 字 · 英文 {} 词",
                            counts.chinese, counts.english_words
                        ))
                        .size(18.0)
                        .color(MUTED),
                    );
                });
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("近期速度").strong());
            ui.label(
                RichText::new("最近 5 分钟；停止输入 30 秒后不显示速度。")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.columns(2, |columns| {
                speed_value(&mut columns[0], "中文", "字/分钟", snapshot.chinese_speed);
                speed_value(&mut columns[1], "英文", "词/分钟", snapshot.english_speed);
            });
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("平均速度").strong());
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                ui.painter()
                    .circle_stroke(rect.center(), 7.0, Stroke::new(1.0_f32, MUTED));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "?",
                    egui::FontId::proportional(12.0),
                    MUTED,
                );
                response
                    .on_hover_text("中文按字数、英文按实际单词数计算均速；连续输入间隔超过 15 秒不计入时间，样本不足时不显示速度；英文单词统计从升级后开始，旧字符记录保留。");
            });
            ui.horizontal(|ui| {
                for period in [Period::Day, Period::Week, Period::Month, Period::Year] {
                    ui.selectable_value(&mut self.period, period, period.label());
                }
            });
            let history = snapshot.history(chrono::Local::now().date_naive(), self.period);
            if history.len() >= 2 {
                let current = history[history.len() - 1].1;
                let previous = history[history.len() - 2].1;
                ui.columns(2, |columns| {
                    for (index, language, unit, speed, prior, count, active) in [
                        (
                            0,
                            "中文",
                            "字/分钟",
                            current.chinese_speed(),
                            previous.chinese_speed(),
                            current.chinese,
                            current.chinese_ms,
                        ),
                        (
                            1,
                            "英文",
                            "词/分钟",
                            current.english_speed(),
                            previous.english_speed(),
                            current.english_words,
                            current.english_word_ms,
                        ),
                    ] {
                        speed_value(&mut columns[index], language, unit, speed);
                        let change = match (speed, prior) {
                            (Some(now), Some(before)) if before > 0 => format!(
                                "较上期 {:+.0}%",
                                (now as f64 - before as f64) * 100.0 / before as f64
                            ),
                            (Some(_), _) => "上期无可比数据".into(),
                            _ => "样本不足".into(),
                        };
                        columns[index].label(RichText::new(change).size(12.0).color(MUTED));
                        columns[index].label(
                            RichText::new(format!(
                                "{count} {} · 有效 {}",
                                if index == 0 { "字" } else { "词" },
                                active_duration(active)
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                    }
                });
            }
            let data: Vec<_> = history
                .iter()
                .map(|(date, counts)| {
                    (
                        match self.period {
                            Period::Day | Period::Week => {
                                format!("{}/{}", date.month(), date.day())
                            }
                            Period::Month => format!("{}月", date.month()),
                            Period::Year => date.year().to_string(),
                        },
                        counts.chinese_speed(),
                        counts.english_speed(),
                    )
                })
                .collect();
            chart(ui, &data, false);
        });
        ui.add_space(14.0);
        if filter_before != (self.period, self.statistics_version.clone()) {
            self.input_selection = [None; 2];
        }
        let history = snapshot.history(chrono::Local::now().date_naive(), self.period);
        let input = history.last().map(|(_, c)| c.input).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new("回改与上屏长度").strong());
            crate::input_chart::info(ui, format!("当前时间与版本范围内取消输入 {} 次。取消没有最终长度，不分配到图中。\n仅包含新口径启用后采集的样本。",input.cancelled));
        });
        for (i, chinese, title) in [
            (0, true, "中文输入 · 字/次"),
            (1, false, "英文输入 · 词/次"),
        ] {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).strong());
                    crate::input_chart::info(
                        ui,
                        crate::input_chart::summary(
                            &input,
                            chinese,
                            &format!(
                                "本{} · {}",
                                self.period.label(),
                                self.statistics_version
                                    .as_deref()
                                    .map(version_label)
                                    .unwrap_or("全部版本")
                            ),
                        ),
                    );
                });
                crate::input_chart::draw(ui, &input, chinese, &mut self.input_selection[i]);
            });
            ui.add_space(10.0);
        }
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("最近 7 天输入量").strong());
            let today = chrono::Local::now().date_naive();
            let data: Vec<_> = (0..7)
                .map(|i| {
                    let date = today - chrono::Duration::days(6 - i);
                    let counts = snapshot.days.get(&date).copied().unwrap_or_default();
                    (
                        format!("{}/{}", date.month(), date.day()),
                        Some(counts.chinese),
                        Some(counts.english_words),
                    )
                })
                .collect();
            chart(ui, &data, true);
        });
    }

    fn about_page(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(
                RichText::new("English Assist · 英文助手")
                    .size(18.0)
                    .strong(),
            );
            ui.label(format!("开发验证版 · {}", self.preferences.version));
            ui.label("Windows 原生英文输入法：本地补词、拼写建议与主动 AI 编辑。");
            ui.label(
                RichText::new("此分支基于 Retype 修改；输入兼容性以随包验证记录为准。")
                    .color(MUTED),
            );
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(RichText::new("来源与许可").strong());
            ui.label("Retype 源码使用 MIT；词库数据保留各自许可与署名。");
            ui.horizontal(|ui| {
                for (title, file) in [("查看署名", "NOTICE.txt"), ("查看许可", "LICENSE")] {
                    if ui.button(title).clicked() {
                        self.open(&self.preferences.directory.join(file).to_string_lossy());
                    }
                }
                ui.hyperlink_to("上游源码", "https://github.com/L-Chris/retype");
            });
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(RichText::new("本地数据与诊断").strong());
            ui.label("诊断日志保存在本机 EnglishAssist/logs 目录，不记录候选或输入正文。");
            if ui.button("打开本地数据目录").clicked() {
                match backend::local_root() {
                    Ok(path) => self.open(&path.to_string_lossy()),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(RichText::new("卸载").strong());
            ui.label("请在 Windows 设置 → 应用 → 已安装的应用中卸载 English Assist。");
            ui.label("卸载会移除本项目英语入口和密钥；使用中的组件可能需重启后清理。");
        });
    }
    fn english_dictionary_page(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(RichText::new("离线英文词库").strong());
            match &self.base_status {
                Some(status) if status.english.is_some() => {
                    ui.colored_label(ONLINE, "英文词库已就绪");
                    ui.label("单词补全与拼写建议无需联网。");
                }
                Some(_) => {
                    ui.colored_label(DANGER, "未找到英文词库，请修复或重新安装 English Assist。");
                }
                None => {
                    ui.spinner();
                    ui.label("正在检查词库…");
                }
            }
            if ui.button("刷新状态").clicked() {
                let _ = self.tasks.send(Task::BaseStatus);
            }
            ui.label(
                RichText::new("英文词频来源：SymSpell / SCOWL / Google Ngram，详见随包 NOTICE。")
                    .color(MUTED),
            );
        });
        ui.add_space(12.0);
        self.manual_words_ui(ui);
    }

    fn general_page(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(
                RichText::new("English Assist · 让英文输入更顺手")
                    .size(19.0)
                    .strong(),
            );
            ui.label("离线补词、中文释义、拼音查英文；AI 编辑由你主动触发。");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Windows 语言与键盘设置").clicked() {
                    self.open("ms-settings:regionlanguage");
                }
                if ui.button("查看离线词库").clicked() {
                    self.select_page(Page::Dictionary);
                }
            });
            ui.label(
                RichText::new(
                    "用 Win+Space 选择 English Assist 英文助手。设置自动保存，回到输入框后生效。",
                )
                .color(MUTED),
            );
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("当前状态").strong());
            ui.label(if self.preferences.english.0 {
                "单词补全：开启"
            } else {
                "单词补全：关闭"
            });
            ui.label(if self.preferences.english_pinyin {
                "拼音查英文：开启"
            } else {
                "拼音查英文：关闭"
            });
            ui.label(format!("手动词库：{} 个词条", self.manual_words.len()));
            ui.label("自动学习：关闭。只有你手动添加或导入的词会进入个人词库。");
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("本地数据").strong());
            ui.label("个人词库、界面偏好和连接配置保存在本机；API Key 存在 Windows 凭据管理器中。");
            if ui.button("打开用户数据文件夹").clicked() {
                match backend::local_root() {
                    Ok(path) => self.open(&path.to_string_lossy()),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        });
    }

    fn appearance_page(&mut self, ui: &mut egui::Ui) {
        use retype_types::CandidateTheme;
        let previous = self.preferences.appearance;
        card(ui, |ui| {
            ui.label(RichText::new("候选窗口").strong());
            ui.horizontal(|ui| {
                ui.label("主题");
                egui::ComboBox::from_id_salt("candidate-theme")
                    .selected_text(match self.preferences.appearance.theme {
                        CandidateTheme::System => "跟随系统",
                        CandidateTheme::Light => "浅色",
                        CandidateTheme::Dark => "深色",
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (CandidateTheme::System, "跟随系统"),
                            (CandidateTheme::Light, "浅色"),
                            (CandidateTheme::Dark, "深色"),
                        ] {
                            ui.selectable_value(
                                &mut self.preferences.appearance.theme,
                                value,
                                label,
                            );
                        }
                    });
            });
            ui.add(
                egui::Slider::new(&mut self.preferences.appearance.font_size, 14..=22).text("字号"),
            );
            ui.add(
                egui::Slider::new(&mut self.preferences.appearance.width, 320..=460)
                    .text("宽度")
                    .suffix(" px"),
            );
            switch_row(ui, &mut self.preferences.appearance.compact, "紧凑行距");
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("候选内容").strong());
            switch_row(
                ui,
                &mut self.preferences.appearance.show_meanings,
                "显示中文释义",
            );
            switch_row(
                ui,
                &mut self.preferences.appearance.show_part_of_speech,
                "显示词性",
            );
            switch_row(
                ui,
                &mut self.preferences.appearance.show_phonetics,
                "显示音标",
            );
            switch_row(
                ui,
                &mut self.preferences.appearance.show_key_hints,
                "显示选词与翻页提示",
            );
            ui.label(
                RichText::new("每页 5 个候选，支持 1–5 选词和 − / + 翻页。")
                    .size(12.0)
                    .color(MUTED),
            );
        });
        ui.add_space(14.0);
        let view = self.preferences.appearance;
        let dark = view.theme == CandidateTheme::Dark
            || view.theme == CandidateTheme::System && backend::system_dark();
        let ink = if dark {
            Color32::from_rgb(234, 239, 245)
        } else {
            INK
        };
        let bg = if dark {
            Color32::from_rgb(30, 38, 49)
        } else {
            Color32::WHITE
        };
        ui.label(RichText::new("即时预览").strong());
        egui::Frame::new()
            .fill(bg)
            .corner_radius(12)
            .inner_margin(if view.compact { 10 } else { 16 })
            .show(ui, |ui| {
                ui.set_max_width((view.width as f32 - 32.0).min(ui.available_width()));
                ui.label(
                    RichText::new("1   environment")
                        .size(view.font_size as f32)
                        .color(ink)
                        .strong(),
                );
                let mut description = String::new();
                if view.show_part_of_speech {
                    description.push_str("n.  ");
                }
                if view.show_meanings {
                    description.push_str("环境；周围状况");
                }
                if !description.is_empty() {
                    ui.label(RichText::new(description).color(ink));
                }
                if view.show_phonetics {
                    ui.label(RichText::new("/ɪnˈvaɪrənmənt/").color(ink));
                }
                if view.show_key_hints {
                    ui.label(
                        RichText::new("1–5 选词   − / + 翻页   Esc 保留原词")
                            .size(12.0)
                            .color(ink),
                    );
                }
            });
        if ui.button("恢复外观默认值").clicked() {
            self.preferences.appearance = Default::default();
        }
        if self.preferences.appearance != previous {
            if let Err(error) = backend::save_appearance(self.preferences.appearance) {
                self.preferences.appearance = previous;
                self.error = Some(format!("保存外观失败：{error}"));
            }
        }
    }

    fn persist_manual_words(&mut self, mut words: Vec<backend::ManualWord>) -> bool {
        words.sort_by_key(|entry| entry.word.to_ascii_lowercase());
        match backend::save_manual_words(&words) {
            Ok(()) => {
                self.manual_words = words;
                self.word_input.clear();
                self.word_comment.clear();
                self.editing_word = None;
                self.confirm_clear_words = false;
                self.word_status = "已保存，返回输入框后使用新词库。".into();
                self.error = None;
                true
            }
            Err(error) => {
                self.manual_words_error(format!("保存个人词库失败：{error}"));
                false
            }
        }
    }

    fn manual_words_error(&mut self, error: String) {
        self.word_status.clear();
        self.error = Some(error);
    }

    fn manual_words_ui(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(RichText::new("个人词库").strong());
            ui.label("手动保存专业术语、姓名或常用词。这里的内容只在本机补全；不会自动学习你键入的文字。");
            ui.add(
                egui::TextEdit::singleline(&mut self.word_input)
                    .hint_text("英文单词，例如 Codex")
                    .desired_width(f32::INFINITY),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.word_comment)
                    .hint_text("词性和中文释义 / 备注（可选）")
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui
                    .button(if self.editing_word.is_some() {
                        "保存修改"
                    } else {
                        "添加词条"
                    })
                    .clicked()
                {
                    let entry = backend::ManualWord {
                        word: self.word_input.trim().into(),
                        comment: self.word_comment.trim().into(),
                    };
                    let mut words = self.manual_words.clone();
                    if let Some(index) = self.editing_word.filter(|&i| i < words.len()) {
                        words[index] = entry;
                    } else {
                        words.push(entry);
                    }
                    self.persist_manual_words(words);
                }
                if self.editing_word.is_some() && ui.button("取消编辑").clicked() {
                    self.editing_word = None;
                    self.word_input.clear();
                    self.word_comment.clear();
                }
            });
            ui.label(
                RichText::new("单词最多 64 个英文字母，可包含内部撇号；释义最多 256 个字符。")
                    .size(12.0)
                    .color(MUTED),
            );
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("导入词库").clicked() {
                    match backend::choose_manual_words_file(false) {
                        Ok(Some(path)) => match backend::import_manual_words(&path) {
                            Ok(imported) => {
                                let count = imported.len();
                                let mut words = self.manual_words.clone();
                                for entry in imported {
                                    if let Some(old) = words
                                        .iter_mut()
                                        .find(|old| old.word.eq_ignore_ascii_case(&entry.word))
                                    {
                                        *old = entry;
                                    } else {
                                        words.push(entry);
                                    }
                                }
                                if self.persist_manual_words(words) {
                                    self.word_status = format!("已导入并合并 {count} 个词条。");
                                }
                            }
                            Err(error) => self.manual_words_error(format!("导入失败：{error}")),
                        },
                        Ok(None) => {}
                        Err(error) => self.manual_words_error(error.to_string()),
                    }
                }
                if ui.button("导出词库").clicked() {
                    match backend::choose_manual_words_file(true) {
                        Ok(Some(path)) => {
                            match backend::export_manual_words(&path, &self.manual_words) {
                                Ok(()) => self.word_status = "词库已导出为 JSON 文件。".into(),
                                Err(error) => self.manual_words_error(format!("导出失败：{error}")),
                            }
                        }
                        Ok(None) => {}
                        Err(error) => self.manual_words_error(error.to_string()),
                    }
                }
                if ui.button("清空词库").clicked() {
                    self.confirm_clear_words = true;
                }
            });
            ui.label(
                RichText::new(
                    "支持 JSON，或 UTF-8 TXT/TSV：每行“单词<Tab>释义”。同名单词以导入内容更新。",
                )
                .size(12.0)
                .color(MUTED),
            );
            if self.confirm_clear_words {
                ui.colored_label(DANGER, "清空会移除全部手动词条，原文件会保留为备份。");
                ui.horizontal(|ui| {
                    if ui.button("确认清空").clicked() {
                        self.persist_manual_words(Vec::new());
                    }
                    if ui.button("取消").clicked() {
                        self.confirm_clear_words = false;
                    }
                });
            }
        });
        ui.add_space(14.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.word_filter)
                .hint_text("搜索单词或释义")
                .desired_width(f32::INFINITY),
        );
        let query = self.word_filter.to_lowercase();
        let rows: Vec<_> = self
            .manual_words
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.word.to_lowercase().contains(&query)
                    || entry.comment.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        ui.label(format!("{} 个词条", rows.len()));
        let mut edit = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("manual-words-list")
            .max_height(260.0)
            .show_rows(ui, 38.0, rows.len(), |ui, range| {
                for row in range {
                    let index = rows[row];
                    let entry = &self.manual_words[index];
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [120.0, 30.0],
                            egui::Label::new(RichText::new(&entry.word).strong()).truncate(),
                        );
                        let shown: String = entry.comment.chars().take(28).collect();
                        ui.add_sized(
                            [(ui.available_width() - 110.0).max(60.0), 30.0],
                            egui::Label::new(shown).truncate(),
                        )
                        .on_hover_text(&entry.comment);
                        if ui.button("编辑").clicked() {
                            edit = Some(index);
                        }
                        if ui.button("删除").clicked() {
                            remove = Some(index);
                        }
                    });
                }
            });
        if let Some(index) = edit {
            self.word_input = self.manual_words[index].word.clone();
            self.word_comment = self.manual_words[index].comment.clone();
            self.editing_word = Some(index);
        }
        if let Some(index) = remove {
            let mut words = self.manual_words.clone();
            words.remove(index);
            self.persist_manual_words(words);
        }
        if !self.word_status.is_empty() {
            ui.colored_label(ONLINE, &self.word_status);
        }
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(RichText::new("本地文件与备份").strong());
            ui.label("词库保存在 EnglishAssist/words/manual.json；每次保存前保留上一个版本 manual.json.bak。导出可保存独立备份。");
            ui.label("个人词条保存在本机，供各应用中的输入法读取；保存前保留上一个版本备份。");
            if ui.button("打开本地数据目录").clicked() {
                match backend::local_root() {
                    Ok(path) => self.open(&path.to_string_lossy()),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        });
    }
    fn new(
        cc: &eframe::CreationContext<'_>,
        options: Options,
        started: Instant,
    ) -> AppResult<Self> {
        crate::settings_log::event(
            "app",
            "fonts_begin",
            options.request_id,
            format!("elapsed_ms={}", started.elapsed().as_millis()),
        );
        chinese_fonts(&cc.egui_ctx)?;
        crate::settings_log::event(
            "app",
            "fonts_ready",
            options.request_id,
            format!("elapsed_ms={}", started.elapsed().as_millis()),
        );
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.override_text_color = Some(INK);
        style.interaction.selectable_labels = false;
        style.interaction.multi_widget_text_select = false;
        style.visuals.panel_fill = BACKGROUND;
        style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(229, 247, 247);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        style.spacing.button_padding = Vec2::new(10.0, 7.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(216, 240, 242);
        style.visuals.selection.stroke = Stroke::new(1.0, INK);
        // Windows defaults to painting preedit as a selection. Keep actual
        // selections visible, and show composition with unobtrusive underlines.
        style.visuals.ime_composition.legacy_visuals = false;
        style.visuals.ime_composition.active_underline_stroke = Stroke::new(1.0, ACCENT);
        style.visuals.ime_composition.inactive_underline_stroke = Stroke::new(1.0, MUTED);
        style.spacing.item_spacing = Vec2::new(10.0, 12.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
        cc.egui_ctx.set_global_style(style);
        let image = logo()?;
        crate::settings_log::event("app", "logo_ready", options.request_id, "decoded");
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
                egui::IconData {
                    rgba: image.pixels.iter().flat_map(|c| c.to_array()).collect(),
                    width: image.size[0] as u32,
                    height: image.size[1] as u32,
                },
            ))));
        crate::settings_log::event(
            "app",
            "preferences_begin",
            options.request_id,
            "registry and update state",
        );
        let preferences = backend::preferences()?;
        crate::settings_log::event("app", "preferences_ready", options.request_id, "loaded");
        let (manual_words, error) = match backend::load_manual_words() {
            Ok(words) => (words, None),
            Err(error) => (Vec::new(), Some(format!("读取个人词库失败：{error}"))),
        };
        let page = options.page;
        let (tasks, events, busy) = backend::worker(cc.egui_ctx.clone());
        let _ = tasks.send(Task::BaseStatus);
        let handle = cc.window_handle()?.as_raw();
        let raw_window_handle::RawWindowHandle::Win32(handle) = handle else {
            return Err("Unsupported window handle".into());
        };
        let instance = instance::Window::create(
            windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut _),
            cc.egui_ctx.clone(),
            busy,
        )?;
        crate::settings_log::event(
            "app",
            "ai_preferences_begin",
            options.request_id,
            "initializing",
        );
        let ai = crate::ai::AiPages::new();
        crate::settings_log::event(
            "app",
            "ai_preferences_ready",
            options.request_id,
            "initialized; no values recorded",
        );
        if page == Page::Statistics {
            let _ = tasks.send(Task::Statistics);
        }
        if options.updates {
            let _ = tasks.send(Task::Check);
        }
        Ok(Self {
            open_request: options.request_id,
            open_started: started,
            frame_logged: false,
            ai,
            lan: None,
            cloud: None,
            started,
            page,
            preferences,
            packs: backend::packs()?,
            installed_packs: Vec::new(),
            busy_pack: None,
            base_status: None,
            base_busy: false,
            base_progress: 0.0,
            base_message: String::new(),
            pack_progress: 0.0,
            pack_can_cancel: false,
            cancel_download: Arc::new(AtomicBool::new(false)),
            statistics: None,
            statistics_version: None,
            input_selection: [None; 2],
            period: Period::Day,
            statistics_pending: page == Page::Statistics,
            statistics_refresh: Instant::now(),
            tasks,
            events,
            offer: None,
            update_message: if options.updates {
                "正在检查更新…".into()
            } else {
                "检查是否有新版本。".into()
            },
            updating: options.updates,
            installing: false,
            hidden_since: None,
            instance,
            error,
            logo: cc
                .egui_ctx
                .load_texture("retype-logo", image, egui::TextureOptions::LINEAR),
            screenshot_requested: false,
            manual_words,
            word_input: String::new(),
            word_comment: String::new(),
            word_filter: String::new(),
            editing_word: None,
            word_status: String::new(),
            confirm_clear_words: false,
            options,
        })
    }

    fn input_page(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(
                RichText::new("English Assist · 英文助手")
                    .size(17.0)
                    .strong(),
            );
            ui.label("在 Windows 的英语（美国）键盘列表中选择 English Assist。");
            ui.label(
                RichText::new("本地补词与拼写建议离线工作；AI 只在你主动触发时处理选中的文本。")
                    .color(MUTED),
            );
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("英文输入法").size(15.0).strong());
            ui.add_space(8.0);
            if switch_row(ui, &mut self.preferences.english.0, "英文单词补全") {
                if let Err(error) =
                    backend::set("EnglishDisabled", u32::from(!self.preferences.english.0))
                {
                    self.preferences.english.0 = !self.preferences.english.0;
                    self.error = Some(format!("保存设置失败：{error}"));
                }
            }
            ui.separator();
            if switch_row(ui, &mut self.preferences.english_pinyin, "拼音查英文") {
                if let Err(error) = backend::set(
                    "EnglishPinyinDisabled",
                    u32::from(!self.preferences.english_pinyin),
                ) {
                    self.preferences.english_pinyin = !self.preferences.english_pinyin;
                    self.error = Some(format!("保存设置失败：{error}"));
                }
            }
            ui.label(
                RichText::new("例如输入 nihao、pingguo 查找对应英文；不影响已知英文词的正常补全。")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.separator();
            ui.add_enabled_ui(self.preferences.english.0, |ui| {
                if switch_row(
                    ui,
                    &mut self.preferences.english_segmentation,
                    "连写智能分词",
                ) {
                    if let Err(error) = backend::set(
                        "EnglishSegmentationDisabled",
                        u32::from(!self.preferences.english_segmentation),
                    ) {
                        self.preferences.english_segmentation =
                            !self.preferences.english_segmentation;
                        self.error = Some(format!("保存设置失败：{error}"));
                    }
                }
                ui.label(
                    RichText::new("识别连写候选；开启自动空格后，Space 可补齐已输入词的内部空格。")
                        .size(12.0)
                        .color(MUTED),
                );
                ui.separator();
                if switch_row(ui, &mut self.preferences.english_phrases, "短语候选") {
                    if let Err(error) = backend::set(
                        "EnglishPhrasesDisabled",
                        u32::from(!self.preferences.english_phrases),
                    ) {
                        self.preferences.english_phrases = !self.preferences.english_phrases;
                        self.error = Some(format!("保存设置失败：{error}"));
                    }
                }
                ui.label(
                    RichText::new("在本地单词候选中提供常用短语，确认后提交完整组合。")
                        .size(12.0)
                        .color(MUTED),
                );
                ui.separator();
                if switch_row(ui, &mut self.preferences.english.1, "英文拼写建议") {
                    if let Err(error) = backend::set(
                        "EnglishSpellingDisabled",
                        u32::from(!self.preferences.english.1),
                    ) {
                        self.preferences.english.1 = !self.preferences.english.1;
                        self.error = Some(format!("保存设置失败：{error}"));
                    }
                }
            });
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("输入状态与空格").size(15.0).strong());
            ui.add_space(8.0);
            if switch_row(ui, &mut self.preferences.english_auto_space, "自动词前空格") {
                if let Err(error) = backend::set(
                    "EnglishAutoSpaceDisabled",
                    u32::from(!self.preferences.english_auto_space),
                ) {
                    self.preferences.english_auto_space = !self.preferences.english_auto_space;
                    self.error = Some(format!("保存设置失败：{error}"));
                }
            }
            ui.label(RichText::new("确认新词时按需补前面的空格，不添加尾空格；单按 Shift 切换。关闭后手动输入空格。").size(12.0).color(MUTED));
            ui.separator();
            if switch_row(ui, &mut self.preferences.statusbar, "显示悬浮输入状态条") {
                if let Err(error) =
                    backend::set("StatusBarDisabled", u32::from(!self.preferences.statusbar))
                {
                    self.preferences.statusbar = !self.preferences.statusbar;
                    self.error = Some(format!("保存设置失败：{error}"));
                }
            }
            ui.label(RichText::new("在英文输入框中显示大小写、空格和输入功能开关；点击不抢输入焦点，拖动底部可移动。").size(12.0).color(MUTED));
            ui.label(
                RichText::new("补词、拼音和连写开关从下次组字生效，保留当前输入。")
                    .size(12.0)
                    .color(MUTED),
            );
            if ui.button("恢复状态条默认位置").clicked() {
                if let Err(error) = backend::set("StatusBarHasPosition", 0) {
                    self.error = Some(format!("恢复位置失败：{error}"));
                }
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            if switch_row(ui, &mut self.preferences.symbol_completion, "符号自动补全") {
                if let Err(error) = backend::set(
                    "SymbolCompletionEnabled",
                    u32::from(self.preferences.symbol_completion),
                ) {
                    self.preferences.symbol_completion = !self.preferences.symbol_completion;
                    self.error = Some(format!("保存设置失败：{error}"));
                }
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("键盘操作").strong());
            ui.label("输入英文可看中文释义和词性；输入拼音可查找对应英文。");
            ui.label("每页 5 个候选，1–5 直接选词；Tab 接受首选，不添加尾空格。");
            ui.label("− / + 翻页（也支持 =、_ 和 PageUp / PageDown）；↑↓ 选择候选。");
            ui.label("自动空格开启时，Space 确认并补词前空格；连写只补已输入词的内部空格。");
            ui.label("单按 Shift 切换自动空格；关闭后使用 Space 手动隔词。");
            ui.label("Enter 原样提交当前输入，不选候选、不补空格；再次 Enter 才交给应用。");
            ui.label("Esc 保留原词；没有候选时，数字和翻页符号按原样输入。");
        });
    }
}

fn heading(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(26.0).strong());
    ui.add_space(18.0);
}
fn navigation(ui: &mut egui::Ui, page: Page, title: &str, selected: bool, height: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title)
    });
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 10, Color32::WHITE);
        painter.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 5.5, rect.center().y),
                Vec2::new(3.0, 16.0),
            ),
            2,
            ACCENT,
        );
    } else if response.hovered() {
        painter.rect_filled(
            rect,
            10,
            Color32::from_rgba_unmultiplied(255, 255, 255, 140),
        );
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            10,
            Stroke::new(1.0_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let color = if selected { ACCENT } else { INK };
    let origin = rect.left_center() + Vec2::new(26.0, 0.0);
    let name = match page {
        Page::General => "sliders-horizontal",
        Page::Input => "keyboard",
        Page::Appearance => "sun",
        Page::Dictionary => "book-open",
        Page::Statistics => "chart-no-axes-column",
        Page::Providers => "cpu",
        Page::Translation => "languages",
        Page::Voice => "mic",
        Page::Shortcuts => "command",
        Page::Lan => "monitor-smartphone",
        Page::Cloud => "cloud",
        Page::About => "info",
    };
    crate::icons::draw(painter, origin, name, 20.0, color);
    painter.text(
        rect.left_center() + Vec2::new(52.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(16.0),
        color,
    );
    response.clicked()
}
#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Plus,
    Trash,
    Refresh,
    Test,
    Check,
    Close,
    Sliders,
    Edit,
    Copy,
    ExternalLink,
}

fn draw_icon(painter: &egui::Painter, c: egui::Pos2, icon: Icon, color: Color32) {
    let name = match icon {
        Icon::Plus => "plus",
        Icon::Trash => "trash",
        Icon::Refresh => "refresh-cw",
        Icon::Test => "flask-conical",
        Icon::Check => "check",
        Icon::Close => "x",
        Icon::Sliders => "sliders-horizontal",
        Icon::Edit => "pencil",
        Icon::Copy => "copy",
        Icon::ExternalLink => "external-link",
    };
    crate::icons::draw(painter, c, name, 18.0, color);
}

pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    tooltip: &str,
    primary: bool,
    danger: bool,
    enabled: bool,
) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tooltip));
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    let (fill, border, fg) = if primary {
        (
            if hovered {
                Color32::from_rgb(15, 122, 128)
            } else {
                ACCENT
            },
            None,
            Color32::WHITE,
        )
    } else if danger {
        (
            if hovered {
                Color32::from_rgb(253, 240, 240)
            } else {
                Color32::WHITE
            },
            Some(Color32::from_rgb(238, 205, 205)),
            DANGER,
        )
    } else {
        (
            if hovered { ACCENT_SOFT } else { Color32::WHITE },
            Some(if hovered { ACCENT } else { BORDER }),
            if hovered { ACCENT } else { INK },
        )
    };
    let alpha = if enabled { 1.0 } else { 0.45 };
    painter.rect_filled(rect, 8, fill.gamma_multiply(alpha));
    if let Some(stroke_color) = border {
        painter.rect_stroke(
            rect,
            8,
            Stroke::new(1.0_f32, stroke_color.gamma_multiply(alpha)),
            egui::StrokeKind::Inside,
        );
    }
    draw_icon(painter, rect.center(), icon, fg.gamma_multiply(alpha));
    let clicked = enabled && response.clicked();
    response.on_hover_text(tooltip);
    clicked
}

pub(crate) fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(Color32::WHITE))
            .fill(ACCENT)
            .stroke(Stroke::NONE)
            .corner_radius(8),
    )
    .clicked()
}

pub(crate) fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(12)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}
pub(crate) fn switch_row(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = toggle(ui, value, label);
        });
    });
    changed
}
pub(crate) fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(42.0, 24.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        12,
        if *value {
            ACCENT
        } else {
            Color32::from_rgb(194, 204, 209)
        },
    );
    painter.circle_filled(
        egui::pos2(
            if *value {
                rect.right() - 12.0
            } else {
                rect.left() + 12.0
            },
            rect.center().y,
        ),
        9.0,
        Color32::WHITE,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            12,
            Stroke::new(1.0_f32, INK),
            egui::StrokeKind::Outside,
        );
    }
    response.clicked()
}
fn speed_value(ui: &mut egui::Ui, language: &str, unit: &str, speed: Option<u64>) {
    ui.label(RichText::new(language).size(13.0).color(MUTED));
    ui.label(
        RichText::new(speed.map(|v| v.to_string()).unwrap_or_else(|| "—".into()))
            .size(27.0)
            .strong(),
    );
    ui.label(RichText::new(unit).size(12.0).color(MUTED));
}
fn active_duration(ms: u64) -> String {
    if ms < 60_000 {
        format!("{:.0}秒", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}分钟", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}小时", ms as f64 / 3_600_000.0)
    }
}
fn chart(ui: &mut egui::Ui, data: &[(String, Option<u64>, Option<u64>)], quantity: bool) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), egui::Sense::hover());
    let painter = ui.painter();
    let max = data
        .iter()
        .map(|(_, ch, en)| ch.unwrap_or(0).max(en.unwrap_or(0)))
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let step = rect.width() / data.len().max(1) as f32;
    let baseline = rect.bottom() - 24.0;
    for (i, (label, ch, en)) in data.iter().enumerate() {
        let x = rect.left() + step * (i as f32 + 0.5);
        let width = (step * 0.28).min(20.0);
        let h_ch = ch.unwrap_or(0) as f32 / max * 110.0;
        let h_en = en.unwrap_or(0) as f32 / max * 110.0;
        let blue = Color32::from_rgb(115, 185, 210);

        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x - width - 1.0, baseline - h_ch),
                egui::pos2(x - 1.0, baseline),
            ),
            2,
            ACCENT,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x + 1.0, baseline - h_en),
                egui::pos2(x + width + 1.0, baseline),
            ),
            2,
            blue,
        );
        painter.text(
            egui::pos2(x, baseline + 10.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.0),
            MUTED,
        );
        let column = egui::Rect::from_min_max(
            egui::pos2(x - step / 2.0, rect.top()),
            egui::pos2(x + step / 2.0, rect.bottom()),
        )
        .intersect(ui.clip_rect());
        let hover = ui.interact(column, response.id.with(i), egui::Sense::hover());
        if hover.hovered() {
            hover.on_hover_text_at_pointer(format!(
                "{label}\n中文 {} {}\n英文 {} {}",
                ch.map(|v| v.to_string())
                    .unwrap_or_else(|| "样本不足".into()),
                if quantity { "字" } else { "字/分钟" },
                en.map(|v| v.to_string())
                    .unwrap_or_else(|| "样本不足".into()),
                if quantity { "词" } else { "词/分钟" }
            ));
        }
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(if quantity {
                "● 中文（字）"
            } else {
                "● 中文（字/分钟）"
            })
            .size(12.0)
            .color(ACCENT),
        );
        ui.label(
            RichText::new(if quantity {
                "● 英文（词）"
            } else {
                "● 英文（词/分钟）"
            })
            .size(12.0)
            .color(Color32::from_rgb(115, 185, 210)),
        );
    });
}

impl eframe::App for SettingsApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.lifecycle(ctx);
    }
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        egui::Panel::left("sidebar")
            .exact_size(220.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(221, 246, 248))
                    .inner_margin(16),
            )
            .show(root, |ui| {
                ui.add_space(8.0);
                let brand = ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(34.0)));
                    ui.label(RichText::new("英文助手").size(18.0).strong());
                });
                if brand.response.interact(egui::Sense::drag()).drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.add_space(20.0);
                let row_height = ((ui.available_height() - 6.0 * 4.0) / 7.0).clamp(34.0, 44.0);
                egui::ScrollArea::vertical()
                    .id_salt("sidebar-pages")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for (page, title) in [
                            (Page::General, "常规"),
                            (Page::Input, "输入与查词"),
                            (Page::Appearance, "候选外观"),
                            (Page::Shortcuts, "快捷键"),
                            (Page::Dictionary, "个人词库与数据"),
                            (Page::Providers, "AI 服务与编辑"),
                            (Page::About, "关于"),
                        ] {
                            if navigation(ui, page, title, self.page == page, row_height) {
                                self.select_page(page);
                            }
                        }
                    });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(40))
            .show(root, |ui| {
                let previous_error = self.error.clone();
                heading(
                    ui,
                    match self.page {
                        Page::General => "常规",
                        Page::Input => "输入与查词",
                        Page::Appearance => "候选外观",
                        Page::Dictionary => "个人词库与数据",
                        Page::Providers => "AI 服务与编辑",
                        Page::Translation => "AI 纠错与补句",
                        Page::Voice => "语音输入",
                        Page::Shortcuts => "快捷键",
                        Page::Statistics => "统计",
                        Page::Cloud => "云同步",
                        Page::Lan => "跨设备",
                        Page::About => "关于",
                    },
                );
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::DARK_RED, error);
                    ui.add_space(8.0);
                }
                ui.scope(|ui| {
                    let content_style = Arc::clone(ui.style());
                    let style = ui.style_mut();
                    // Reserve a separate gutter so the handle never covers cards.
                    style.spacing.scroll = egui::style::ScrollStyle {
                        content_margin: egui::Margin::ZERO,
                        fade: egui::style::ScrollFadeStyle {
                            strength: 0.0,
                            size: 0.0,
                        },
                        floating: true,
                        bar_width: 4.0,
                        floating_width: 4.0,
                        floating_allocated_width: 0.0,
                        handle_min_length: 28.0,
                        bar_inner_margin: 16.0,
                        bar_outer_margin: 2.0,
                        foreground_color: true,
                        dormant_handle_opacity: 1.0,
                        active_handle_opacity: 1.0,
                        interact_handle_opacity: 1.0,
                        dormant_background_opacity: 0.0,
                        active_background_opacity: 0.0,
                        interact_background_opacity: 0.0,
                    };
                    let scrollbar_gutter = style.spacing.scroll.bar_width
                        + style.spacing.scroll.bar_inner_margin
                        + style.spacing.scroll.bar_outer_margin;
                    style.visuals.extreme_bg_color = BACKGROUND;
                    style.visuals.widgets.inactive.fg_stroke.color =
                        Color32::from_rgb(205, 216, 222);
                    style.visuals.widgets.hovered.fg_stroke.color =
                        Color32::from_rgb(181, 198, 208);
                    style.visuals.widgets.active.fg_stroke.color = Color32::from_rgb(159, 183, 197);
                    egui::ScrollArea::vertical()
                        .id_salt(match self.page {
                            Page::General => "general-content",
                            Page::Input => "input-content",
                            Page::Appearance => "appearance-content",
                            Page::Dictionary => "dictionary-content",
                            Page::Providers => "providers-content",
                            Page::Translation => "translation-content",
                            Page::Voice => "voice-content",
                            Page::Shortcuts => "shortcuts-content",
                            Page::Statistics => "statistics-content",
                            Page::Cloud => "cloud-content",
                            Page::Lan => "lan-content",
                            Page::About => "about-content",
                        })
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Keep content width constant even when the bar is hidden.
                            ui.set_width((ui.available_width() - scrollbar_gutter).max(0.0));
                            // Scrollbar colors must not change page text or controls.
                            ui.set_style(content_style);
                            match self.page {
                                Page::General => self.general_page(ui),
                                Page::Input => self.input_page(ui),
                                Page::Appearance => self.appearance_page(ui),
                                Page::Dictionary => self.english_dictionary_page(ui),
                                Page::Providers => {
                                    self.ai.providers(ui);
                                    ui.add_space(16.0);
                                    egui::CollapsingHeader::new("语法修正与句子补全")
                                        .default_open(true)
                                        .show(ui, |ui| self.ai.translation(ui));
                                }
                                Page::Translation => {
                                    self.ai.translation(ui);
                                    if self.ai.config.selected().is_err()
                                        && ui.button("配置 AI 提供商").clicked()
                                    {
                                        self.select_page(Page::Providers);
                                    }
                                }
                                Page::Voice => self.ai.voice_page(ui),
                                Page::Shortcuts => {
                                    card(ui, |ui| {
                                        ui.label(RichText::new("候选键盘操作").strong());
                                        ui.label("1–5 选择当前页；Tab 接受首选或已选择项。");
                                        ui.label("− / _ 上一页；= / + 下一页；也支持 PageUp / PageDown。");
                                        ui.label("↑↓ 移动选择；Enter 原样提交当前输入；Esc 保留原词。");
                                        ui.label("单按 Shift 切换自动词前空格；确认候选不添加尾空格。");
                                        ui.label(RichText::new("这些是固定候选操作，AI 触发键可在下方设置。").size(12.0).color(MUTED));
                                    });
                                    ui.add_space(14.0);
                                    self.ai.shortcuts_page(ui);
                                }
                                Page::Statistics => self.statistics_page(ui),
                                Page::Cloud => {
                                    if let Some(cloud) = &mut self.cloud {
                                        cloud.ui(ui);
                                    }
                                }
                                Page::Lan => {
                                    self.lan.get_or_insert_with(crate::lan::LanPage::new).ui(ui);
                                }
                                Page::About => self.about_page(ui),
                            }
                        });
                });
                if self.error != previous_error {
                    // Actions inside the scroll area can fail during this frame.
                    // Reserve banner space before laying out the next frame.
                    ctx.request_repaint();
                }
            });
        // Foreground areas keep controls visible above every scrollable page.
        // No separate title/header row consumes space in the window.
        let bounds = ctx.content_rect();
        egui::Area::new(egui::Id::new("window-drag"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(238.0, 0.0))
            .show(&ctx, |ui| {
                let response = ui.allocate_response(
                    Vec2::new((bounds.width() - 298.0).max(1.0), 28.0),
                    egui::Sense::drag(),
                );
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            });
        egui::Area::new(egui::Id::new("window-close"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(bounds.right() - 48.0, 12.0))
            .show(&ctx, |ui| {
                if ui
                    .add_sized(
                        [36.0, 32.0],
                        egui::Button::new(RichText::new("×").size(24.0).color(MUTED)).frame(false),
                    )
                    .on_hover_text("关闭")
                    .clicked()
                {
                    self.hide(&ctx);
                }
            });
        if self.options.screenshot.is_some() && !self.screenshot_requested {
            if _frame.info().cpu_usage.is_some() {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            } else {
                ctx.request_repaint();
            }
        }
        if let Some(path) = &self.options.screenshot {
            for event in ctx.input(|input| input.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let result = (|| -> AppResult<()> {
                        let mut encoder = png::Encoder::new(
                            std::fs::File::create(path)?,
                            image.size[0] as u32,
                            image.size[1] as u32,
                        );
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        let pixels: Vec<u8> =
                            image.pixels.iter().flat_map(|c| c.to_array()).collect();
                        encoder.write_header()?.write_image_data(&pixels)?;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        self.error = Some(format!("截图失败：{error}"));
                    }
                }
            }
        }
        if let Some(delay) = self.options.hide_after {
            if self.started.elapsed() >= delay {
                self.options.hide_after = None;
                self.hide(&ctx);
            } else {
                ctx.request_repaint_after(delay.saturating_sub(self.started.elapsed()));
            }
        }
        if let Some(delay) = self.options.exit_after {
            if self.started.elapsed() >= delay {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(delay.saturating_sub(self.started.elapsed()));
            }
        }
        if !self.frame_logged && self.hidden_since.is_none() {
            self.frame_logged = true;
            self.instance.log_visible(self.open_request);
            crate::settings_log::event(
                "app",
                "ui_frame_complete",
                self.open_request,
                format!(
                    "open_to_frame_ms={} process_elapsed_ms={}",
                    self.open_started.elapsed().as_millis(),
                    self.started.elapsed().as_millis()
                ),
            );
        }
    }
}

pub fn run() -> AppResult<()> {
    let started = Instant::now();
    crate::settings_log::event("app", "options_parse", 0, "begin");
    let options = Options::parse()?;
    crate::settings_log::event(
        "app",
        "options_ready",
        options.request_id,
        format!(
            "renderer={:?} updates={}",
            options.renderer, options.updates
        ),
    );
    let Some(_guard) = instance::acquire(options.updates, options.request_id)? else {
        crate::settings_log::event(
            "app",
            "forwarded_exit",
            options.request_id,
            "existing instance notified",
        );
        return Ok(());
    };
    let native = eframe::NativeOptions {
        renderer: options.renderer,
        centered: true,
        persist_window: false,
        viewport: egui::ViewportBuilder::default()
            .with_title("English Assist 英文助手设置")
            .with_decorations(false)
            .with_inner_size([840.0, 600.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    let request = options.request_id;
    crate::settings_log::event(
        "app",
        "renderer_begin",
        request,
        format!("elapsed_ms={}", started.elapsed().as_millis()),
    );
    let result = eframe::run_native(
        "English Assist 英文助手设置",
        native,
        Box::new(move |cc| {
            SettingsApp::new(cc, options, started)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> {
                    error.to_string().into()
                })
        }),
    )
    .map_err(|error| error.to_string().into());
    crate::settings_log::event(
        "app",
        "renderer_end",
        request,
        format!(
            "success={} elapsed_ms={}",
            result.is_ok(),
            started.elapsed().as_millis()
        ),
    );
    result
}

fn version_label(version: &str) -> &str {
    if version.is_empty() {
        "历史未标记版本"
    } else {
        version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unsafe_code)]
    fn scheme_round_trip_uses_the_shared_dword_format() -> AppResult<()> {
        // Exercise Windows persistence without changing the user's input mode.
        let key = format!(
            "Software\\EnglishAssist\\Tests\\SettingsEgui-{}",
            std::process::id()
        );
        let result = (|| -> AppResult<()> {
            assert_eq!(read_scheme_at(&key)?, PinyinScheme::Full);
            write_scheme_at(&key, PinyinScheme::Flypy)?;
            assert_eq!(read_scheme_at(&key)?, PinyinScheme::Flypy);
            write_scheme_at(&key, PinyinScheme::Full)?;
            assert_eq!(read_scheme_at(&key)?, PinyinScheme::Full);
            Ok(())
        })();
        let key = backend::wide(&key);
        // SAFETY: Only the unique test key created above is removed.
        unsafe {
            windows::Win32::System::Registry::RegDeleteKeyW(
                windows::Win32::System::Registry::HKEY_CURRENT_USER,
                windows::core::PCWSTR(key.as_ptr()),
            )
            .ok()?;
        }
        result
    }
}
