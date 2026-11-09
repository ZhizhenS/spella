//! On-demand egui candidate rendering into a native-window-compatible bitmap.
//! No event loop, GPU device, keyboard interception, or platform calls live here.
use egui::{epaint, Color32, FontId, Pos2, Rect, Vec2};
use retype_types::RenderState;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

fn shared_cjk_font() -> Option<&'static [u8]> {
    static FONT: OnceLock<Option<retype_file_map::ReadOnlyFile>> = OnceLock::new();
    FONT.get_or_init(|| {
        let directory = std::path::PathBuf::from(std::env::var_os("WINDIR")?).join("Fonts");
        ["msyh.ttc", "msjh.ttc", "simhei.ttf", "simsun.ttc"]
            .iter()
            .find_map(|name| {
                retype_file_map::ReadOnlyFile::open(&directory.join(name), 64 * 1024 * 1024).ok()
            })
    })
    .as_ref()
    .map(AsRef::as_ref)
}

const BACKGROUND: Color32 = Color32::from_rgb(252, 252, 253);
const ACCENT: Color32 = Color32::from_rgb(19, 143, 150);
const INK: Color32 = Color32::from_rgb(52, 61, 68);
const CANDIDATE_FONT_SIZE: f32 = 16.0;
const MUTED: Color32 = Color32::from_rgb(142, 147, 154);

#[derive(Clone, Debug)]
pub struct Geometry {
    /// Physical pixel coordinates, also used for native mouse hit testing.
    pub cells: Vec<Rect>,
    pub width: usize,
    pub height: usize,
}

impl Geometry {
    pub fn hit_test(&self, x: f32, y: f32) -> Option<usize> {
        self.cells
            .iter()
            .position(|cell| x >= cell.min.x && x < cell.max.x && y >= cell.min.y && y < cell.max.y)
    }
}

pub struct Bitmap {
    pub geometry: Geometry,
    /// Opaque, top-down BGRX pixels for Win32 BI_RGB presentation.
    pub pixels: Vec<u32>,
}

struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
    alpha: Option<Vec<u8>>,
    monochrome: bool,
}

/// One cache per TSF apartment, retained across compositions. Fonts and atlas are
/// initialized lazily; idle windows do not run frames or timers.
pub struct Surface {
    appearance: retype_types::CandidateAppearance,
    context: egui::Context,
    textures: HashMap<egui::TextureId, Texture>,
    initialized: bool,
    widths: HashMap<String, (f32, u64)>,
    width_bytes: usize,
    width_tick: u64,
    width_scale: u32,
    last_frame: Option<(PaintKey, Arc<Bitmap>)>,
}

const WIDTH_CACHE_ENTRIES: usize = 4096;
const WIDTH_CACHE_BYTES: usize = 128 * 1024;

struct PaintKey {
    texts: Vec<String>,
    comments: Vec<String>,
    annotations: Vec<Option<retype_types::EnglishAnnotation>>,
    composition: String,
    english_header: String,
    page_start: usize,
    page_count: usize,
    selected: Option<usize>,
    numbered: bool,
    widths: Vec<i32>,
    gap: i32,
    viewport: i32,
    scale: u32,
}

fn painted_selection(state: &RenderState) -> Option<usize> {
    state
        .selected
        .checked_sub(state.page_start)
        .filter(|&index| index < state.visible().len())
}

/// Bound lookup evidence to one short badge; never expose the stored reading as a word.
fn english_source_badge(annotation: Option<&retype_types::EnglishAnnotation>) -> Option<String> {
    let annotation = annotation?;
    if !annotation.lexical_sense_id.is_empty() && !annotation.lexical_usage.is_empty() {
        let label: String = annotation
            .lexical_usage
            .chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .take(6)
            .collect();
        return (!label.is_empty()).then_some(label);
    }
    let (kind, evidence) = if !annotation.entity_kind.is_empty() {
        ("现代", &annotation.entity_kind)
    } else if annotation.pinyin_match {
        (
            if annotation.pinyin_exact {
                "拼音"
            } else {
                "扩展"
            },
            &annotation.matched_chinese,
        )
    } else {
        return None;
    };
    let mut characters = evidence
        .chars()
        .filter(|character| !character.is_whitespace() && !character.is_control());
    let mut headword: String = characters.by_ref().take(6).collect();
    if characters.next().is_some() {
        headword.push('…');
    }
    Some(if headword.is_empty() {
        kind.to_owned()
    } else {
        format!("{kind} · {headword}")
    })
}

/// Explain a candidate's inferred boundary without changing the raw composition.
fn english_header(state: &RenderState) -> String {
    let raw = &state.composition;
    if state.status.contains(retype_types::StatusFlags::CHINESE) || !raw.is_ascii() {
        return raw.clone();
    }
    let Some(candidate) = state.visible().get(painted_selection(state).unwrap_or(0)) else {
        return raw.clone();
    };
    let Some(display) = candidate
        .display_text
        .as_deref()
        .filter(|text| !text.is_empty())
    else {
        return raw.clone();
    };
    let Some(prefix) = candidate
        .text
        .strip_suffix(display)
        .filter(|prefix| prefix.ends_with(' '))
    else {
        return raw.clone();
    };
    let compact: String = prefix.chars().filter(|&c| c != ' ').collect();
    if compact.is_empty()
        || !compact.bytes().all(|c| c.is_ascii_alphabetic())
        || raw.len() <= compact.len()
        || !raw[..compact.len()].eq_ignore_ascii_case(&compact)
    {
        return raw.clone();
    }
    format!("{} · {}", prefix.trim_matches(' '), &raw[compact.len()..])
}

impl PaintKey {
    fn matches(
        &self,
        state: &RenderState,
        widths: &[i32],
        gap: i32,
        viewport: i32,
        scale: f32,
    ) -> bool {
        let english = !state.status.contains(retype_types::StatusFlags::CHINESE);
        self.selected == painted_selection(state)
            && self.numbered == state.status.contains(retype_types::StatusFlags::CHINESE)
            && (!english
                || (self.composition == state.composition
                    && self.english_header == english_header(state)
                    && self.page_start == state.page_start
                    && self.page_count == state.page_count()
                    && self
                        .comments
                        .iter()
                        .map(String::as_str)
                        .eq(state.visible().iter().map(|c| c.comment.as_str()))
                    && self
                        .annotations
                        .iter()
                        .eq(state.visible().iter().map(|c| &c.annotation))))
            && self.widths == widths
            && self.gap == gap
            && self.viewport == viewport
            && self.scale == scale.to_bits()
            && self
                .texts
                .iter()
                .map(String::as_str)
                .eq(state.visible().iter().map(|c| c.display_text()))
    }
}

impl Default for Surface {
    fn default() -> Self {
        let context = egui::Context::default();
        // The popup has a light background; dark-mode font coverage would
        // artificially thicken the edges of black-on-white candidate text.
        context.set_theme(egui::Theme::Light);
        let mut fonts = egui::FontDefinitions::default();
        if let Some(data) = shared_cjk_font() {
            fonts.font_data.insert(
                "candidate-cjk".into(),
                Arc::new(egui::FontData::from_static(data)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "candidate-cjk".into());
        }
        context.set_fonts(fonts);
        Self {
            appearance: retype_types::CandidateAppearance::default(),
            context,
            textures: HashMap::new(),
            initialized: false,
            widths: HashMap::new(),
            width_bytes: 0,
            width_tick: 0,
            width_scale: 0,
            last_frame: None,
        }
    }
}

impl Surface {
    pub fn set_appearance(&mut self, appearance: retype_types::CandidateAppearance) {
        let appearance = appearance.normalized();
        if self.appearance != appearance {
            self.appearance = appearance;
            self.context
                .set_theme(if appearance.theme == retype_types::CandidateTheme::Dark {
                    egui::Theme::Dark
                } else {
                    egui::Theme::Light
                });
            self.last_frame = None;
            self.widths.clear();
            self.width_bytes = 0;
        }
    }
    fn input(&self, width: f32, height: f32, scale: f32) -> egui::RawInput {
        self.context.set_pixels_per_point(scale);
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
            ..Default::default()
        }
    }

