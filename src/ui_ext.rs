//! 通用小控件：提示条、分段控件、分隔条拖动、带高亮的文本、设置项布局等。

use std::time::{Duration, Instant};

use egui::{Color32, Response, RichText, Sense, Ui, Vec2};

use crate::theme::Theme;

// ---------------------------------------------------------------- 提示条

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: Instant,
}

#[derive(Default)]
pub struct Toasts {
    pub items: Vec<Toast>,
}

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>) {
        let text = text.into();
        // 相同内容只保留最新一条
        self.items.retain(|t| t.text != text);
        self.items.push(Toast {
            text,
            kind,
            until: Instant::now() + Duration::from_secs(if kind == ToastKind::Error { 6 } else { 3 }),
        });
        if self.items.len() > 4 {
            self.items.remove(0);
        }
    }

    pub fn info(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Info, t);
    }
    pub fn ok(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Success, t);
    }
    pub fn warn(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Warn, t);
    }
    pub fn error(&mut self, t: impl Into<String>) {
        self.push(ToastKind::Error, t);
    }

    pub fn show(&mut self, ctx: &egui::Context, theme: &Theme) {
        self.items.retain(|t| t.until > Instant::now());
        if self.items.is_empty() {
            return;
        }
        // 提示条按可用宽度给足空间：太窄会把一句话折成好几行（阅读体验很差）
        let max_w = (ctx.content_rect().width() * 0.62).clamp(320.0, 760.0);
        egui::Area::new(egui::Id::new("easymd-toasts"))
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -46.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for t in &self.items {
                    let (fg, bg) = match t.kind {
                        ToastKind::Info => (theme.strong, theme.subtle),
                        ToastKind::Success => (theme.strong, theme.selected),
                        ToastKind::Warn => (theme.strong, theme.hover),
                        ToastKind::Error => (Color32::WHITE, theme.danger),
                    };
                    egui::Frame::new()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, theme.border))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(14, 8))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 3],
                            blur: 12,
                            spread: 0,
                            color: Color32::from_black_alpha(60),
                        })
                        .show(ui, |ui| {
                            ui.set_max_width(max_w);
                            ui.label(RichText::new(&t.text).color(fg));
                        });
                    ui.add_space(4.0);
                }
            });
    }
}

// ---------------------------------------------------------------- 分段控件

/// 分段按钮组，返回被点击的新值。
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: T,
    options: &[(T, &str)],
    theme: &Theme,
) -> Option<T> {
    let mut result = None;
    egui::Frame::new()
        .fill(theme.subtle)
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (val, label) in options {
                    let active = *val == current;
                    let text = RichText::new(*label).color(if active {
                        theme.strong
                    } else {
                        theme.weak
                    });
                    let btn = egui::Button::new(text)
                        .fill(if active { theme.bg } else { Color32::TRANSPARENT })
                        .stroke(if active {
                            egui::Stroke::new(1.0, theme.border)
                        } else {
                            egui::Stroke::NONE
                        })
                        .corner_radius(egui::CornerRadius::same(5))
                        .min_size(Vec2::new(48.0, 22.0));
                    if ui.add(btn).clicked() && !active {
                        result = Some(*val);
                    }
                }
            });
        });
    result
}

// ---------------------------------------------------------------- 图标 / 小按钮

pub fn tool_button(ui: &mut Ui, glyph: &str, tooltip: &str, theme: &Theme) -> Response {
    let resp = ui.add(
        egui::Button::new(RichText::new(glyph).color(theme.text))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE)
            .corner_radius(egui::CornerRadius::same(5))
            .min_size(Vec2::new(24.0, 22.0)),
    );
    resp.on_hover_text(tooltip)
}


// ---------------------------------------------------------------- 可拖动分隔条

