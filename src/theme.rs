//! 主题系统：内置多套阅读主题，支持自定义背景色 / 文字色 / 强调色与背景图片。

use egui::{Color32, CornerRadius, Stroke};

use crate::config::{AppConfig, ImageFit};
use crate::util::hex_to_color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeId {
    Paper,
    White,
    Sepia,
    Green,
    Graphite,
    Midnight,
    WarmDark,
    Nord,
}

impl Default for ThemeId {
    fn default() -> Self {
        Self::Paper
    }
}

impl ThemeId {
    pub fn all() -> [Self; 8] {
        [
            Self::Paper,
            Self::White,
            Self::Sepia,
            Self::Green,
            Self::Graphite,
            Self::Midnight,
            Self::WarmDark,
            Self::Nord,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Paper => "纸白",
            Self::White => "纯白",
            Self::Sepia => "羊皮纸",
            Self::Green => "护眼绿",
            Self::Graphite => "石墨",
            Self::Midnight => "午夜蓝",
            Self::WarmDark => "暖夜",
            Self::Nord => "极地",
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Graphite | Self::Midnight | Self::WarmDark | Self::Nord)
    }

    pub fn next_light_dark(self) -> Self {
        // 用于工具栏上的快速明暗切换
        match self {
            Self::Paper | Self::White | Self::Sepia | Self::Green => Self::Graphite,
            _ => Self::Paper,
        }
    }
}

/// 一套完整配色。
#[derive(Debug, Clone)]
pub struct Theme {
    pub dark: bool,
    /// 阅读区背景
    pub bg: Color32,
    /// 侧栏 / 顶栏 / 状态栏背景
    pub panel: Color32,
    /// 次级填充（输入框、悬浮按钮、列表 hover 之外的底）
    pub subtle: Color32,
    /// hover 高亮
    pub hover: Color32,
    /// 选中项背景
    pub selected: Color32,
    pub border: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub link: Color32,
    /// 代码块背景
    pub code_bg: Color32,
    /// 行内代码背景
    pub inline_code_bg: Color32,
    pub selection: Color32,
    pub danger: Color32,
}