    fn apply_textures(&mut self, delta: &mut epaint::textures::TexturesDelta) {
        for (id, changes) in &delta.set {
            for change in changes {
                let epaint::ImageData::Color(image) = &change.image;
                let pixels = &image.pixels;
                let size = image.size;
                let monochrome = pixels.iter().all(|pixel| {
                    let [r, g, b, a] = pixel.to_array();
                    r == a && g == a && b == a
                });
                if let Some([x, y]) = change.pos {
                    if let Some(texture) = self.textures.get_mut(id) {
                        if !monochrome {
                            if let Some(alpha) = texture.alpha.take() {
                                texture.pixels = alpha
                                    .into_iter()
                                    .map(|a| Color32::from_rgba_premultiplied(a, a, a, a))
                                    .collect();
                            }
                        }
                        texture.monochrome &= monochrome;
                        for row in 0..size[1] {
                            let start = (y + row) * texture.size[0] + x;
                            if let Some(alpha) = &mut texture.alpha {
                                if let Some(target) = alpha.get_mut(start..start + size[0]) {
                                    for (to, from) in target
                                        .iter_mut()
                                        .zip(&pixels[row * size[0]..(row + 1) * size[0]])
                                    {
                                        *to = from.a();
                                    }
                                }
                            } else if let Some(target) =
                                texture.pixels.get_mut(start..start + size[0])
                            {
                                target.copy_from_slice(&pixels[row * size[0]..(row + 1) * size[0]]);
                            }
                        }
                    }
                } else {
                    self.textures.insert(
                        *id,
                        Texture {
                            size,
                            pixels: if monochrome {
                                Vec::new()
                            } else {
                                pixels.clone()
                            },
                            alpha: monochrome.then(|| pixels.iter().map(Color32::a).collect()),
                            monochrome,
                        },
                    );
                }
            }
        }
        delta.set.clear();
    }

    fn free_textures(&mut self, delta: &mut epaint::textures::TexturesDelta) {
        for id in &delta.free {
            self.textures.remove(id);
        }
        delta.clear();
    }

    fn initialize(&mut self, scale: f32) {
        // fonts() needs an initialized pass; keep its atlas delta for the first paint.
        if !self.initialized || self.context.pixels_per_point() != scale {
            let mut output = self.context.run_ui(self.input(480.0, 38.0, scale), |_| {});
            self.apply_textures(&mut output.textures_delta);
            self.free_textures(&mut output.textures_delta);
            self.initialized = true;
        }
    }

    pub fn measure(&mut self, state: &RenderState, available: i32, scale: f32) -> Vec<i32> {
        if !state.status.contains(retype_types::StatusFlags::CHINESE) {
            // English entries are vertical dictionary rows. Width is independent
            // of page capacity; the kernel keeps five entries on each page.
            return vec![
                ((self.appearance.width as f32 * scale).round() as i32)
                    .min(available)
                    .max(1);
                state.candidates.len()
            ];
        }
        if self.width_scale != scale.to_bits() {
            self.widths.clear();
            self.width_bytes = 0;
            self.width_scale = scale.to_bits();
        }
        self.initialize(scale);
        let chinese = state.status.contains(retype_types::StatusFlags::CHINESE);
        let text_padding = if chinese { 21.0 } else { 12.0 };
        self.context.fonts_mut(|fonts| {
            state
                .candidates
                .iter()
                .map(|candidate| {
                    let text = candidate.display_text();
                    self.width_tick = self.width_tick.wrapping_add(1);
                    let width = if let Some((width, used)) = self.widths.get_mut(text) {
                        *used = self.width_tick;
                        *width
                    } else {
                        let width = fonts
                            .layout_no_wrap(
                                text.to_owned(),
                                FontId::proportional(CANDIDATE_FONT_SIZE),
                                INK,
                            )
                            .size()
                            .x;
                        // Cache metrics, not galleys or font-atlas UV coordinates.
                        // The latter become invalid when egui rebuilds its atlas.
                        if text.len() <= WIDTH_CACHE_BYTES / 8 {
                            if self.widths.len() >= WIDTH_CACHE_ENTRIES
                                || self.width_bytes + text.len() > WIDTH_CACHE_BYTES
                            {
                                let cutoff = self
                                    .width_tick
                                    .saturating_sub((WIDTH_CACHE_ENTRIES / 2) as u64);
                                self.widths.retain(|_, (_, used)| *used > cutoff);
                                self.width_bytes = self.widths.keys().map(String::len).sum();
                                if self.widths.len() >= WIDTH_CACHE_ENTRIES
                                    || self.width_bytes + text.len() > WIDTH_CACHE_BYTES
                                {
                                    self.widths.clear();
                                    self.width_bytes = 0;
                                }
                            }
                            self.width_bytes += text.len();
                            self.widths
                                .insert(text.to_owned(), (width, self.width_tick));
                        }
                        width
                    };
                    // Round upwards: pagination must never underestimate the painted width.
                    ((width + text_padding + 1.0) * scale)
                        .ceil()
                        .max(36.0 * scale)
                        .min(available as f32) as i32
                })
                .collect()
        })
    }

