//! 右侧编辑面板：等宽编辑、格式化插入、光标定位、Tab 缩进。

use egui::{FontId, RichText, Ui};

use crate::config::AppConfig;
use crate::doc::Document;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    H1,
    H2,
    H3,
    Bold,
    Italic,
    Strike,
    InlineCode,
    CodeBlock,
    Quote,
    Bullet,
    Numbered,
    Task,
    Link,
    Image,
    Table,
    Hr,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::H1 => "H1",
            Self::H2 => "H2",
            Self::H3 => "H3",
            Self::Bold => "粗体",
            Self::Italic => "斜体",
            Self::Strike => "删除线",
            Self::InlineCode => "行内代码",
            Self::CodeBlock => "代码块",
            Self::Quote => "引用",
            Self::Bullet => "无序列表",
            Self::Numbered => "有序列表",
            Self::Task => "任务",
            Self::Link => "链接",
            Self::Image => "图片",
            Self::Table => "表格",
            Self::Hr => "分隔线",
        }
    }
    pub fn tooltip(self) -> &'static str {
        match self {
            Self::H1 => "一级标题",
            Self::H2 => "二级标题",
            Self::H3 => "三级标题",
            Self::Bold => "粗体 (**文字**)",
            Self::Italic => "斜体 (*文字*)",
            Self::Strike => "删除线 (~~文字~~)",
            Self::InlineCode => "行内代码 (`代码`)",
            Self::CodeBlock => "代码块 (```)",
            Self::Quote => "引用 (>)",
            Self::Bullet => "无序列表 (- )",
            Self::Numbered => "有序列表 (1. )",
            Self::Task => "任务列表 (- [ ] )",
            Self::Link => "链接 [文字](地址)",
            Self::Image => "图片 ![说明](路径)",
            Self::Table => "插入表格",
            Self::Hr => "分隔线 (---)",
        }
    }

    /// 在光标处应用格式，返回新的光标/选区（字符索引）。
    pub fn apply(self, text: &mut String, cursor: usize, tab: usize) -> Option<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        let cur = cursor.min(chars.len());
        let line_start = {
            let mut i = cur;
            while i > 0 && chars[i - 1] != '\n' {
                i -= 1;
            }
            i
        };
        let line_end = {
            let mut i = cur;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            i
        };

        let replace = |text: &mut String, start: usize, end: usize, ins: &str| {
            let before: String = chars[..start].iter().collect();
            let after: String = chars[end..].iter().collect();
            *text = format!("{before}{ins}{after}");
        };

        match self {
            Self::H1 | Self::H2 | Self::H3 => {
                let prefix = match self {
                    Self::H1 => "# ",
                    Self::H2 => "## ",
                    Self::H3 => "### ",
                    _ => unreachable!(),
                };
                // 去掉已有标题标记
                let existing: String = chars[line_start..line_end].iter().collect();
                let stripped = existing.trim_start_matches('#').trim_start();
                let leading_hashes = existing.len() - existing.trim_start_matches('#').len();
                let start = if leading_hashes > 0 {
                    line_start
                } else {
                    line_start
                };
                let body: String = stripped.to_string();
                replace(text, start, line_end, &format!("{prefix}{body}"));
                let new_cur = start + prefix.chars().count() + (cur - line_start - leading_hashes.min(cur - line_start));
                Some((new_cur, new_cur))
            }
            Self::Quote => {
                let existing: String = chars[line_start..line_end].iter().collect();
                if existing.trim_start().starts_with('>') {
                    let stripped = existing.trim_start().trim_start_matches('>').trim_start();
                    replace(text, line_start, line_end, stripped);
                    Some((cur.saturating_sub(2), cur.saturating_sub(2)))
                } else {
                    replace(text, line_start, line_end, &format!("> {existing}"));
                    Some((cur + 2, cur + 2))
                }
            }
            Self::Bullet | Self::Numbered | Self::Task => {
                let mark = match self {
                    Self::Bullet => "- ".to_string(),
                    Self::Numbered => "1. ".to_string(),
                    _ => "- [ ] ".to_string(),
                };
                replace(text, line_start, line_end, &format!("{mark}{}", chars[line_start..line_end].iter().collect::<String>()));
                let add = mark.chars().count();
                Some((cur + add, cur + add))
            }
            Self::Bold | Self::Italic | Self::Strike | Self::InlineCode => {
                let (open, close) = match self {
                    Self::Bold => ("**", "**"),
                    Self::Italic => ("*", "*"),
                    Self::Strike => ("~~", "~~"),
                    _ => ("`", "`"),
                };
                replace(text, cur, cur, &format!("{open}{close}"));
                Some((cur + open.chars().count(), cur + open.chars().count()))
            }
            Self::CodeBlock => {
                let ins = "\n```\n\n```\n";
                replace(text, cur, cur, ins);
                Some((cur + 5, cur + 5))
            }
            Self::Link => {
                let ins = "[文字](地址)";
                replace(text, cur, cur, ins);
                Some((cur + 1, cur + 1))
            }
            Self::Image => {
                let ins = "![说明](路径)";
                replace(text, cur, cur, ins);
                Some((cur + 2, cur + 2))
            }
            Self::Table => {
                let ins = "\n| 列 1 | 列 2 | 列 3 |\n| --- | --- | --- |\n| 内容 | 内容 | 内容 |\n\n";
                replace(text, cur, cur, ins);
                Some((cur + 3, cur + 3))
            }
            Self::Hr => {
                let ins = "\n\n---\n\n";
                replace(text, cur, cur, ins);
                Some((cur + 5, cur + 5))
            }
        }
        .map(|(a, b)| {
            let _ = tab;
            (a.min(text.chars().count()), b.min(text.chars().count()))
        })
    }
}