impl Theme {
    pub fn from_id(id: ThemeId) -> Self {
        let f = |r: u8, g: u8, b: u8| Color32::from_rgb(r, g, b);
        match id {
            ThemeId::Paper => Self {
                dark: false,
                bg: f(0xFA, 0xF8, 0xF3),
                panel: f(0xF2, 0xEF, 0xE8),
                subtle: f(0xE9, 0xE4, 0xDA),
                hover: f(0xE3, 0xDC, 0xCE),
                selected: f(0xDE, 0xD3, 0xBD),
                border: f(0xE0, 0xDA, 0xCD),
                text: f(0x32, 0x30, 0x2C),
                strong: f(0x18, 0x17, 0x14),
                weak: f(0x8A, 0x85, 0x78),
                accent: f(0xA8, 0x66, 0x3A),
                link: f(0xA8, 0x5C, 0x22),
                code_bg: f(0xF3, 0xEE, 0xE4),
                inline_code_bg: f(0xEC, 0xE4, 0xD5),
                selection: f(0xDC, 0xCB, 0xA6),
                danger: f(0xB4, 0x3B, 0x2E),
            },
            ThemeId::White => Self {
                dark: false,
                bg: f(0xFF, 0xFF, 0xFF),
                panel: f(0xF6, 0xF7, 0xF9),
                subtle: f(0xED, 0xEF, 0xF2),
                hover: f(0xE7, 0xEA, 0xEF),
                selected: f(0xDD, 0xE6, 0xF5),
                border: f(0xD8, 0xDC, 0xE1),
                text: f(0x24, 0x29, 0x2F),
                strong: f(0x08, 0x0A, 0x0D),
                weak: f(0x6E, 0x77, 0x81),
                accent: f(0x09, 0x69, 0xDA),
                link: f(0x09, 0x69, 0xDA),
                code_bg: f(0xF6, 0xF8, 0xFA),
                inline_code_bg: f(0xEF, 0xF1, 0xF4),
                selection: f(0xC6, 0xDF, 0xFF),
                danger: f(0xCF, 0x22, 0x2E),
            },
            ThemeId::Sepia => Self {
                dark: false,
                bg: f(0xF7, 0xEC, 0xD5),
                panel: f(0xEF, 0xE1, 0xC4),
                subtle: f(0xE6, 0xD5, 0xB4),
                hover: f(0xE0, 0xCD, 0xA7),
                selected: f(0xDB, 0xC4, 0x96),
                border: f(0xD9, 0xC4, 0x9C),
                text: f(0x4B, 0x3B, 0x2A),
                strong: f(0x2A, 0x1E, 0x12),
                weak: f(0x8C, 0x75, 0x5A),
                accent: f(0x9A, 0x6B, 0x33),
                link: f(0x96, 0x5E, 0x22),
                code_bg: f(0xEE, 0xE1, 0xC8),
                inline_code_bg: f(0xE7, 0xD8, 0xBA),
                selection: f(0xE0, 0xC7, 0x9B),
                danger: f(0xA8, 0x35, 0x28),
            },
            ThemeId::Green => Self {
                dark: false,
                bg: f(0xE7, 0xF0, 0xE4),
                panel: f(0xDD, 0xE9, 0xD9),
                subtle: f(0xD2, 0xE2, 0xCC),
                hover: f(0xCA, 0xDD, 0xC3),
                selected: f(0xBF, 0xD8, 0xB7),
                border: f(0xC1, 0xD5, 0xBA),
                text: f(0x26, 0x36, 0x2A),
                strong: f(0x10, 0x1D, 0x13),
                weak: f(0x63, 0x76, 0x5E),
                accent: f(0x3F, 0x7A, 0x4A),
                link: f(0x2C, 0x6B, 0x39),
                code_bg: f(0xDC, 0xE8, 0xD6),
                inline_code_bg: f(0xD4, 0xE3, 0xCE),
                selection: f(0xC3, 0xDD, 0xBB),
                danger: f(0xA5, 0x33, 0x2A),
            },
            ThemeId::Graphite => Self {
                dark: true,
                bg: f(0x1B, 0x1D, 0x1F),
                panel: f(0x23, 0x26, 0x2A),
                subtle: f(0x2B, 0x2F, 0x34),
                hover: f(0x33, 0x38, 0x3E),
                selected: f(0x2F, 0x45, 0x63),
                border: f(0x3A, 0x3F, 0x45),
                text: f(0xD2, 0xD6, 0xDA),
                strong: f(0xF7, 0xF9, 0xFB),
                weak: f(0x8B, 0x92, 0x98),
                accent: f(0x6E, 0xA8, 0xFE),
                link: f(0x7C, 0xB4, 0xFF),
                code_bg: f(0x21, 0x25, 0x2A),
                inline_code_bg: f(0x2C, 0x32, 0x38),
                selection: f(0x30, 0x4E, 0x78),
                danger: f(0xF0, 0x6A, 0x6A),
            },
            ThemeId::Midnight => Self {
                dark: true,
                bg: f(0x12, 0x17, 0x22),
                panel: f(0x18, 0x20, 0x31),
                subtle: f(0x1F, 0x29, 0x3C),
                hover: f(0x26, 0x32, 0x49),
                selected: f(0x25, 0x3B, 0x60),
                border: f(0x2C, 0x3A, 0x52),
                text: f(0xC6, 0xD2, 0xE4),
                strong: f(0xEC, 0xF2, 0xFC),
                weak: f(0x7E, 0x8E, 0xA6),
                accent: f(0x7A, 0xA2, 0xF7),
                link: f(0x8A, 0xB4, 0xFF),
                code_bg: f(0x15, 0x1C, 0x2A),
                inline_code_bg: f(0x1F, 0x2A, 0x3E),
                selection: f(0x2A, 0x40, 0x68),
                danger: f(0xE5, 0x6B, 0x74),
            },
            ThemeId::WarmDark => Self {
                dark: true,
                bg: f(0x1E, 0x1A, 0x17),
                panel: f(0x26, 0x21, 0x1C),
                subtle: f(0x2F, 0x29, 0x21),
                hover: f(0x39, 0x31, 0x27),
                selected: f(0x4A, 0x3A, 0x25),
                border: f(0x40, 0x38, 0x2E),
                text: f(0xDE, 0xD5, 0xC9),
                strong: f(0xF8, 0xF2, 0xEA),
                weak: f(0x94, 0x89, 0x7B),
                accent: f(0xD9, 0xA0, 0x5B),
                link: f(0xE2, 0xAC, 0x6C),
                code_bg: f(0x22, 0x1D, 0x19),
                inline_code_bg: f(0x2E, 0x27, 0x21),
                selection: f(0x4E, 0x3D, 0x26),
                danger: f(0xE8, 0x7A, 0x6E),
            },
            ThemeId::Nord => Self {
                dark: true,
                bg: f(0x2E, 0x34, 0x40),
                panel: f(0x34, 0x3B, 0x48),
                subtle: f(0x3B, 0x42, 0x52),
                hover: f(0x43, 0x4C, 0x5E),
                selected: f(0x44, 0x5A, 0x74),
                border: f(0x4C, 0x56, 0x6A),
                text: f(0xD8, 0xDE, 0xE9),
                strong: f(0xEC, 0xEF, 0xF4),
                weak: f(0x98, 0xA2, 0xB3),
                accent: f(0x88, 0xC0, 0xD0),
                link: f(0x8F, 0xBC, 0xBB),
                code_bg: f(0x2B, 0x31, 0x3C),
                inline_code_bg: f(0x3B, 0x42, 0x52),
                selection: f(0x43, 0x4C, 0x5E),
                danger: f(0xBF, 0x61, 0x6A),
            },
        }
    }