/// 竖直分隔条（用于分栏视图），拖动时修改 `ratio`。返回是否正在拖动。
pub fn drag_splitter(
    ui: &mut Ui,
    id: egui::Id,
    available: f32,
    ratio: &mut f32,
    min_ratio: f32,
    theme: &Theme,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(8.0, ui.available_height()), Sense::drag());
    let dragging = resp.dragged() || resp.hovered();
    let bar_color = if resp.dragged() {
        theme.accent
    } else if resp.hovered() {
        theme.accent.gamma_multiply(0.5)
    } else {
        theme.border
    };
    let x = rect.center().x;
    let h = if resp.dragged() {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x - 1.5, rect.top()),
                egui::pos2(x + 1.5, rect.bottom()),
            ),
            1.0,
            bar_color,
        );
        0.0
    } else {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x - 0.5, rect.top() + 8.0),
                egui::pos2(x + 0.5, rect.bottom() - 8.0),
            ),
            0.0,
            bar_color,
        );
        0.0
    };
    let _ = h;
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let _ = id;
            let new_ratio = ((pos.x - rect.left() + rect.width() * 0.5) / available).clamp(min_ratio, 1.0 - min_ratio);
            *ratio = new_ratio;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        return true;
    }
    if dragging {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    false
}

// ---------------------------------------------------------------- 带高亮的文本

/// 生成一段可高亮指定字符范围的 LayoutJob。
pub fn highlight_job(
    text: &str,
    ranges: &[(usize, usize)],
    base: egui::FontId,
    color: Color32,
    highlight_bg: Color32,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    job.wrap.break_anywhere = false;
    let chars: Vec<char> = text.chars().collect();
    let mut idx = 0usize;
    while idx < chars.len() {
        let in_range = ranges.iter().find(|(s, e)| idx >= *s && idx < *e);
        let (end, is_hit) = match in_range {
            Some((_, e)) => (*e, true),
            None => {
                let next = ranges
                    .iter()
                    .map(|(s, _)| *s)
                    .filter(|s| *s > idx)
                    .min()
                    .unwrap_or(chars.len());
                (next, false)
            }
        };
        let end = end.min(chars.len()).max(idx + 1);
        let seg: String = chars[idx..end].iter().collect();
        job.append(
            &seg,
            0.0,
            TextFormat {
                font_id: base.clone(),
                color,
                background: if is_hit { highlight_bg } else { Color32::TRANSPARENT },
                ..Default::default()
            },
        );
        idx = end;
    }
    job
}

// ---------------------------------------------------------------- 设置项

/// 设置页的分组标题。
pub fn section_title(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).strong().color(theme.accent));
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(4.0);
}

/// 一行设置：左侧说明 + 右侧控件。
pub fn setting_row<R>(ui: &mut Ui, label: &str, hint: Option<&str>, add: impl FnOnce(&mut Ui) -> R) {
    ui.horizontal(|ui| {
        let w = 128.0;
        ui.allocate_ui_with_layout(
            Vec2::new(w, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        let r = add(ui);
        if let Some(hint) = hint {
            ui.label(RichText::new(hint).weak().small());
        }
        r
    });
}

/// 主题色小圆点（点击选中）。
pub fn color_swatch(ui: &mut Ui, color: Color32, selected: bool, theme: &Theme) -> Response {
    let size = Vec2::new(22.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().circle_filled(rect.center(), 9.0, color);
    if selected {
        ui.painter().circle_stroke(
            rect.center(),
            11.0,
            egui::Stroke::new(2.0, theme.accent),
        );
    } else if resp.hovered() {
        ui.painter().circle_stroke(
            rect.center(),
            11.0,
            egui::Stroke::new(1.0, theme.border),
        );
    }
    resp
}

/// 简易进度条（用于全文检索进度）。
pub fn thin_progress(ui: &mut Ui, fraction: f32, theme: &Theme) {
    let h = 3.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(rect, 1.5, theme.subtle);
    let w = rect.width() * fraction.clamp(0.0, 1.0);
    if w > 0.0 {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, Vec2::new(w, h)),
            1.5,
            theme.accent,
        );
    }
}


// ---------------------------------------------------------------- 紧凑布局的工具条
//
// 右上角悬浮工具条用它来排版。
//
// 为什么不用 `ui.horizontal` 直接排：egui 的水平布局在这一排里给每个项计算的
// 交叉轴可用高度会随项数变化，导致各项顶边逐项下移（实测 48px → 50px → 51px，
// 高度都一样却不在一条线上）。这里改为自己算好每个项的矩形，用 `ui.put` 精确摆放，
// 位置完全确定，不受布局交叉轴行为影响。

/// 工具条内边距与项间距。
const TOOLBAR_PAD: Vec2 = Vec2::new(8.0, 2.0);
const TOOLBAR_GAP: f32 = 2.0;

/// 给定字号下，按钮内容应有的精确高度（行高 + 上下内边距）。
pub fn toolbar_item_height(ui: &Ui, font_size: f32) -> f32 {
    let row = ui.fonts_mut(|f| f.row_height(&egui::FontId::proportional(font_size)));
    (row + TOOLBAR_PAD.y * 2.0).round()
}

/// 工具条布局器：一行内所有项共用同一高度与同一顶边。
pub struct Toolbar<'a> {
    theme: &'a Theme,
    font_size: f32,
    height: f32,
    origin: egui::Pos2,
    used: f32,
}