pub struct EditorOutput {
    pub changed: bool,
    /// 当前光标位置（字符索引）
    pub cursor_char: usize,
    /// 是否请求保存
    pub save_requested: bool,
    /// 与预览联动所需的信息
    pub view: EditorView,
}

/// 编辑器的可视状态：把「字符索引 ↔ 纵向位置」的换算交给上层做联动。
#[derive(Default)]
pub struct EditorView {
    /// 视口顶部对应的字符索引
    pub top_char: usize,
    /// 当前滚动偏移
    pub scroll_offset: f32,
    /// 滚动内容的原点（屏幕坐标），用于把 galley 坐标换算成滚动偏移
    pub content_origin_y: f32,
    /// 本帧排版结果与位置
    pub galley: Option<std::sync::Arc<egui::Galley>>,
    pub galley_pos: egui::Pos2,
    pub has_focus: bool,
}

impl EditorView {
    /// 把字符索引换算成「滚动内容内」的纵坐标（未减去行高）。
    pub fn y_of_char(&self, char_index: usize) -> Option<f32> {
        let galley = self.galley.as_ref()?;
        let rect = galley.pos_from_cursor(egui::text::CCursor::new(
            egui::text::CharIndex(char_index),
        ));
        Some(self.galley_pos.y + rect.top() - self.content_origin_y)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    doc: &mut Document,
    cfg: &AppConfig,
    theme: &Theme,
    restore_cursor: Option<usize>,
    compact: bool,
    top_inset: f32,
    // 需要滚动到的内容相对纵坐标（联动时由预览侧给出）；只发动画请求，不直接改偏移
    scroll_to_y: Option<f32>,
) -> EditorOutput {
    let mut out = EditorOutput {
        changed: false,
        cursor_char: 0,
        save_requested: false,
        view: EditorView::default(),
    };

    let id = egui::Id::new(("easmd-editor", &doc.path));

    // 待定位光标（来自全文检索 / 大纲 / 预览点击）
    // 注意：编辑器首次显示时还没有已保存的状态，这里要用默认状态兜底，
    // 否则定位请求会被静默丢弃。
    if let Some(idx) = restore_cursor {
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(egui::text::CharIndex(idx)),
        )));
        state.store(ui.ctx(), id);
        // 聚焦后 TextEdit 会自己把光标滚入视野（搜索命中跳转、预览点击定位都要靠它）
        ui.memory_mut(|m| m.request_focus(id));
    }

    // 顶部留白：避让右上角悬浮按钮，避免工具栏被压住
    if !compact {
        ui.add_space(top_inset);
        editor_toolbar(ui, doc, cfg, theme, &mut out);
        ui.add_space(4.0);
    }

    let cursor_char = egui::TextEdit::load_state(ui.ctx(), id)
        .and_then(|s| s.cursor.char_range())
        .map(|r| r.primary.index.0)
        .unwrap_or(0);
    out.cursor_char = cursor_char;

    // Tab 缩进
    let has_focus = ui.memory(|m| m.has_focus(id));
    if has_focus && ui.input(|i| i.key_pressed(egui::Key::Tab)) {
        let spaces = " ".repeat(cfg.tab_spaces);
        let mut chars: Vec<char> = doc.text.chars().collect();
        let at = cursor_char.min(chars.len());
        for (i, c) in spaces.chars().enumerate() {
            chars.insert(at + i, c);
        }
        doc.text = chars.into_iter().collect();
        doc.mark_edited();
        out.changed = true;
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
            let new = at + spaces.chars().count();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(egui::text::CharIndex(new)),
            )));
            state.store(ui.ctx(), id);
        }
        out.cursor_char = at + spaces.chars().count();
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    }

    let font = FontId::monospace(cfg.font_size_editor);

    // 是否让编辑区随内容增长（在外部滚动容器里）
    let line_count = doc.text.lines().count().max(1);

    let scroll = egui::ScrollArea::vertical()
        .id_salt(("easmd-editor-scroll", &doc.path))
        .auto_shrink([false, false]);

    let mut galley: Option<std::sync::Arc<egui::Galley>> = None;
    let mut galley_pos = egui::Pos2::ZERO;

    let scroll_out = scroll.show(ui, |ui| {
        // 联动：请求滚动到预览给出的位置（动画执行，避免与 egui 的滚动动画互相拉扯）
        if let Some(y) = scroll_to_y {
            let top = ui.next_widget_position().y + y.max(0.0);
            ui.scroll_to_rect(
                egui::Rect::from_min_size(
                    egui::pos2(ui.max_rect().left(), top),
                    egui::vec2(1.0, 1.0),
                ),
                Some(egui::Align::TOP),
            );
        }
        let text_color = if doc.binary { theme.weak } else { theme.text };
        let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = if cfg.soft_wrap { wrap } else { f32::INFINITY };
            job.append(
                buf.as_str(),
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: text_color,
                    ..Default::default()
                },
            );
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let te = egui::TextEdit::multiline(&mut doc.text)
            .id(id)
            .code_editor()
            .desired_width(if cfg.soft_wrap {
                ui.available_width()
            } else {
                f32::INFINITY
            })
            .desired_rows(line_count.max(12).min(400))
            // 首次聚焦时把光标放在开头（默认行为是放到文末并自动滚过去）
            .cursor_at_end(false)
            .lock_focus(true)
            .layouter(&mut layouter)
            .margin(egui::Margin::symmetric(12, 10));
        let resp = te.show(ui);
        galley = Some(resp.galley.clone());
        galley_pos = resp.galley_pos;
        if resp.response.changed() {
            out.changed = true;
            doc.mark_edited();
        }
        out.cursor_char = resp
            .cursor_range
            .map(|r| r.primary.index.0)
            .unwrap_or(out.cursor_char);
        resp.response.widget_info(|| {
            egui::WidgetInfo::text_edit(true, doc.text.as_str(), doc.name(), "编辑 Markdown 内容")
        });
    });

    // ---- 联动信息 ----
    out.view.scroll_offset = scroll_out.state.offset.y;
    // 内容原点：视口顶部减去滚动偏移，galley 坐标可直接换算成滚动内容坐标
    out.view.content_origin_y = scroll_out.inner_rect.top() - scroll_out.state.offset.y;
    out.view.galley_pos = galley_pos;
    out.view.has_focus = has_focus;
    if let Some(g) = &galley {
        // 视口顶部对应的字符（用于「编辑器滚动 → 预览跟随」）
        out.view.top_char = g
            .cursor_from_pos(egui::vec2(0.0, scroll_out.inner_rect.top() - galley_pos.y))
            .index
            .0;
    }
    out.view.galley = galley;

    out
}