    /// 叠加用户在设置里的自定义颜色。
    pub fn apply_overrides(mut self, cfg: &AppConfig) -> Self {
        if let Some(c) = cfg.custom_bg.as_deref().and_then(hex_to_color) {
            self.bg = c;
            // 面板色根据背景自动推导，保证整体协调
            self.panel = if cfg.custom_bg.is_some() {
                c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.035)
            } else {
                self.panel
            };
            self.subtle = c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.07);
            self.hover = c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.12);
            self.code_bg = c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.05);
            self.inline_code_bg = c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.09);
            self.border = c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.20);
            self.dark = luminance(c) < 0.5;
        }
        if let Some(c) = cfg.custom_text.as_deref().and_then(hex_to_color) {
            self.text = c;
            self.strong = c.lerp_to_gamma(if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.35);
            self.weak = c.lerp_to_gamma(self.bg, 0.40);
        }
        if let Some(c) = cfg.custom_accent.as_deref().and_then(hex_to_color) {
            self.accent = c;
            self.link = c;
        }
        self
    }

    /// 写入 egui 样式。
    pub fn apply(&self, style: &mut egui::Style, cfg: &AppConfig) {
        let v = &mut style.visuals;
        v.dark_mode = self.dark;
        v.override_text_color = None;
        v.weak_text_color = Some(self.weak);
        v.weak_text_alpha = 0.65;

        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.noninteractive.bg_fill = self.panel;
        v.widgets.noninteractive.weak_bg_fill = self.panel;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        v.widgets.noninteractive.corner_radius = CornerRadius::same(4);
        v.widgets.noninteractive.expansion = 0.0;

        v.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.inactive.bg_fill = self.subtle;
        v.widgets.inactive.weak_bg_fill = self.subtle;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        v.widgets.inactive.corner_radius = CornerRadius::same(5);

        v.widgets.hovered.fg_stroke = Stroke::new(1.0, self.strong);
        v.widgets.hovered.bg_fill = self.hover;
        v.widgets.hovered.weak_bg_fill = self.hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent.gamma_multiply(0.6));
        v.widgets.hovered.corner_radius = CornerRadius::same(5);
        v.widgets.hovered.expansion = 0.0;

        v.widgets.active.fg_stroke = Stroke::new(1.0, self.strong);
        v.widgets.active.bg_fill = self.selected;
        v.widgets.active.weak_bg_fill = self.selected;
        v.widgets.active.bg_stroke = Stroke::new(1.0, self.accent);
        v.widgets.active.corner_radius = CornerRadius::same(5);

        v.widgets.open.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.open.bg_fill = self.selected;
        v.widgets.open.weak_bg_fill = self.subtle;
        v.widgets.open.bg_stroke = Stroke::new(1.0, self.border);

        v.selection.bg_fill = self.selection;
        v.selection.stroke = Stroke::new(1.0, self.strong);
        v.hyperlink_color = self.link;
        v.faint_bg_color = self.subtle;
        v.extreme_bg_color = self.code_bg;
        v.text_edit_bg_color = Some(self.bg);
        v.code_bg_color = self.inline_code_bg;
        v.panel_fill = self.panel;
        v.window_fill = self.panel;
        v.window_stroke = Stroke::new(1.0, self.border);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.window_shadow = egui::epaint::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 120 } else { 40 }),
        };
        v.popup_shadow = v.window_shadow;
        v.warn_fg_color = if self.dark {
            Color32::from_rgb(0xE8, 0xC0, 0x6A)
        } else {
            Color32::from_rgb(0x9A, 0x69, 0x00)
        };
        v.error_fg_color = self.danger;
        v.text_cursor = egui::style::TextCursorStyle {
            stroke: Stroke::new(2.0, self.accent),
            preview: false,
            blink: true,
            ..Default::default()
        };
        v.indent_has_left_vline = false;
        v.striped = false;
        v.button_frame = true;
        v.collapsing_header_frame = false;
        v.image_loading_spinners = true;

        // 间距 / 密度
        let d = cfg.ui_density;
        style.spacing.item_spacing = egui::vec2(8.0 * d, 5.0 * d);
        style.spacing.button_padding = egui::vec2(9.0 * d, 4.0 * d);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.window_margin = egui::Margin::same(10);
        style.spacing.menu_spacing = 4.0;
        style.spacing.indent = 16.0;
        style.spacing.interact_size = egui::vec2(26.0, 24.0 * d);
        style.spacing.scroll.bar_width = 10.0;
        style.spacing.scroll.floating = true;
        style.spacing.scroll.foreground_color = true;
        style.spacing.scroll.dormant_background_opacity = 0.0;
        style.spacing.scroll.active_background_opacity = 0.35;
        style.spacing.tooltip_width = 520.0;
        style.url_in_tooltip = false;
    }

    /// 生成文本样式表（字体大小随缩放变化）。
    pub fn text_styles(&self, cfg: &AppConfig) -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
        use egui::{FontFamily, FontId, TextStyle};
        let mut m = std::collections::BTreeMap::new();
        // 字号取整：每种字号都会在字形图集里单独缓存，减少零碎字号可明显降低内存占用
        let s = cfg.font_size.round();
        let mono = cfg.font_size_editor.round();
        m.insert(TextStyle::Small, FontId::new((s * 0.82).round(), FontFamily::Proportional));
        m.insert(TextStyle::Body, FontId::new(s, FontFamily::Proportional));
        // 标题基准：正文 + (Heading-Body)*系数，见 egui_commonmark 的标题缩放规则
        m.insert(TextStyle::Heading, FontId::new((s * 1.66).round(), FontFamily::Proportional));
        m.insert(TextStyle::Monospace, FontId::new(mono, FontFamily::Monospace));
        m.insert(TextStyle::Button, FontId::new((s * 0.95).round(), FontFamily::Proportional));
        m
    }
}