    pub fn render(
        &mut self,
        state: &RenderState,
        widths: &[i32],
        gap: i32,
        viewport: i32,
        scale: f32,
    ) -> Arc<Bitmap> {
        if let Some((key, bitmap)) = &self.last_frame {
            if key.matches(state, widths, gap, viewport, scale) {
                return Arc::clone(bitmap);
            }
        }
        self.initialize(scale);
        if !state.status.contains(retype_types::StatusFlags::CHINESE) {
            return self.render_english(state, widths, gap, viewport, scale);
        }
        let pad = (4.0 * scale).round();
        let chinese = state.status.contains(retype_types::StatusFlags::CHINESE);
        // English digits are literal input, so only Chinese candidates advertise
        // numeric shortcuts. Match measure(), retaining one point of slack.
        let text_padding = if chinese { 21.0 } else { 12.0 };
        let text_left = if chinese { 17.0 } else { 6.0 };
        let layout_galleys = |context: &egui::Context| {
            context.fonts_mut(|fonts| {
                state
                    .visible()
                    .iter()
                    .zip(widths)
                    .map(|(candidate, width)| {
                        fonts.layout(
                            candidate.display_text().to_owned(),
                            FontId::proportional(CANDIDATE_FONT_SIZE),
                            INK,
                            (*width as f32 / scale - text_padding).max(1.0),
                        )
                    })
                    .collect::<Vec<_>>()
            })
        };
        // Only scalar metrics may cross begin_pass. It can recreate a full
        // font atlas, invalidating every previously laid-out galley's glyph UVs.
        let row = layout_galleys(&self.context)
            .iter()
            .map(|g| (g.size().y * scale).ceil() + (10.0 * scale).round())
            .fold((30.0 * scale).round(), f32::max);
        let mut x = pad;
        let cells: Vec<_> = widths
            .iter()
            .map(|width| {
                let rect = Rect::from_min_size(Pos2::new(x, pad), Vec2::new(*width as f32, row));
                x += *width as f32 + gap as f32;
                rect
            })
            .collect();
        let width = if cells.is_empty() {
            (60.0 * scale).round()
        } else {
            x - gap as f32 + pad
        }
        .min(viewport as f32)
        .max(1.0) as usize;
        let height = (row + 2.0 * pad).max(1.0) as usize;
        let geometry = Geometry {
            cells,
            width,
            height,
        };
        let input = self.input(width as f32 / scale, height as f32 / scale, scale);
        let mut output = self.context.run_ui(input, |ctx| {
            // Layout against this pass's atlas, just like the candidate numbers.
            let galleys = layout_galleys(ctx.ctx());
            let painter = ctx.layer_painter(egui::LayerId::background());
            let bounds =
                Rect::from_min_size(Pos2::ZERO, Vec2::new(width as f32, height as f32) / scale);
            painter.rect_filled(bounds, 5, BACKGROUND);
            painter.rect_stroke(
                bounds,
                5,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(223, 226, 230)),
                egui::StrokeKind::Inside,
            );
            for (i, (cell, galley)) in geometry.cells.iter().zip(&galleys).enumerate() {
                let cell = Rect::from_min_max(cell.min / scale, cell.max / scale);
                let selected = state.page_start + i == state.selected
                    && (chinese
                        || state
                            .status
                            .contains(retype_types::StatusFlags::ENGLISH_SELECTED));
                if selected {
                    painter.rect_filled(cell, 4, ACCENT);
                }
                if chinese {
                    painter.text(
                        Pos2::new(cell.left() + 4.0, cell.center().y),
                        egui::Align2::LEFT_CENTER,
                        (i + 1).to_string(),
                        FontId::proportional(11.0),
                        if selected { Color32::WHITE } else { MUTED },
                    );
                }
                painter.galley_with_override_text_color(
                    Pos2::new(cell.left() + text_left, cell.top() + 5.0),
                    Arc::clone(galley),
                    if selected { Color32::WHITE } else { INK },
                );
            }
        });
        self.apply_textures(&mut output.textures_delta);
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let mut pixels = vec![0x00fcfcfd; width * height];
        for primitive in primitives {
            if let epaint::Primitive::Mesh(mesh) = primitive.primitive {
                if let Some(texture) = self.textures.get(&mesh.texture_id) {
                    rasterize(
                        &mut pixels,
                        width,
                        height,
                        &mesh,
                        primitive.clip_rect,
                        scale,
                        texture,
                    );
                }
            }
        }
        self.free_textures(&mut output.textures_delta);
        let bitmap = Arc::new(Bitmap { geometry, pixels });
        self.last_frame = Some((
            PaintKey {
                texts: state
                    .visible()
                    .iter()
                    .map(|c| c.display_text().to_owned())
                    .collect(),
                comments: Vec::new(),
                annotations: Vec::new(),
                composition: String::new(),
                english_header: String::new(),
                page_start: 0,
                page_count: 0,
                selected: painted_selection(state),
                numbered: chinese,
                widths: widths.to_vec(),
                gap,
                viewport,
                scale: scale.to_bits(),
            },
            Arc::clone(&bitmap),
        ));
        bitmap
    }

    fn render_english(
        &mut self,
        state: &RenderState,
        widths: &[i32],
        gap: i32,
        viewport: i32,
        scale: f32,
    ) -> Arc<Bitmap> {
        let pad = 6.0;
        let header = 32.0;
        let appearance = self.appearance;
        let word_size = appearance.font_size as f32;
        let gloss_size = (word_size - 5.0).max(11.0);
        let row_height = if !appearance.show_meanings && !appearance.show_part_of_speech {
            word_size + 16.0
        } else {
            word_size + if appearance.compact { 27.0 } else { 45.0 }
        };
        let footer = if appearance.show_key_hints { 28.0 } else { 6.0 };
        let dark = appearance.theme == retype_types::CandidateTheme::Dark;
        let background = if dark {
            Color32::from_rgb(30, 38, 44)
        } else {
            BACKGROUND
        };
        let ink = if dark {
            Color32::from_rgb(235, 242, 246)
        } else {
            INK
        };
        let muted = if dark {
            Color32::from_rgb(155, 173, 184)
        } else {
            MUTED
        };
        let gloss_ink = if dark {
            Color32::from_rgb(177, 196, 207)
        } else {
            Color32::from_rgb(92, 111, 120)
        };
        let row_width = (appearance.width as f32 * scale)
            .min(viewport as f32 - 2.0 * pad * scale)
            .max(1.0);
        let width = (row_width + pad * 2.0 * scale)
            .min(viewport as f32)
            .max(1.0)
            .ceil() as usize;
        let logical_width = width as f32 / scale;
        let height = ((header + row_height * state.visible().len() as f32 + footer + pad) * scale)
            .ceil()
            .max(1.0) as usize;
        let cells = state
            .visible()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                Rect::from_min_size(
                    Pos2::new(pad * scale, (header + i as f32 * row_height) * scale),
                    Vec2::new(
                        (logical_width - pad * 2.0).max(1.0) * scale,
                        row_height * scale,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let geometry = Geometry {
            cells,
            width,
            height,
        };
        let input = self.input(logical_width, height as f32 / scale, scale);
        let mut output = self.context.run_ui(input, |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let bounds =
                Rect::from_min_size(Pos2::ZERO, Vec2::new(logical_width, height as f32 / scale));
            painter.rect_filled(bounds, 9, background);
            painter.rect_stroke(
                bounds,
                9,
                egui::Stroke::new(
                    1.0,
                    if dark {
                        Color32::from_rgb(63, 80, 89)
                    } else {
                        Color32::from_rgb(216, 228, 231)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let mut label = english_header(state);
            if label.chars().count() > 30 {
                label = label.chars().take(29).chain(['…']).collect();
            }
            painter.text(
                Pos2::new(14.0, 16.0),
                egui::Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.0),
                if dark {
                    Color32::from_rgb(96, 209, 207)
                } else {
                    ACCENT
                },
            );
            let page = state
                .page_starts
                .iter()
                .take_while(|&&start| start <= state.page_start)
                .count()
                .max(1);
            let page = if state.page_starts.is_empty() {
                state.page_start / state.page_size.max(1) + 1
            } else {
                page
            };
            painter.text(
                Pos2::new(logical_width - 14.0, 16.0),
                egui::Align2::RIGHT_CENTER,
                format!("{page} / {}", state.page_count().max(1)),
                FontId::proportional(11.0),
                muted,
            );
            for (i, (physical, candidate)) in geometry.cells.iter().zip(state.visible()).enumerate()
            {
                let cell = Rect::from_min_max(physical.min / scale, physical.max / scale);
                let selected = state.page_start + i == state.selected;
                if selected {
                    painter.rect_filled(
                        cell.shrink(1.0),
                        6,
                        if dark {
                            Color32::from_rgb(35, 70, 73)
                        } else {
                            Color32::from_rgb(232, 245, 245)
                        },
                    );
                }
                let badge = Rect::from_min_size(
                    Pos2::new(cell.left() + 7.0, cell.top() + 9.0),
                    Vec2::new(22.0, 22.0),
                );
                painter.rect_filled(
                    badge,
                    5,
                    if selected {
                        ACCENT
                    } else if dark {
                        Color32::from_rgb(47, 59, 68)
                    } else {
                        Color32::from_rgb(237, 241, 244)
                    },
                );
                painter.text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    FontId::proportional(12.0),
                    if selected { Color32::WHITE } else { muted },
                );
                let text_left = cell.left() + 39.0;
                let max_width = (cell.right() - text_left - 10.0).max(1.0);
                let source_badge =
                    english_source_badge(candidate.annotation.as_ref()).map(|label| {
                        painter.layout_job(clipped_text(
                            &label,
                            10.0,
                            muted,
                            (max_width * 0.4).min(112.0),
                            1,
                        ))
                    });
                let source_width = source_badge.as_ref().map_or(0.0, |badge| badge.size().x);
                if let Some(badge) = source_badge {
                    painter.galley(
                        Pos2::new(cell.right() - 10.0 - source_width, cell.top() + 9.0),
                        badge,
                        muted,
                    );
                }
                let word_text = if appearance.show_phonetics {
                    candidate
                        .annotation
                        .as_ref()
                        .filter(|a| !a.phonetic.is_empty())
                        .map_or_else(
                            || candidate.display_text().to_owned(),
                            |a| {
                                format!(
                                    "{}   /{}/",
                                    candidate.display_text(),
                                    a.phonetic.trim_matches('/')
                                )
                            },
                        )
                } else {
                    candidate.display_text().to_owned()
                };
                let word = painter.layout_job(clipped_text(
                    &word_text,
                    word_size,
                    ink,
                    (max_width
                        - if source_width > 0.0 {
                            source_width + 8.0
                        } else {
                            0.0
                        })
                    .max(1.0),
                    1,
                ));
                painter.galley(Pos2::new(text_left, cell.top() + 5.0), word, ink);
                let mut gloss = candidate.annotation_text(
                    appearance.show_meanings,
                    appearance.show_part_of_speech,
                    false,
                );
                if gloss.is_empty() && appearance.show_meanings {
                    gloss = "暂无中文释义".into();
                }
                let meaning = painter.layout_job(clipped_text(
                    &gloss,
                    gloss_size,
                    gloss_ink,
                    max_width,
                    if appearance.compact { 1 } else { 2 },
                ));
                painter.galley(
                    Pos2::new(text_left, cell.top() + word_size + 11.0),
                    meaning,
                    gloss_ink,
                );
            }
            if appearance.show_key_hints {
                painter.text(
                    Pos2::new(14.0, height as f32 / scale - 15.0),
                    egui::Align2::LEFT_CENTER,
                    "1–5 选词 · Tab 首选 · −/+ 翻页 · Enter 原样",
                    FontId::proportional(10.5),
                    muted,
                );
            }
        });
        self.apply_textures(&mut output.textures_delta);
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let mut pixels = vec![if dark { 0x001e262c } else { 0x00fcfcfd }; width * height];
        for primitive in primitives {
            if let epaint::Primitive::Mesh(mesh) = primitive.primitive {
                if let Some(texture) = self.textures.get(&mesh.texture_id) {
                    rasterize(
                        &mut pixels,
                        width,
                        height,
                        &mesh,
                        primitive.clip_rect,
                        scale,
                        texture,
                    );
                }
            }
        }
        self.free_textures(&mut output.textures_delta);
        let bitmap = Arc::new(Bitmap { geometry, pixels });
        self.last_frame = Some((
            PaintKey {
                texts: state
                    .visible()
                    .iter()
                    .map(|c| c.display_text().to_owned())
                    .collect(),
                comments: state.visible().iter().map(|c| c.comment.clone()).collect(),
                annotations: state
                    .visible()
                    .iter()
                    .map(|c| c.annotation.clone())
                    .collect(),
                composition: state.composition.clone(),
                english_header: english_header(state),
                page_start: state.page_start,
                page_count: state.page_count(),
                selected: painted_selection(state),
                numbered: false,
                widths: widths.to_vec(),
                gap,
                viewport,
                scale: scale.to_bits(),
            },
            Arc::clone(&bitmap),
        ));
        bitmap
    }
}

fn clipped_text(
    text: &str,
    size: f32,
    color: Color32,
    width: f32,
    rows: usize,
) -> egui::text::LayoutJob {
    let mut job =
        egui::text::LayoutJob::simple(text.to_owned(), FontId::proportional(size), color, width);
    job.wrap.max_rows = rows;
    job.wrap.overflow_character = Some('…');
    job
}

#[inline(always)]
fn edge(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
#[inline(always)]
fn top_left(a: Pos2, b: Pos2) -> bool {
    b.y < a.y || (b.y == a.y && b.x > a.x)
}

fn rasterize(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    mesh: &epaint::Mesh,
    clip: Rect,
    scale: f32,
    texture: &Texture,
) {
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let Some(a) = mesh.vertices.get(triangle[0] as usize) else {
            continue;
        };
        let Some(b) = mesh.vertices.get(triangle[1] as usize) else {
            continue;
        };
        let Some(c) = mesh.vertices.get(triangle[2] as usize) else {
            continue;
        };
        let mut vertices = [a, b, c];
        let mut points = [a.pos * scale, b.pos * scale, c.pos * scale];
        let mut area = edge(points[0], points[1], points[2]);
        if area < 0.0 {
            vertices.swap(1, 2);
            points.swap(1, 2);
            area = -area;
        }
        if area <= f32::EPSILON {
            continue;
        }
        let colors = vertices.map(|v| v.color.to_array().map(f32::from));
        let uniform_color = (a.color == b.color && b.color == c.color).then_some(colors[0]);
        let uniform_texel = (a.uv == b.uv && b.uv == c.uv).then(|| sample(texture, a.uv.to_vec2()));
        let solid = uniform_color.zip(uniform_texel).and_then(|(color, texel)| {
            (color[3] == 255.0 && texel[3] == 255.0).then(|| {
                let channel = |i: usize| (color[i] * texel[i] / 255.0).round() as u32;
                channel(0) << 16 | channel(1) << 8 | channel(2)
            })
        });
        let inclusive = [
            top_left(points[1], points[2]),
            top_left(points[2], points[0]),
            top_left(points[0], points[1]),
        ];
        let x0 = points
            .iter()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min)
            .max(clip.left() * scale)
            .floor()
            .max(0.0) as usize;
        let x1 = points
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(clip.right() * scale)
            .ceil()
            .min(width as f32)
            .max(0.0) as usize;
        let y0 = points
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min)
            .max(clip.top() * scale)
            .floor()
            .max(0.0) as usize;
        let y1 = points
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(clip.bottom() * scale)
            .ceil()
            .min(height as f32)
            .max(0.0) as usize;
        if let Some(color) = solid {
            // Convex triangles cover one continuous span per scanline. Find
            // its ends with the exact same edge/top-left tests as the pixel
            // path, then fill it without per-pixel geometry or blending.
            for y in y0..y1 {
                let py = y as f32 + 0.5;
                if py < clip.top() * scale || py >= clip.bottom() * scale {
                    continue;
                }
                let mut lo = x0.max((clip.left() * scale - 0.5).ceil().max(0.0) as usize);
                let mut hi = x1.min((clip.right() * scale - 0.5).ceil().max(0.0) as usize);
                for (i, (a, b)) in [
                    (points[1], points[2]),
                    (points[2], points[0]),
                    (points[0], points[1]),
                ]
                .into_iter()
                .enumerate()
                {
                    let inside = |x: usize| {
                        let value = edge(a, b, Pos2::new(x as f32 + 0.5, py));
                        value > 0.0 || value == 0.0 && inclusive[i]
                    };
                    if a.y == b.y {
                        if !inside(lo) {
                            hi = lo;
                        }
                    } else {
                        let increasing = a.y > b.y;
                        let (mut left, mut right) = (lo, hi);
                        while left < right {
                            let mid = left + (right - left) / 2;
                            if inside(mid) == increasing {
                                right = mid;
                            } else {
                                left = mid + 1;
                            }
                        }
                        if increasing {
                            lo = left;
                        } else {
                            hi = left;
                        }
                    }
                    if lo >= hi {
                        break;
                    }
                }
                if lo < hi {
                    pixels[y * width + lo..y * width + hi].fill(color);
                }
            }
            continue;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                if p.x < clip.left() * scale
                    || p.x >= clip.right() * scale
                    || p.y < clip.top() * scale
                    || p.y >= clip.bottom() * scale
                {
                    continue;
                }
                let e = [
                    edge(points[1], points[2], p),
                    edge(points[2], points[0], p),
                    edge(points[0], points[1], p),
                ];
                // Shared triangle edges belong to exactly one triangle (no double alpha).
                if e.iter()
                    .enumerate()
                    .any(|(i, v)| *v < 0.0 || (*v == 0.0 && !inclusive[i]))
                {
                    continue;
                }
                let bary = [e[0] / area, e[1] / area, e[2] / area];
                let uv = vertices
                    .iter()
                    .zip(bary)
                    .fold(Vec2::ZERO, |sum, (v, w)| sum + v.uv.to_vec2() * w);
                let texel = uniform_texel.unwrap_or_else(|| sample(texture, uv));
                let mut source = [0.0; 4];
                for (channel, output) in source.iter_mut().enumerate() {
                    let color = uniform_color
                        .map(|color| color[channel])
                        .unwrap_or_else(|| {
                            colors[0][channel] * bary[0]
                                + colors[1][channel] * bary[1]
                                + colors[2][channel] * bary[2]
                        });
                    *output = color * texel[channel] / 255.0;
                }
                let destination = pixels[y * width + x];
                let inverse = 1.0 - source[3] / 255.0;
                let channels = [
                    ((destination >> 16) & 255) as f32,
                    ((destination >> 8) & 255) as f32,
                    (destination & 255) as f32,
                ];
                let blend = |i: usize| {
                    (source[i] + channels[i] * inverse)
                        .round()
                        .clamp(0.0, 255.0) as u32
                };
                pixels[y * width + x] = blend(0) << 16 | blend(1) << 8 | blend(2);
            }
        }
    }
}