impl<'a> Toolbar<'a> {
    pub fn begin(ui: &mut Ui, theme: &'a Theme, font_size: f32) -> Self {
        ui.spacing_mut().button_padding = TOOLBAR_PAD;
        let height = toolbar_item_height(ui, font_size);
        Self {
            theme,
            font_size,
            height,
            origin: ui.cursor().min,
            used: 0.0,
        }
    }

    fn take_rect(&mut self, width: f32) -> egui::Rect {
        let rect = egui::Rect::from_min_size(
            egui::pos2(self.origin.x + self.used, self.origin.y),
            egui::vec2(width, self.height),
        );
        self.used += width + TOOLBAR_GAP;
        rect
    }

    fn button(&self, text: RichText, active: bool) -> egui::Button<'a> {
        egui::Button::new(text)
            .fill(if active {
                self.theme.selected
            } else {
                Color32::TRANSPARENT
            })
            .stroke(if active {
                egui::Stroke::new(1.0, self.theme.accent.gamma_multiply(0.5))
            } else {
                egui::Stroke::NONE
            })
            .corner_radius(egui::CornerRadius::same(4))
    }

    /// 一段文字在该字号下的实际宽度（用字体度量，避免估宽导致折行）。
    fn label_width(&self, ui: &Ui, text: &str) -> f32 {
        let font = egui::FontId::proportional(self.font_size);
        let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, Color32::WHITE));
        (galley.size().x + TOOLBAR_PAD.x * 2.0 + 4.0).ceil()
    }

    /// 单个文字按钮；宽度按文字实测，返回是否被点击。
    pub fn button_clicked(
        &mut self,
        ui: &mut Ui,
        label: &str,
        tooltip: &str,
        active: bool,
    ) -> bool {
        let width = self.label_width(ui, label);
        let rect = self.take_rect(width);
        let resp = ui.put(
            rect,
            self.button(
                RichText::new(label)
                    .size(self.font_size)
                    .color(if active {
                        self.theme.strong
                    } else {
                        self.theme.text
                    }),
                active,
            ),
        );
        resp.on_hover_text(tooltip).clicked()
    }

    /// 分段控件（例如 阅读 / 编辑）；返回新选中的值。
    pub fn segments<T: Copy + PartialEq>(
        &mut self,
        ui: &mut Ui,
        current: T,
        options: &[(T, &str)],
    ) -> Option<T> {
        let each_width = options
            .iter()
            .map(|(_, label)| self.label_width(ui, label))
            .fold(0.0f32, f32::max);
        let total = each_width * options.len() as f32 + 4.0;
        let group = self.take_rect(total);
        // 底板
        ui.painter().rect_filled(
            group,
            egui::CornerRadius::same(5),
            self.theme.subtle,
        );
        let mut chosen = None;
        for (i, (val, label)) in options.iter().enumerate() {
            let active = *val == current;
            // 纵向**不内缩**：内缩会让按钮比内容矮，egui 会把文字从框顶开始排，
            // 结果这两个按钮看起来比其它按钮低一截（实测低 2px）。
            let rect = egui::Rect::from_min_size(
                egui::pos2(group.left() + 2.0 + i as f32 * each_width, group.top()),
                egui::vec2(each_width, self.height),
            );
            let resp = ui.put(
                rect,
                self.button(
                    RichText::new(*label).size(self.font_size).color(if active {
                        self.theme.strong
                    } else {
                        self.theme.weak
                    }),
                    active,
                ),
            );
            if resp.clicked() && !active {
                chosen = Some(*val);
            }
        }
        chosen
    }

    /// 结束布局：把占用的矩形交给父布局，避免遮挡其它内容。
    pub fn end(self, ui: &mut Ui) {
        ui.advance_cursor_after_rect(egui::Rect::from_min_size(
            self.origin,
            egui::vec2(self.used, self.height),
        ));
    }
}