fn luminance(c: Color32) -> f32 {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0
}

/// 背景图片绘制所需的数据。
pub struct BackgroundImage {
    pub texture: Option<egui::TextureHandle>,
    pub loaded_from: Option<std::path::PathBuf>,
    pub error: Option<String>,
}

impl Default for BackgroundImage {
    fn default() -> Self {
        Self {
            texture: None,
            loaded_from: None,
            error: None,
        }
    }
}

impl BackgroundImage {
    /// 载入（或卸载）背景图片。图片会先缩放到不超过窗口大小的 1.5 倍，控制显存与内存占用。
    pub fn update(&mut self, ctx: &egui::Context, path: Option<&std::path::Path>, max_px: u32) {
        match path {
            None => {
                self.texture = None;
                self.loaded_from = None;
                self.error = None;
            }
            Some(p) => {
                if self.loaded_from.as_deref() == Some(p) && self.texture.is_some() {
                    return;
                }
                match load_scaled_image(p, max_px) {
                    Ok(img) => {
                        let tex = ctx.load_texture(
                            "easymd-bg",
                            img,
                            egui::TextureOptions::LINEAR,
                        );
                        self.texture = Some(tex);
                        self.loaded_from = Some(p.to_path_buf());
                        self.error = None;
                    }
                    Err(e) => {
                        self.texture = None;
                        self.loaded_from = Some(p.to_path_buf());
                        self.error = Some(e);
                    }
                }
            }
        }
    }