#[inline(always)]
fn sample(texture: &Texture, uv: Vec2) -> [f32; 4] {
    let x = (uv.x * texture.size[0] as f32 - 0.5).clamp(0.0, (texture.size[0] - 1) as f32);
    let y = (uv.y * texture.size[1] as f32 - 0.5).clamp(0.0, (texture.size[1] - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(texture.size[0] - 1);
    let y1 = (y0 + 1).min(texture.size[1] - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    if texture.monochrome {
        let at = |x: usize, y: usize| {
            let index = y * texture.size[0] + x;
            texture
                .alpha
                .as_ref()
                .map_or_else(|| texture.pixels[index].a(), |a| a[index]) as f32
        };
        let alpha = (at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx) * (1.0 - ty)
            + (at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx) * ty;
        return [alpha; 4];
    }
    let mut result = [0.0; 4];
    for (channel, value) in result.iter_mut().enumerate() {
        let at =
            |x: usize, y: usize| texture.pixels[y * texture.size[0] + x].to_array()[channel] as f32;
        *value = (at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx) * (1.0 - ty)
            + (at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx) * ty;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use retype_types::{Candidate, CandidateSource};
    #[test]
    fn coverage_texture_preserves_every_sample_and_promotes_colored_updates() {
        let mut surface = Surface::default();
        let id = egui::TextureId::Managed(123);
        let colors: Vec<_> = [0, 64, 128, 255]
            .into_iter()
            .map(|a| Color32::from_rgba_premultiplied(a, a, a, a))
            .collect();
        let rgba = Texture {
            size: [2, 2],
            pixels: colors.clone(),
            alpha: None,
            monochrome: true,
        };
        let mut delta = epaint::textures::TexturesDelta::default();
        delta.set.insert(
            id,
            vec![epaint::ImageDelta::full(
                egui::ColorImage::new([2, 2], colors),
                egui::TextureOptions::LINEAR,
            )]
            .into(),
        );
        surface.apply_textures(&mut delta);
        let stored = &surface.textures[&id];
        assert!(stored.pixels.is_empty());
        for x in 0..=10 {
            for y in 0..=10 {
                let uv = Vec2::new(x as f32 / 10.0, y as f32 / 10.0);
                assert_eq!(sample(&rgba, uv), sample(stored, uv));
            }
        }
        delta.set.insert(
            id,
            vec![epaint::ImageDelta::partial(
                [1, 1],
                egui::ColorImage::new([1, 1], vec![Color32::RED]),
                egui::TextureOptions::LINEAR,
            )]
            .into(),
        );
        surface.apply_textures(&mut delta);
        let stored = &surface.textures[&id];
        assert!(stored.alpha.is_none());
        assert!(!stored.monochrome);
        assert_eq!(stored.pixels[3], Color32::RED);
        assert_eq!(
            stored.pixels[1],
            Color32::from_rgba_premultiplied(64, 64, 64, 64)
        );
    }
    fn state(texts: &[&str]) -> RenderState {
        RenderState {
            status: retype_types::StatusFlags::CHINESE,
            composition: "nihao".into(),
            page_size: texts.len(),
            candidates: texts
                .iter()
                .map(|text| Candidate::new(*text, CandidateSource::Local))
                .collect(),
            ..Default::default()
        }
    }
    #[test]
    fn pinyin_badge_distinguishes_exact_headwords_from_expansions_and_bounds_labels() {
        let mut annotation = retype_types::EnglishAnnotation {
            pinyin_match: true,
            matched_chinese: "想".into(),
            matched_pinyin: "xiang3".into(),
            pinyin_exact: true,
            ..Default::default()
        };
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("拼音 · 想")
        );
        annotation.matched_chinese = "想法".into();
        annotation.matched_pinyin = "xiang3 fa3".into();
        annotation.pinyin_exact = false;
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("扩展 · 想法")
        );
        annotation.matched_chinese = "一二三四五六七八九十".into();
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("扩展 · 一二三四五六…")
        );
        annotation.matched_chinese = " \n想\t法\r ".into();
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("扩展 · 想法")
        );
        annotation.matched_chinese.clear();
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("扩展")
        );
        annotation.pinyin_match = false;
        assert_eq!(english_source_badge(Some(&annotation)), None);
        assert_eq!(english_source_badge(None), None);
    }

    #[test]
    fn modern_entity_badge_uses_bounded_kind_without_exposing_its_source() {
        let mut annotation = retype_types::EnglishAnnotation {
            entity_kind: "公司".into(),
            entity_source: "https://www.anthropic.com/".into(),
            ..Default::default()
        };
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("现代 · 公司")
        );
        annotation.entity_kind = "产品".into();
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("现代 · 产品")
        );
        annotation.entity_kind = "一二三四五六七八九十".into();
        assert_eq!(
            english_source_badge(Some(&annotation)).as_deref(),
            Some("现代 · 一二三四五六…")
        );
        annotation.entity_kind.clear();
        assert_eq!(english_source_badge(Some(&annotation)), None);
    }

    #[test]
    fn modern_dictionary_usage_is_not_an_entity_or_pinyin_badge() {
        let mut annotation = retype_types::EnglishAnnotation {
            lexical_usage: "网络".into(),
            lexical_sense_id: "conversation-one".into(),
            expanded_form: "I don't know".into(),
            lexical_source: "https://example.org/dictionary".into(),
            ..Default::default()
        };
        for usage in ["网络", "缩写", "俚语", "非正式", "专业", "旧义", "词汇"] {
            annotation.lexical_usage = usage.into();
            assert_eq!(
                english_source_badge(Some(&annotation)).as_deref(),
                Some(usage)
            );
        }
        annotation.lexical_sense_id.clear();
        assert_eq!(english_source_badge(Some(&annotation)), None);
    }

    #[test]
    fn modern_sense_usage_and_expansion_changes_refresh_without_changing_commit() {
        let mut surface = Surface::default();
        let mut current = state(&["IMO"]);
        current.status = retype_types::StatusFlags::EMPTY;
        current.composition = "imo".into();
        current.candidates[0].annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "短语".into(),
            definition: "依我看".into(),
            lexical_usage: "缩写".into(),
            expanded_form: "in my opinion".into(),
            lexical_sense_id: "imo-chat".into(),
            ..Default::default()
        });
        let widths = surface.measure(&current, 472, 1.0);
        let first = surface.render(&current, &widths, 2, 480, 1.0);
        let cached = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(Arc::ptr_eq(&first, &cached));
        let annotation = current.candidates[0].annotation.as_mut().unwrap();
        annotation.part_of_speech = "名词".into();
        annotation.definition = "国际海事组织".into();
        annotation.lexical_usage = "专业".into();
        annotation.expanded_form = "International Maritime Organization".into();
        annotation.lexical_sense_id = "imo-organization".into();
        let second = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(!Arc::ptr_eq(&first, &second));
        assert_ne!(first.pixels, second.pixels);
        assert_eq!(current.candidates[0].text, "IMO");
        assert_eq!(current.candidates[0].display_text(), "IMO");
    }

    #[test]
    fn mixed_pinyin_evidence_changes_the_badge_but_not_the_prefix_or_commit() {
        for raw in ["iwilldotheyiyang", "iwilldothetongyang"] {
            let mut current = state(&["I will do the same"]);
            current.status = retype_types::StatusFlags::EMPTY;
            current.composition = raw.into();
            current.candidates[0].display_text = Some("same".into());
            current.candidates[0].annotation = Some(retype_types::EnglishAnnotation {
                pinyin_match: true,
                matched_chinese: if raw.ends_with("yiyang") {
                    "一样"
                } else {
                    "同样"
                }
                .into(),
                pinyin_exact: true,
                ..Default::default()
            });
            let before = current.clone();
            assert_eq!(
                english_header(&current),
                format!("I will do the · {}", &raw[10..])
            );
            assert_eq!(current.candidates[0].display_text(), "same");
            assert_eq!(current.candidates[0].text, "I will do the same");
            assert!(
                english_source_badge(current.candidates[0].annotation.as_ref())
                    .unwrap()
                    .starts_with("拼音 · ")
            );
            assert_eq!(current, before);
        }
    }

    #[test]
    fn inferred_english_header_requires_an_exact_raw_prefix_and_stays_display_only() {
        for (raw, commit, display, chinese, expected) in [
            ("iamg", "I am going", Some("going"), false, "I am · g"),
            ("ia", "I am", Some("am"), false, "I · a"),
            (
                "iamt",
                "I am trying to",
                Some("trying to"),
                false,
                "I am · t",
            ),
            ("iamtr", "I am trying", Some("trying"), false, "I am · tr"),
            (
                "iwilldotheyiyang",
                "I will do the same",
                Some("same"),
                false,
                "I will do the · yiyang",
            ),
            (
                "iwilldothetongyang",
                "I will do the same",
                Some("same"),
                false,
                "I will do the · tongyang",
            ),
            (
                "iwilldotheyiyang",
                "I will do a same",
                Some("same"),
                false,
                "iwilldotheyiyang",
            ),
            (
                "iwilldothetongyang",
                "I will do the identical",
                Some("same"),
                false,
                "iwilldothetongyang",
            ),
            ("IAMG", "I am going", Some("going"), false, "I am · G"),
            (
                "wearego",
                "We are going",
                Some("going"),
                false,
                "We are · go",
            ),
            ("iamg", "You are going", Some("going"), false, "iamg"),
            ("iamg", "I am going", Some("trying"), false, "iamg"),
            ("iamg", "Iamgoing", Some("going"), false, "iamg"),
            ("iam", "I am going", Some("going"), false, "iam"),
            ("going", "going", Some("going"), false, "going"),
            ("going", "going", None, false, "going"),
            ("你好g", "你好 going", Some("going"), false, "你好g"),
            ("iamg", "I am going", Some("going"), true, "iamg"),
        ] {
            let mut current = state(&[commit]);
            current.status = if chinese {
                retype_types::StatusFlags::CHINESE
            } else {
                retype_types::StatusFlags::EMPTY
            };
            current.composition = raw.into();
            current.candidates[0].display_text = display.map(str::to_owned);
            let before = current.clone();
            assert_eq!(
                english_header(&current),
                expected,
                "{raw:?} / {commit:?} / {display:?}"
            );
            assert_eq!(
                current, before,
                "deriving a header must not change input or commit text"
            );
        }
        let mut empty = state(&[]);
        empty.status = retype_types::StatusFlags::EMPTY;
        empty.composition = "iamg".into();
        assert_eq!(english_header(&empty), "iamg");
    }

    #[test]
    fn inferred_header_tracks_selection_page_and_hidden_commit_boundary_changes() {
        let mut current = state(&["I am going", "I am glad"]);
        current.status = retype_types::StatusFlags::EMPTY;
        current.composition = "iamg".into();
        current.candidates[0].display_text = Some("going".into());
        current.candidates[1].display_text = Some("am glad".into());
        assert_eq!(english_header(&current), "I am · g");
        current.selected = 1;
        assert_eq!(english_header(&current), "I · amg");
        current.page_size = 1;
        current.page_start = 1;
        assert_eq!(english_header(&current), "I · amg");

        current.page_start = 0;
        current.selected = 0;
        let mut surface = Surface::default();
        let widths = surface.measure(&current, 472, 1.0);
        let first = surface.render(&current, &widths, 2, 480, 1.0);
        current.candidates[0].text = "I a m going".into();
        assert_eq!(english_header(&current), "I a m · g");
        let changed = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(
            !Arc::ptr_eq(&first, &changed),
            "derived header changes must invalidate an otherwise identical row"
        );
        assert_ne!(first.pixels, changed.pixels);
        let row_start = first.geometry.cells[0].min.y as usize * first.geometry.width;
        assert_eq!(
            &first.pixels[row_start..],
            &changed.pixels[row_start..],
            "the inferred prefix belongs only in the header"
        );
    }
    #[test]
    fn candidate_text_survives_font_atlas_recreation() {
        fn small_atlas(surface: &mut Surface, scale: f32) {
            let mut input = surface.input(480.0, 38.0, scale);
            input.max_texture_side = Some(1024);
            let mut output = surface.context.run_ui(input, |_| {});
            surface.apply_textures(&mut output.textures_delta);
            surface.free_textures(&mut output.textures_delta);
            surface.initialized = true;
        }
        for scale in [1.0, 1.5, 2.0] {
            let current = state(&["你好", "我是", "快捷键", "Rust"]);
            let available = (472.0 * scale) as i32;
            let gap = (2.0 * scale) as i32;
            let viewport = (480.0 * scale) as i32;
            let mut fresh = Surface::default();
            small_atlas(&mut fresh, scale);
            let widths = fresh.measure(&current, available, scale);
            let expected = fresh.render(&current, &widths, gap, viewport, scale);

            let mut surface = Surface::default();
            small_atlas(&mut surface, scale);
            // Populate the atlas outside a paint pass, as measurement does.
            // Distinct sizes work even on systems without a CJK font. Stop
            // before overflow; begin_pass recreates it at the real 80% threshold.
            surface.context.fonts_mut(|fonts| {
                'fill: for size in 16..80 {
                    for ch in ' '..='~' {
                        if fonts.font_atlas_fill_ratio() > 0.82 {
                            break 'fill;
                        }
                        fonts.layout_no_wrap(
                            ch.to_string(),
                            FontId::proportional(size as f32),
                            INK,
                        );
                    }
                }
                assert!(fonts.font_atlas_fill_ratio() > 0.8);
            });
            let widths = surface.measure(&current, available, scale);
            let actual = surface.render(&current, &widths, gap, viewport, scale);
            assert_eq!(actual.geometry.width, expected.geometry.width);
            assert_eq!(actual.geometry.height, expected.geometry.height);
            assert!(
                actual.pixels == expected.pixels,
                "atlas reset at scale {scale}"
            );
            assert!(Arc::ptr_eq(
                &actual,
                &surface.render(&current, &widths, gap, viewport, scale)
            ));
        }
    }
    #[test]
    fn same_metrics_drive_pixels_and_clicks_at_multiple_scales() {
        let mut surface = Surface::default();
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let state = state(&["你好", "我是", "快捷键", "乔布斯", "额度"]);
            let available = (472.0 * scale) as i32;
            let widths = surface.measure(&state, available, scale);
            let bitmap = surface.render(
                &state,
                &widths,
                (2.0 * scale) as i32,
                (480.0 * scale) as i32,
                scale,
            );
            assert!(bitmap.geometry.width <= (480.0 * scale) as usize);
            assert_eq!(
                bitmap.pixels.len(),
                bitmap.geometry.width * bitmap.geometry.height
            );
            for (i, cell) in bitmap.geometry.cells.iter().enumerate() {
                assert!(
                    surface
                        .context
                        .fonts_mut(|fonts| fonts
                            .layout_no_wrap(
                                state.candidates[i].text.clone(),
                                FontId::proportional(CANDIDATE_FONT_SIZE),
                                INK
                            )
                            .size()
                            .x)
                        .ceil()
                        <= (cell.width() / scale - 21.0).ceil()
                );
                assert_eq!(
                    bitmap.geometry.hit_test(cell.center().x, cell.center().y),
                    Some(i)
                );
            }
            assert_eq!(bitmap.geometry.hit_test(0.0, 0.0), None);
        }
    }
    #[test]
    fn english_dictionary_rows_keep_click_targets_at_every_scale() {
        let mut surface = Surface::default();
        let mut current = state(&["help", "hello", "helpful", "helper", "helping"]);
        current.status = retype_types::StatusFlags::EMPTY;
        current.candidates[0].annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "v. / n.".into(),
            definition: "帮助；援助；有用的东西".into(),
            phonetic: "help".into(),
            pinyin_match: false,
            ..Default::default()
        });
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let widths = surface.measure(&current, (472.0 * scale) as i32, scale);
            let bitmap = surface.render(
                &current,
                &widths,
                (2.0 * scale) as i32,
                (480.0 * scale) as i32,
                scale,
            );
            assert_eq!(bitmap.geometry.cells.len(), 5);
            assert!(bitmap.geometry.width <= (480.0 * scale) as usize);
            assert!(bitmap.geometry.height > (250.0 * scale) as usize);
            for (i, cell) in bitmap.geometry.cells.iter().enumerate() {
                assert_eq!(
                    bitmap.geometry.hit_test(cell.center().x, cell.center().y),
                    Some(i)
                );
                if i > 0 {
                    assert!(cell.top() >= bitmap.geometry.cells[i - 1].bottom());
                }
            }
        }
    }

    #[test]
    fn dictionary_metadata_and_page_headers_invalidate_the_painted_frame() {
        let mut surface = Surface::default();
        let mut current = state(&["apple", "application"]);
        current.status = retype_types::StatusFlags::EMPTY;
        current.candidates[0].annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "n.".into(),
            definition: "苹果；苹果树".into(),
            ..Default::default()
        });
        let widths = surface.measure(&current, 472, 1.0);
        let first = surface.render(&current, &widths, 2, 480, 1.0);
        current.candidates[0]
            .annotation
            .as_mut()
            .unwrap()
            .definition = "应用；申请；用途".into();
        let changed = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(!Arc::ptr_eq(&first, &changed));
        assert!(
            first.pixels != changed.pixels,
            "a changed definition must repaint"
        );
        current.composition = "pingguo".into();
        let pinyin = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(
            changed.pixels != pinyin.pixels,
            "the query header must repaint"
        );
        let cached = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(Arc::ptr_eq(&pinyin, &cached));
    }

    #[test]
    fn appearance_changes_apply_to_existing_candidates() {
        use retype_types::{CandidateAppearance, CandidateTheme};
        let mut surface = Surface::default();
        let mut current = state(&["example", "examples"]);
        current.status = retype_types::StatusFlags::EMPTY;
        current.candidates[0].comment = "n. 例子；榜样".into();
        let widths = surface.measure(&current, 472, 1.0);
        let light = surface.render(&current, &widths, 2, 480, 1.0);
        surface.set_appearance(CandidateAppearance {
            theme: CandidateTheme::Dark,
            ..Default::default()
        });
        let dark = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(
            light.pixels != dark.pixels,
            "theme must change actual pixels"
        );
        surface.set_appearance(CandidateAppearance {
            compact: true,
            show_meanings: false,
            show_part_of_speech: false,
            show_key_hints: false,
            width: 320,
            ..Default::default()
        });
        let widths = surface.measure(&current, 472, 1.0);
        let compact = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(compact.geometry.height < dark.geometry.height);
        assert!(compact.geometry.width < dark.geometry.width);
    }
    #[test]
    fn english_suffix_label_is_painted_and_invalidates_without_changing_commit_text() {
        let mut surface = Surface::default();
        surface.set_appearance(retype_types::CandidateAppearance {
            show_phonetics: true,
            ..Default::default()
        });
        let mut current = state(&["I am trying to"]);
        current.status = retype_types::StatusFlags::EMPTY;
        current.composition = "iamt".into();
        current.candidates[0].language = retype_types::CandidateLanguage::English;
        current.candidates[0].display_text = Some("trying to".into());
        current.candidates[0].annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "v.".into(),
            definition: "尝试".into(),
            phonetic: "traɪɪŋ".into(),
            pinyin_match: false,
            ..Default::default()
        });
        let widths = surface.measure(&current, 472, 1.0);
        let suffix = surface.render(&current, &widths, 2, 480, 1.0);
        current.candidates[0].text = "You are trying to".into();
        let hidden_change = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(
            !Arc::ptr_eq(&suffix, &hidden_change),
            "a changed inferred header must repaint"
        );
        let row_start = suffix.geometry.cells[0].min.y as usize * suffix.geometry.width;
        assert_eq!(
            &suffix.pixels[row_start..],
            &hidden_change.pixels[row_start..],
            "hidden commit prefixes must not change the candidate row"
        );
        current.candidates[0].display_text = Some("trying".into());
        let changed = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(!Arc::ptr_eq(&hidden_change, &changed));
        assert_ne!(
            suffix.pixels, changed.pixels,
            "display-only changes must update actual pixels"
        );
        assert_eq!(current.candidates[0].text, "You are trying to");
        current.candidates[0].text = "trying to".into();
        current.candidates[0].display_text = None;
        let plain_suffix = surface.render(&current, &widths, 2, 480, 1.0);
        assert_eq!(
            &suffix.pixels[row_start..],
            &plain_suffix.pixels[row_start..],
            "the displayed suffix must not contain its completed prefix"
        );
    }
    #[test]
    fn long_phrase_wraps_without_ellipsis_and_atlas_survives_updates() {
        let mut surface = Surface::default();
        let state = state(&["这是一个很长的候选词组用于验证完整显示而不是省略号"
            .repeat(4)
            .as_str()]);
        let widths = surface.measure(&state, 472, 1.0);
        let bitmap = surface.render(&state, &widths, 2, 480, 1.0);
        assert_eq!(bitmap.geometry.width, 480);
        assert!(bitmap.geometry.height > 38);
        let again = surface.render(&state, &widths, 2, 480, 1.0);
        assert_eq!(bitmap.pixels, again.pixels);
    }

    #[test]
    fn selected_cell_tracks_absolute_index_on_later_pages() {
        let mut surface = Surface::default();
        let mut state = state(&["之前", "上一页", "你好", "额度"]);
        state.page_start = 2;
        state.page_size = 2;
        state.selected = 3;
        let widths = surface.measure(&state, 472, 1.0);
        let image = surface.render(&state, &widths[2..], 2, 480, 1.0);
        let cell = image.geometry.cells[1];
        let pixel = image.pixels
            [(cell.bottom() as usize - 3) * image.geometry.width + cell.center().x as usize];
        assert_eq!(pixel, 0x00138f96);
        assert_eq!(
            image
                .geometry
                .hit_test(cell.center().x, cell.center().y)
                .map(|i| state.page_start + i),
            Some(3)
        );
    }

    #[test]
    fn cached_widths_survive_other_frames_and_reclamp_for_viewport_and_dpi() {
        let mut surface = Surface::default();
        let original = state(&["我", "额度", "快捷键", "a longer English completion"]);
        let widths = surface.measure(&original, 472, 1.0);
        for _ in 0..5 {
            let other = state(&["另一组候选"]);
            let measured = surface.measure(&other, 472, 1.0);
            surface.render(&other, &measured, 2, 480, 1.0);
        }
        assert_eq!(surface.measure(&original, 472, 1.0), widths);
        assert_eq!(
            surface.measure(&original, 80, 1.0),
            widths.iter().map(|&w| w.min(80)).collect::<Vec<_>>()
        );
        for scale in [1.5, 2.0, 1.0] {
            let mut fresh = Surface::default();
            assert_eq!(
                surface.measure(&original, (472.0 * scale) as i32, scale),
                fresh.measure(&original, (472.0 * scale) as i32, scale)
            );
        }
        // Eviction changes storage, never the measured result.
        let many: Vec<_> = (0..WIDTH_CACHE_ENTRIES + 32)
            .map(|i| format!("word{i}"))
            .collect();
        let refs: Vec<_> = many.iter().map(String::as_str).collect();
        surface.measure(&state(&refs), 472, 1.0);
        assert!(surface.widths.len() <= WIDTH_CACHE_ENTRIES);
        assert!(surface.width_bytes <= WIDTH_CACHE_BYTES);
        assert_eq!(surface.measure(&original, 472, 1.0), widths);
    }

    #[test]
    fn bitmap_reuse_ignores_metadata_but_tracks_every_visual_input() {
        let mut surface = Surface::default();
        let mut current = state(&["你好", "额度"]);
        let widths = surface.measure(&current, 472, 1.0);
        let original = surface.render(&current, &widths, 2, 480, 1.0);
        current.gen += 1;
        current.composition = "nihc".into();
        current.candidates[0].score += 1.0;
        let same = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(Arc::ptr_eq(&original, &same));
        current.selected = 1;
        let selected = surface.render(&current, &widths, 2, 480, 1.0);
        assert!(!Arc::ptr_eq(&same, &selected));
        assert_ne!(same.pixels, selected.pixels);
        current.status = retype_types::StatusFlags::EMPTY;
        let english = surface.render(&current, &widths, 2, 480, 1.0);
        assert_ne!(selected.pixels, english.pixels);
        current.candidates[0].text = "再见".into();
        let changed = surface.render(&current, &widths, 2, 480, 1.0);
        assert_ne!(english.pixels, changed.pixels);
        let resized = surface.render(&current, &widths, 4, 300, 1.0);
        assert!(!Arc::ptr_eq(&changed, &resized));
        let scaled_widths = surface.measure(&current, 944, 2.0);
        let scaled = surface.render(&current, &scaled_widths, 4, 960, 2.0);
        assert!(!Arc::ptr_eq(&resized, &scaled));
    }

    #[test]
    fn solid_scanlines_match_pixel_path_for_rotated_clipped_and_shared_edges() {
        let texture = Texture {
            size: [1, 1],
            pixels: vec![Color32::WHITE],
            alpha: None,
            monochrome: true,
        };
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for shift in [0.0, 0.25, 0.5, 0.9] {
                let mut mesh = epaint::Mesh {
                    vertices: [
                        Pos2::new(0.0, 1.0),
                        Pos2::new(9.0, 0.0),
                        Pos2::new(7.0, 10.0),
                        Pos2::new(1.0, 8.0),
                    ]
                    .map(|p| epaint::Vertex {
                        pos: p + Vec2::splat(shift),
                        uv: Pos2::ZERO,
                        color: Color32::from_rgb(19, 143, 150),
                    })
                    .to_vec(),
                    indices: vec![0, 1, 2, 0, 2, 3],
                    ..Default::default()
                };
                let clip = Rect::from_min_max(Pos2::new(0.6, 0.7), Pos2::new(8.1, 9.2));
                let mut fast = vec![0; 24 * 24];
                rasterize(&mut fast, 24, 24, &mesh, clip, scale, &texture);
                // Unequal UVs force the per-pixel path, but a 1x1 white
                // texture still produces exactly the same color and coverage.
                for (i, vertex) in mesh.vertices.iter_mut().enumerate() {
                    vertex.uv = Pos2::new(i as f32 * 0.1, 0.0);
                }
                let mut reference = vec![0; 24 * 24];
                rasterize(&mut reference, 24, 24, &mesh, clip, scale, &texture);
                assert_eq!(fast, reference, "scale={scale} shift={shift}");
            }
        }
    }

    #[test]
    fn shared_edges_and_clip_do_not_double_blend_or_overdraw() {
        let texture = Texture {
            size: [1, 1],
            pixels: vec![Color32::WHITE],
            alpha: None,
            monochrome: true,
        };
        let mut mesh = epaint::Mesh::default();
        mesh.add_colored_rect(
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(4.0)),
            Color32::from_rgba_premultiplied(128, 0, 0, 128),
        );
        let mut pixels = vec![0; 16];
        rasterize(
            &mut pixels,
            4,
            4,
            &mesh,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(3.0, 4.0)),
            1.0,
            &texture,
        );
        for row in pixels.as_chunks::<4>().0 {
            assert_eq!(row, &[0x00800000, 0x00800000, 0x00800000, 0]);
        }
    }
    #[test]
    fn export_visual_fixtures_when_requested() -> Result<(), Box<dyn std::error::Error>> {
        let Ok(directory) = std::env::var("RETYPE_CANDIDATE_FIXTURES") else {
            return Ok(());
        };
        std::fs::create_dir_all(&directory)?;
        let mut surface = Surface::default();
        for (name,texts,scale) in [
            ("short",vec!["我","窝","握","卧","沃","喔","倭","涡"],1.0),
            ("phrases",vec!["我是","我说","我市","我想","我上","我时","我司"],1.0),
            ("mixed",vec!["快捷键","乔布斯","Rust","Visual Studio"],1.5),
            ("long",vec!["这是一个很长的候选词组用于验证完整显示而不是省略号这是一个很长的候选词组用于验证完整显示而不是省略号"],1.0),
        ] {
            let state = state(&texts);
            let widths = surface.measure(&state,(472.0*scale) as i32,scale);
            let image = surface.render(&state,&widths,(2.0*scale) as i32,(480.0*scale) as i32,scale);
            let file = std::fs::File::create(std::path::Path::new(&directory).join(format!("{name}.png")))?;
            let mut encoder = png::Encoder::new(file,image.geometry.width as u32,image.geometry.height as u32);
            encoder.set_color(png::ColorType::Rgb);
            let mut writer = encoder.write_header()?;
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| [(p>>16) as u8,(p>>8) as u8,*p as u8]).collect();
            writer.write_image_data(&bytes)?;
        }
        Ok(())
    }
}