fn editor_toolbar(
    ui: &mut Ui,
    doc: &mut Document,
    cfg: &AppConfig,
    theme: &Theme,
    out: &mut EditorOutput,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        let id = egui::Id::new(("easmd-editor", &doc.path));
        let cursor = egui::TextEdit::load_state(ui.ctx(), id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index.0)
            .unwrap_or(0);

        for f in [
            Format::H1,
            Format::H2,
            Format::H3,
            Format::Bold,
            Format::Italic,
            Format::Strike,
            Format::InlineCode,
            Format::CodeBlock,
            Format::Quote,
            Format::Bullet,
            Format::Numbered,
            Format::Task,
            Format::Link,
            Format::Image,
            Format::Table,
            Format::Hr,
        ] {
            let label = RichText::new(f.label()).size(cfg.font_size * 0.82);
            if ui
                .add(
                    egui::Button::new(label)
                        .fill(theme.subtle)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(egui::CornerRadius::same(5))
                        .min_size(egui::Vec2::new(0.0, 22.0)),
                )
                .on_hover_text(f.tooltip())
                .clicked()
            {
                if let Some((a, b)) = f.apply(&mut doc.text, cursor, cfg.tab_spaces) {
                    doc.mark_edited();
                    out.changed = true;
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(egui::text::CharIndex(a)),
                        )));
                        state.store(ui.ctx(), id);
                    }
                    ui.memory_mut(|m| m.request_focus(id));
                    let _ = b;
                }
            }
        }
        // 保存按钮 + 统计信息（跟随换行，不会与其它按钮重叠）
        ui.add_space(6.0);
        let dirty = doc.is_dirty();
        let text = if dirty { "保存 ●" } else { "已保存" };
        let btn = egui::Button::new(RichText::new(text).color(theme.text))
            .fill(if dirty { theme.selected } else { theme.subtle })
            .stroke(egui::Stroke::NONE)
            .corner_radius(egui::CornerRadius::same(5))
            .min_size(egui::Vec2::new(0.0, 22.0));
        if ui.add(btn).on_hover_text("保存 (Ctrl+S)").clicked() {
            out.save_requested = true;
        }
        let stats = crate::util::text_stats(&doc.text);
        ui.label(
            RichText::new(format!(
                "{} 字 · {} 行 · {}",
                stats.chars,
                stats.lines,
                crate::util::reading_time_text(&stats)
            ))
            .weak()
            .small(),
        );
    });
}