    pub fn paint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        fit: ImageFit,
        opacity: f32,
        dim: f32,
        dim_color: Color32,
    ) {
        let Some(tex) = &self.texture else { return };
        let sz = tex.size_vec2();
        if sz.x <= 0.0 || sz.y <= 0.0 {
            return;
        }
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
        if alpha == 0 {
            return;
        }
        let tint = Color32::from_white_alpha(alpha);
        let painter = painter.with_clip_rect(rect);
        match fit {
            ImageFit::Cover => {
                // 保持比例铺满，超出部分裁掉
                let scale = (rect.width() / sz.x).max(rect.height() / sz.y);
                let draw = sz * scale;
                let offset = egui::vec2(
                    (draw.x - rect.width()) * 0.5,
                    (draw.y - rect.height()) * 0.5,
                );
                let r = egui::Rect::from_min_size(
                    egui::pos2(rect.left() - offset.x, rect.top() - offset.y),
                    draw,
                );
                painter.image(tex.id(), r, uv_full(), tint);
            }
            ImageFit::Contain => {
                let scale = (rect.width() / sz.x).min(rect.height() / sz.y);
                let draw = sz * scale;
                let r = egui::Rect::from_center_size(rect.center(), draw);
                painter.image(tex.id(), r, uv_full(), tint);
            }
            ImageFit::Center => {
                let r = egui::Rect::from_center_size(rect.center(), sz);
                painter.image(tex.id(), r, uv_full(), tint);
            }
            ImageFit::Tile => {
                let step = sz.max(egui::vec2(16.0, 16.0));
                let cols = (rect.width() / step.x).ceil() as i32 + 1;
                let rows = (rect.height() / step.y).ceil() as i32 + 1;
                // 限制总块数，避免极端情况下绘制量过大
                for iy in 0..rows.min(120) {
                    for ix in 0..cols.min(120) {
                        let min = egui::pos2(
                            rect.left() + ix as f32 * step.x,
                            rect.top() + iy as f32 * step.y,
                        );
                        painter.image(
                            tex.id(),
                            egui::Rect::from_min_size(min, step),
                            uv_full(),
                            tint,
                        );
                    }
                }
            }
        }
        // 蒙版：保证文字始终清晰可读
        if dim > 0.0 {
            let c = Color32::from_rgba_unmultiplied(
                dim_color.r(),
                dim_color.g(),
                dim_color.b(),
                (dim.clamp(0.0, 1.0) * 255.0) as u8,
            );
            painter.rect_filled(rect, 0.0, c);
        }
    }
}

fn uv_full() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// 读取图片并缩放到合理尺寸（控制内存占用）。
pub fn load_scaled_image(path: &std::path::Path, max_px: u32) -> Result<egui::ColorImage, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| format!("无法打开图片: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("无法识别图片格式: {e}"))?;
    let img = reader.decode().map_err(|e| format!("解码失败: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let limit = max_px.max(320);
    let img = if w > limit || h > limit {
        let scale = (limit as f32 / w as f32).min(limit as f32 / h as f32);
        img.resize(
            (w as f32 * scale).round() as u32,
            (h as f32 * scale).round() as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}
