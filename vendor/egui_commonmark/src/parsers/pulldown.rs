use std::iter::Peekable;
use std::ops::Range;

use crate::{CommonMarkCache, CommonMarkOptions};

use egui::{self, Id, Pos2, TextStyle, Ui};

use crate::List;
use egui_commonmark_backend::elements::*;
use egui_commonmark_backend::misc::*;
use egui_commonmark_backend::pulldown::*;
use pulldown_cmark::{CowStr, HeadingLevel};

/// Newline logic is constructed by the following:
/// All elements try to insert a newline before them (if they are allowed)
/// and end their own line.
struct Newline {
    /// Whether a newline should not be inserted before a widget. This is only for
    /// the first widget.
    should_not_start_newline_forced: bool,
    /// Whether an element should insert a newline before it
    should_start_newline: bool,
    /// Whether an element should end it's own line using a newline
    /// This will have to be set to false in cases such as when blocks are within
    /// a list.
    should_end_newline: bool,
    /// only false when the widget is the last one.
    should_end_newline_forced: bool,
}

impl Default for Newline {
    fn default() -> Self {
        Self {
            should_not_start_newline_forced: true,
            should_start_newline: true,
            should_end_newline: true,
            should_end_newline_forced: true,
        }
    }
}

impl Newline {
    pub fn can_insert_end(&self) -> bool {
        self.should_end_newline && self.should_end_newline_forced
    }

    pub fn can_insert_start(&self) -> bool {
        self.should_start_newline && !self.should_not_start_newline_forced
    }

    pub fn try_insert_start(&self, ui: &mut Ui) {
        if self.can_insert_start() {
            newline(ui);
        }
    }

    pub fn try_insert_end(&self, ui: &mut Ui) {
        if self.can_insert_end() {
            newline(ui);
        }
    }
}

#[derive(Default)]
struct DefinitionList {
    is_first_item: bool,
    is_def_list_def: bool,
}

pub struct CommonMarkViewerInternal {
    /// 本地修改（EasyMD）：表格序号（当前已不再用于 Grid id，保留以备扩展）
    #[allow(dead_code)]
    curr_table: usize,
    text_style: Style,
    list: List,
    link: Option<Link>,
    image: Option<Image>,
    line: Newline,
    code_block: Option<CodeBlock>,

    /// Only populated if the html_fn option has been set
    html_block: String,
    is_list_item: bool,
    def_list: DefinitionList,
    is_table: bool,
    is_blockquote: bool,
    checkbox_events: Vec<CheckboxClickEvent>,
    deferred_scroll_to_heading: Option<String>,
    /// 本地新增（EasyMD）：内容原点（屏幕坐标），用于把绘制矩形换算成内容相对坐标
    diag_content_top: f32,
    /// 本地新增（EasyMD）：本帧实际绘制出来的元素矩形 (源码偏移, 行顶, 行底)。
    /// 联动用的锚点直接取它——比「推断布局游标」可靠得多。
    placements: Vec<(usize, f32, f32)>,
    /// 本地新增（EasyMD）：是否让正文文字块感应点击
    /// （由 `CommonMarkViewer::text_clicks` 是否传入收集器决定）
    text_click_enabled: bool,
    /// 本地新增（EasyMD）：本次渲染中被点击的文字块源码偏移
    clicked_text: Vec<usize>,
}

pub(crate) struct CheckboxClickEvent {
    pub(crate) checked: bool,
    pub(crate) span: Range<usize>,
}

impl CommonMarkViewerInternal {
    pub fn new() -> Self {
        Self {
            curr_table: 0,
            text_style: Style::default(),
            list: List::default(),
            link: None,
            image: None,
            line: Newline::default(),
            is_list_item: false,
            def_list: Default::default(),
            code_block: None,
            html_block: String::new(),
            is_table: false,
            is_blockquote: false,
            checkbox_events: Vec::new(),
            deferred_scroll_to_heading: None,
            diag_content_top: 0.0,
            placements: Vec::new(),
            text_click_enabled: false,
            clicked_text: Vec::new(),
        }
    }
}

fn parser_options_extras(
    is_math_enabled: bool,
    is_scroll_to_heading_enabled: bool,
) -> pulldown_cmark::Options {
    let mut result = parser_options();
    if is_math_enabled {
        result |= pulldown_cmark::Options::ENABLE_MATH;
    }
    if is_scroll_to_heading_enabled {
        result |= pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;
    }
    result
}

impl CommonMarkViewerInternal {
    /// Be aware that this acquires egui::Context internally.
    /// If split Id is provided then split points will be populated
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        text: &str,
        split_points_id: Option<Id>,
        block_positions: Option<&std::cell::RefCell<Vec<(usize, f32, f32)>>>,
        text_clicks: Option<&std::cell::RefCell<Vec<usize>>>,
    ) -> (egui::InnerResponse<()>, Vec<CheckboxClickEvent>) {
        let max_width = options.max_width(ui);
        let layout = egui::Layout::left_to_right(egui::Align::BOTTOM).with_main_wrap(true);

        // 本地新增（EasyMD）：记录渲染起点的纵向坐标，用于换算锚点的内容相对位置
        let content_top = ui.next_widget_position().y;
        self.diag_content_top = content_top;
        self.placements.clear();
        self.text_click_enabled = text_clicks.is_some();
        self.clicked_text.clear();
        let re = ui.allocate_ui_with_layout(egui::vec2(max_width, 0.0), layout, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let height = ui.text_style_height(&TextStyle::Body);
            ui.set_row_height(height);

            let mut events = pulldown_cmark::Parser::new_ext(
                text,
                parser_options_extras(options.math_fn.is_some(), options.enable_scroll_to_heading),
            )
            .into_offset_iter()
            .enumerate()
            .peekable();

            while let Some((index, (e, src_span))) = events.next() {
                if index == 0 {
                    self.line.should_not_start_newline_forced = false;
                }

                self.process_event(ui, &mut events, e, src_span, cache, options, max_width);

                // 本地修改（EasyMD）：把本事件期间**实际绘制出来**的元素矩形交给收集器，
                // 作为「源码字节偏移 → 行顶/行底」锚点，供左右分栏做行级联动。
                // 这些矩形来自 ui.label / ui.add 的返回值，是真实绘制位置，
                // 比用布局游标推断可靠（自动换行布局里文本不会推进布局游标）。
                if let Some(sink) = block_positions {
                    if !self.placements.is_empty() {
                        let mut sink = sink.borrow_mut();
                        for (off, top, bottom) in self.placements.drain(..) {
                            let (top, bottom) = match sink.last() {
                                Some((_, last_top, last_bottom)) => (
                                    top.max(*last_top),
                                    bottom.max(*last_bottom).max(top),
                                ),
                                None => (top, bottom.max(top)),
                            };
                            let need_push = match sink.last() {
                                None => true,
                                Some((last_offset, last_top, _)) => {
                                    off > *last_offset
                                        || (off == *last_offset && (top - last_top).abs() > 0.5)
                                }
                            };
                            if need_push {
                                sink.push((off, top, bottom));
                            } else if let Some(last) = sink.last_mut() {
                                last.2 = bottom;
                            }
                        }
                    }
                }
            }

            // deferral to make it consistent no matter whether the target is before or after the link
            *cache.scroll_to_id_target_mut() = self.deferred_scroll_to_heading.take();

            if let Some(source_id) = split_points_id {
                scroll_cache(cache, &source_id).page_size =
                    Some(ui.next_widget_position().to_vec2());
            }
        });

        // 本地新增（EasyMD）：把本次渲染中被点击的文字块偏移交给收集器
        if let Some(sink) = text_clicks {
            if !self.clicked_text.is_empty() {
                sink.borrow_mut().append(&mut self.clicked_text);
            }
        }

        (re, std::mem::take(&mut self.checkbox_events))
    }

    pub(crate) fn show_scrollable(
        &mut self,
        source_id: Id,
        ui: &mut egui::Ui,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        text: &str,
    ) {
        let available_size = ui.available_size();
        let scroll_id = source_id.with("_scroll_area");

        let Some(page_size) = scroll_cache(cache, &source_id).page_size else {
            egui::ScrollArea::vertical()
                .id_salt(scroll_id)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    self.show(ui, cache, options, text, Some(source_id), None, None);
                });
            // Prevent repopulating points twice at startup
            scroll_cache(cache, &source_id).available_size = available_size;
            return;
        };

        let events = pulldown_cmark::Parser::new_ext(
            text,
            parser_options_extras(options.math_fn.is_some(), options.enable_scroll_to_heading),
        )
        .into_offset_iter()
        .collect::<Vec<_>>();

        let num_rows = events.len();

        egui::ScrollArea::vertical()
            .id_salt(scroll_id)
            // Elements have different widths, so the scroll area cannot try to shrink to the
            // content, as that will mean that the scroll bar will move when loading elements
            // with different widths.
            .auto_shrink([false, true])
            .show_viewport(ui, |ui, viewport| {
                ui.set_height(page_size.y);
                let layout = egui::Layout::left_to_right(egui::Align::BOTTOM).with_main_wrap(true);

                let max_width = options.max_width(ui);
                ui.allocate_ui_with_layout(egui::vec2(max_width, 0.0), layout, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let scroll_cache = scroll_cache(cache, &source_id);

                    // finding the first element that's not in the viewport anymore
                    let (first_event_index, _, first_end_position) = scroll_cache
                        .split_points
                        .iter()
                        .filter(|(_, _, end_position)| end_position.y < viewport.min.y)
                        .nth_back(1)
                        .copied()
                        .unwrap_or((0, Pos2::ZERO, Pos2::ZERO));

                    // finding the last element that's just outside the viewport
                    let last_event_index = scroll_cache
                        .split_points
                        .iter()
                        .filter(|(_, start_position, _)| start_position.y > viewport.max.y)
                        .nth(1)
                        .map(|(index, _, _)| *index)
                        .unwrap_or(num_rows);

                    ui.allocate_space(first_end_position.to_vec2());

                    // only rendering the elements that are inside the viewport
                    let mut events = events
                        .into_iter()
                        .enumerate()
                        .skip(first_event_index)
                        .take(last_event_index - first_event_index)
                        .peekable();

                    while let Some((i, (e, src_span))) = events.next() {
                        if events.peek().is_none() {
                            self.line.should_end_newline_forced = false;
                        }

                        self.process_event(ui, &mut events, e, src_span, cache, options, max_width);

                        if i == 0 {
                            self.line.should_not_start_newline_forced = false;
                        }
                    }
                });
            });

        // Forcing full re-render to repopulate split points for the new size
        let scroll_cache = scroll_cache(cache, &source_id);
        if available_size != scroll_cache.available_size {
            scroll_cache.available_size = available_size;
            scroll_cache.page_size = None;
            scroll_cache.split_points.clear();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_event<'e>(
        &mut self,
        ui: &mut Ui,
        events: &mut Peekable<impl Iterator<Item = EventIteratorItem<'e>>>,
        event: pulldown_cmark::Event,
        src_span: Range<usize>,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        max_width: f32,
    ) {
        self.event(ui, event, src_span, cache, options, max_width);

        self.def_list_def_wrapping(events, max_width, cache, options, ui);
        self.item_list_wrapping(events, max_width, cache, options, ui);
        self.table(events, cache, options, ui, max_width);
        self.blockquote(events, max_width, cache, options, ui);
    }

    fn def_list_def_wrapping<'e>(
        &mut self,
        events: &mut Peekable<impl Iterator<Item = EventIteratorItem<'e>>>,
        max_width: f32,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        ui: &mut Ui,
    ) {
        if self.def_list.is_def_list_def {
            self.def_list.is_def_list_def = false;

            let item_events = delayed_events(events, |tag| {
                matches!(tag, pulldown_cmark::TagEnd::DefinitionListDefinition)
            });

            let mut events_iter = item_events.into_iter().enumerate().peekable();

            self.line.try_insert_start(ui);

            // Proccess a single event separately so that we do not insert spaces where we do not
            // want them
            self.line.should_start_newline = false;
            if let Some((_, (e, src_span))) = events_iter.next() {
                self.process_event(ui, &mut events_iter, e, src_span, cache, options, max_width);
            }

            ui.label(" ".repeat(options.indentation_spaces));
            self.line.should_start_newline = true;
            self.line.should_end_newline = false;
            // Required to ensure that the content is aligned with the identation
            ui.horizontal_wrapped(|ui| {
                while let Some((_, (e, src_span))) = events_iter.next() {
                    self.process_event(
                        ui,
                        &mut events_iter,
                        e,
                        src_span,
                        cache,
                        options,
                        max_width,
                    );
                }
            });
            self.line.should_end_newline = true;

            // Only end the definition items line if it is not the last element in the list
            if !matches!(
                events.peek(),
                Some((
                    _,
                    (
                        pulldown_cmark::Event::End(pulldown_cmark::TagEnd::DefinitionList),
                        _
                    )
                ))
            ) {
                self.line.try_insert_end(ui);
            }
        }
    }

    fn item_list_wrapping<'e>(
        &mut self,
        events: &mut impl Iterator<Item = EventIteratorItem<'e>>,
        max_width: f32,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        ui: &mut Ui,
    ) {
        if self.is_list_item {
            self.is_list_item = false;

            let item_events = delayed_events_list_item(events);
            let mut events_iter = item_events.into_iter().enumerate().peekable();

            // Required to ensure that the content of the list item is aligned with
            // the * or - when wrapping
            ui.horizontal_wrapped(|ui| {
                while let Some((_, (e, src_span))) = events_iter.next() {
                    self.process_event(
                        ui,
                        &mut events_iter,
                        e,
                        src_span,
                        cache,
                        options,
                        max_width,
                    );
                }
            });
        }
    }

    fn blockquote<'e>(
        &mut self,
        events: &mut Peekable<impl Iterator<Item = EventIteratorItem<'e>>>,
        max_width: f32,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        ui: &mut Ui,
    ) {
        if self.is_blockquote {
            let mut collected_events = delayed_events(events, |tag| {
                matches!(tag, pulldown_cmark::TagEnd::BlockQuote(_))
            });
            self.line.try_insert_start(ui);

            // Currently the blockquotes are made in such a way that they need a newline at the end
            // and the start so when this is the first element in the markdown the newline must be
            // manually enabled
            self.line.should_not_start_newline_forced = false;
            if let Some(alert) = parse_alerts(&options.alerts, &mut collected_events) {
                egui_commonmark_backend::alert_ui(alert, ui, |ui| {
                    for (event, src_span) in collected_events {
                        self.event(ui, event, src_span, cache, options, max_width);
                    }
                })
            } else {
                blockquote(ui, ui.visuals().weak_text_color(), |ui| {
                    self.text_style.quote = true;
                    for (event, src_span) in collected_events {
                        self.event(ui, event, src_span, cache, options, max_width);
                    }
                    self.text_style.quote = false;
                });
            }

            if events.peek().is_none() {
                self.line.should_end_newline_forced = false;
            }

            self.line.try_insert_end(ui);
            self.is_blockquote = false;
        }
    }

    fn table<'e>(
        &mut self,
        events: &mut Peekable<impl Iterator<Item = EventIteratorItem<'e>>>,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        ui: &mut Ui,
        max_width: f32,
    ) {
        if self.is_table {
            self.line.try_insert_start(ui);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                let Table { header, rows } = parse_table(events);

                // ===== 本地修改（EasyMD）：表格在阅读栏宽内按内容分配列宽 =====
                // 原实现用「不换行的水平布局」渲染单元格，列多/内容长时整张表会超出栏宽被裁掉。
                // 现在按内容量算列宽：每列先量出「不换行的自然宽度」，装得下就按自然宽度
                // （一格都不换行），装不下才按比例压缩到刚好铺满栏宽。
                let columns = header.len().max(1);
                // 列间距给足，否则相邻两列的文字会挤在一起不好读（不画分隔线）
                let col_gap = 18.0f32.max(ui.spacing().item_spacing.x);
                let frame_padding = 18.0; // Frame::group 的内边距 + 少量余量
                let inner = (max_width.min(ui.available_width()) - frame_padding).max(96.0);
                let widths = column_widths(ui, &header, &rows, columns, inner, col_gap);

                // 行间距：单元格内部换行时也要留一点，避免行与行贴死。
                //
                // 这里不再用 `Grid`：它的列宽/行高语义与「自定义列宽」互相打架
                // （会从列宽里再扣一次间距、行高按错误的单元格尺寸算导致行重叠）。
                // 改成自己排版：行高由 `horizontal_top` 自动取最高的单元格，
                // 条纹用占位形状补画到文字下面。
                // 注意：文档正文是在「自动换行的水平布局」里排的（内联流），
                // 所以表格必须自己开一个竖向布局，否则各"行"会被当成内联元素排在一条线上。
                let stripe = ui.visuals().faint_bg_color;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for (row_index, row) in
                        std::iter::once(&header).chain(rows.iter()).enumerate()
                    {
                        let bg = ui.painter().add(egui::Shape::Noop);
                        let rect = ui
                            .horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = col_gap;
                                for (i, cell) in row.iter().enumerate() {
                                    let w = widths[i.min(widths.len() - 1)];
                                    table_cell(ui, cell, w, |ui, e, src_span| {
                                        if is_table_boundary(&e) {
                                            return;
                                        }
                                        let tmp_start = std::mem::replace(
                                            &mut self.line.should_start_newline,
                                            false,
                                        );
                                        let tmp_end = std::mem::replace(
                                            &mut self.line.should_end_newline,
                                            false,
                                        );
                                        self.event(ui, e, src_span, cache, options, max_width);
                                        self.line.should_start_newline = tmp_start;
                                        self.line.should_end_newline = tmp_end;
                                    });
                                }
                            })
                            .response
                            .rect;
                        if row_index % 2 == 1 {
                            ui.painter().set(bg, egui::Shape::rect_filled(rect, 0.0, stripe));
                        }
                    }
                });
                // ===== 本地修改结束 =====
            });

            self.is_table = false;
            if events.peek().is_none() {
                self.line.should_end_newline_forced = false;
            }

            self.line.try_insert_end(ui);
        }
    }

    fn event(
        &mut self,
        ui: &mut Ui,
        event: pulldown_cmark::Event,
        src_span: Range<usize>,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        max_width: f32,
    ) {
        match event {
            pulldown_cmark::Event::Start(tag) => self.start_tag(ui, tag, cache, options),
            pulldown_cmark::Event::End(tag) => self.end_tag(ui, tag, cache, options, max_width),
            pulldown_cmark::Event::Text(text) => {
                self.event_text(text, ui, src_span.clone());
            }
            pulldown_cmark::Event::Code(text) => {
                self.text_style.code = true;
                self.event_text(text, ui, src_span.clone());
                self.text_style.code = false;
            }
            pulldown_cmark::Event::InlineHtml(text) => {
                self.event_text(text, ui, src_span.clone());
            }

            pulldown_cmark::Event::Html(text) => {
                if options.html_fn.is_some() {
                    self.html_block.push_str(&text);
                } else {
                    self.event_text(text, ui, src_span.clone());
                }
            }
            pulldown_cmark::Event::FootnoteReference(footnote) => {
                footnote_start(ui, &footnote);
            }
            pulldown_cmark::Event::SoftBreak => {
                soft_break(ui);
            }
            pulldown_cmark::Event::HardBreak => newline(ui),
            pulldown_cmark::Event::Rule => {
                self.line.try_insert_start(ui);
                rule(ui, self.line.can_insert_end());
            }
            pulldown_cmark::Event::TaskListMarker(mut checkbox) => {
                if options.mutable {
                    if ui
                        .add(egui::Checkbox::without_text(&mut checkbox))
                        .clicked()
                    {
                        self.checkbox_events.push(CheckboxClickEvent {
                            checked: checkbox,
                            span: src_span,
                        });
                    }
                } else {
                    ui.add(ImmutableCheckbox::without_text(&mut checkbox));
                }
            }
            pulldown_cmark::Event::InlineMath(tex) => {
                if let Some(math_fn) = options.math_fn {
                    math_fn(ui, &tex, true);
                }
            }
            pulldown_cmark::Event::DisplayMath(tex) => {
                if let Some(math_fn) = options.math_fn {
                    math_fn(ui, &tex, false);
                }
            }
        }
    }

    fn event_text(&mut self, text: CowStr, ui: &mut Ui, src_span: Range<usize>) {
        let rich_text = self.text_style.to_richtext(ui, &text);
        if let Some(image) = &mut self.image {
            image.alt_text.push(rich_text);
        } else if let Some(block) = &mut self.code_block {
            block.content.push_str(&text);
        } else if let Some(link) = &mut self.link {
            link.text.push(rich_text);
        } else {
            // 本地修改（EasyMD）：记录实际绘制矩形
            let resp = if self.text_click_enabled {
                // 只在需要「点正文定位」时让文字感应点击，避免影响默认交互
                ui.add(egui::Label::new(rich_text).sense(egui::Sense::click()))
            } else {
                ui.label(rich_text)
            };
            if resp.clicked() {
                self.clicked_text.push(src_span.start);
            }
            let top = resp.rect.top() - self.diag_content_top;
            let bottom = resp.rect.bottom() - self.diag_content_top;
            self.placements.push((src_span.start, top, bottom));
        }
    }

    fn start_tag(
        &mut self,
        ui: &mut Ui,
        tag: pulldown_cmark::Tag,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
    ) {
        match tag {
            pulldown_cmark::Tag::Paragraph => {
                self.line.try_insert_start(ui);
            }
            pulldown_cmark::Tag::Heading { level, id, .. } => {
                if let Some(scroll_target) = cache.scroll_to_id_target()
                    && let Some(id) = id
                    && id.into_string() == scroll_target
                {
                    ui.scroll_to_cursor(Some(egui::Align::TOP));
                    cache.scroll_to_id_target_mut().take();
                }

                // Headings should always insert a newline even if it is at the start.
                // Whether this is okay in all scenarios is a different question.
                newline(ui);
                self.text_style.heading = Some(match level {
                    HeadingLevel::H1 => 0,
                    HeadingLevel::H2 => 1,
                    HeadingLevel::H3 => 2,
                    HeadingLevel::H4 => 3,
                    HeadingLevel::H5 => 4,
                    HeadingLevel::H6 => 5,
                });
            }

            // deliberately not using the built in alerts from pulldown-cmark as
            // the markdown itself cannot be localized :( e.g: [!TIP]
            pulldown_cmark::Tag::BlockQuote(_) => {
                self.is_blockquote = true;
            }
            pulldown_cmark::Tag::CodeBlock(c) => {
                match c {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        self.code_block = Some(crate::CodeBlock {
                            lang: Some(lang.to_string()),
                            content: "".to_string(),
                        });
                    }
                    pulldown_cmark::CodeBlockKind::Indented => {
                        self.code_block = Some(crate::CodeBlock {
                            lang: None,
                            content: "".to_string(),
                        });
                    }
                }
                self.line.try_insert_start(ui);
            }

            pulldown_cmark::Tag::List(point) => {
                if !self.list.is_inside_a_list() && self.line.can_insert_start() {
                    newline(ui);
                }

                if let Some(number) = point {
                    self.list.start_level_with_number(number);
                } else {
                    self.list.start_level_without_number();
                }
                self.line.should_start_newline = false;
                self.line.should_end_newline = false;
            }

            pulldown_cmark::Tag::Item => {
                self.is_list_item = true;
                self.list.start_item(ui, options);
            }

            pulldown_cmark::Tag::FootnoteDefinition(note) => {
                self.line.try_insert_start(ui);

                self.line.should_start_newline = false;
                self.line.should_end_newline = false;
                footnote(ui, &note);
            }
            pulldown_cmark::Tag::Table(_) => {
                self.is_table = true;
            }
            pulldown_cmark::Tag::TableHead => {}
            pulldown_cmark::Tag::TableRow => {}
            pulldown_cmark::Tag::TableCell => {}
            pulldown_cmark::Tag::Emphasis => {
                self.text_style.emphasis = true;
            }
            pulldown_cmark::Tag::Strong => {
                self.text_style.strong = true;
            }
            pulldown_cmark::Tag::Strikethrough => {
                self.text_style.strikethrough = true;
            }
            pulldown_cmark::Tag::Link { dest_url, .. } => {
                self.link = Some(crate::Link {
                    destination: dest_url.to_string(),
                    text: Vec::new(),
                });
            }
            pulldown_cmark::Tag::Image { dest_url, .. } => {
                self.image = Some(crate::Image::new(&dest_url, options));
            }
            pulldown_cmark::Tag::HtmlBlock => {
                self.line.try_insert_start(ui);
            }
            pulldown_cmark::Tag::MetadataBlock(_) => {}

            pulldown_cmark::Tag::DefinitionList => {
                self.line.try_insert_start(ui);
                self.def_list.is_first_item = true;
            }
            pulldown_cmark::Tag::DefinitionListTitle => {
                // we disable newline as the first title should not insert a newline
                // as we have already done that upon the DefinitionList Tag
                if !self.def_list.is_first_item {
                    self.line.try_insert_start(ui)
                } else {
                    self.def_list.is_first_item = false;
                }
            }
            pulldown_cmark::Tag::DefinitionListDefinition => {
                self.def_list.is_def_list_def = true;
            }
            // Not yet supported
            pulldown_cmark::Tag::Superscript | pulldown_cmark::Tag::Subscript => {}
        }
    }

    fn end_tag(
        &mut self,
        ui: &mut Ui,
        tag: pulldown_cmark::TagEnd,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        max_width: f32,
    ) {
        match tag {
            pulldown_cmark::TagEnd::Paragraph => {
                self.line.try_insert_end(ui);
            }
            pulldown_cmark::TagEnd::Heading { .. } => {
                self.line.try_insert_end(ui);
                self.text_style.heading = None;
            }
            pulldown_cmark::TagEnd::BlockQuote(_) => {}
            pulldown_cmark::TagEnd::CodeBlock => {
                self.end_code_block(ui, cache, options, max_width);
            }

            pulldown_cmark::TagEnd::List(_) => {
                if self.list.is_last_level() {
                    self.line.should_start_newline = true;
                    self.line.should_end_newline = true;
                }

                self.list.end_level(ui, self.line.can_insert_end());

                if !self.list.is_inside_a_list() {
                    // Reset all the state and make it ready for the next list that occurs
                    self.list = List::default();
                }
            }
            pulldown_cmark::TagEnd::Item => {}
            pulldown_cmark::TagEnd::FootnoteDefinition => {
                self.line.should_start_newline = true;
                self.line.should_end_newline = true;
                self.line.try_insert_end(ui);
            }
            pulldown_cmark::TagEnd::Table => {}
            pulldown_cmark::TagEnd::TableHead => {}
            pulldown_cmark::TagEnd::TableRow => {}
            pulldown_cmark::TagEnd::TableCell => {
                // Ensure space between cells
                ui.label("  ");
            }
            pulldown_cmark::TagEnd::Emphasis => {
                self.text_style.emphasis = false;
            }
            pulldown_cmark::TagEnd::Strong => {
                self.text_style.strong = false;
            }
            pulldown_cmark::TagEnd::Strikethrough => {
                self.text_style.strikethrough = false;
            }
            pulldown_cmark::TagEnd::Link => {
                if let Some(link) = self.link.take() {
                    link.end(ui, cache, options, &mut self.deferred_scroll_to_heading);
                }
            }
            pulldown_cmark::TagEnd::Image => {
                if let Some(image) = self.image.take() {
                    image.end(ui, options);
                }
            }
            pulldown_cmark::TagEnd::HtmlBlock => {
                if let Some(html_fn) = options.html_fn {
                    html_fn(ui, &self.html_block);
                    self.html_block.clear();
                }
            }

            pulldown_cmark::TagEnd::MetadataBlock(_) => {}

            pulldown_cmark::TagEnd::DefinitionList => self.line.try_insert_end(ui),
            pulldown_cmark::TagEnd::DefinitionListTitle
            | pulldown_cmark::TagEnd::DefinitionListDefinition => {}
            pulldown_cmark::TagEnd::Superscript | pulldown_cmark::TagEnd::Subscript => {}
        }
    }

    fn end_code_block(
        &mut self,
        ui: &mut Ui,
        cache: &mut CommonMarkCache,
        options: &CommonMarkOptions,
        max_width: f32,
    ) {
        if let Some(block) = self.code_block.take() {
            block.end(ui, cache, options, max_width);
            self.line.try_insert_end(ui);
        }
    }
}

/// 本地新增（EasyMD）：渲染一个表格单元格（固定列宽，内容超出自动换行）。
///
/// 两个要点：
/// - `set_min_width(column_width)`：egui 里文本标签**不会推进布局游标**（只有 `allocate_*` 会），
///   不把列宽显式写进本单元格的 `min_rect`，这一列就会被当成"只有文字那么宽"，
///   右边几列全部左移、挤成一团（行高也会算错，出现行与行重叠）。
/// - 外层 `vertical`：让单元格高度按实际换行行数计算，多行单元格不会被下一行盖住。
fn table_cell<'e>(
    ui: &mut Ui,
    cell: &[(pulldown_cmark::Event<'e>, Range<usize>)],
    column_width: f32,
    mut render: impl FnMut(&mut Ui, pulldown_cmark::Event<'e>, Range<usize>),
) {
    let column_width = column_width.max(16.0);
    ui.allocate_ui_with_layout(
        egui::vec2(column_width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(column_width);
            // 与上游渲染正文一致：文字之间不留 item 间距（列间距由外层水平布局统一给）
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.set_max_width(column_width);
            ui.horizontal_wrapped(|ui| {
                for (e, src_span) in cell {
                    render(ui, e.clone(), src_span.clone());
                }
            });
        },
    );
}

/// 本地新增（EasyMD）：按内容量分配各列宽度（**只算文本宽度，不含列间距**）。
///
/// - 先量每列「不换行的自然宽度」（取该列所有单元格里最宽的）；
/// - 总宽装得下就按自然宽度（不换行，表格宽度贴合内容）；
/// - 装不下才按比例压缩到刚好铺满可用宽度（此时才会换行，且没有多余空白）。
///
/// 注意：这里算的是**文字宽度**。列与列之间的间距由外层水平布局的
/// `item_spacing.x` 负责，不能在这份宽度里再扣一次，否则紧的单元格会差几个像素而换行。
fn column_widths(
    ui: &Ui,
    header: &[Vec<(pulldown_cmark::Event<'_>, Range<usize>)>],
    rows: &[Vec<Vec<(pulldown_cmark::Event<'_>, Range<usize>)>>],
    columns: usize,
    inner: f32,
    spacing: f32,
) -> Vec<f32> {
    let body = TextStyle::Body.resolve(ui.style());
    let code = TextStyle::Monospace.resolve(ui.style());
    let mut natural = vec![0.0f32; columns];
    let mut consider = |cell: &Vec<(pulldown_cmark::Event<'_>, Range<usize>)>, col: usize| {
        if col >= columns {
            return;
        }
        let w = cell_natural_width(ui, cell, &body, &code);
        natural[col] = natural[col].max(w);
    };
    for (i, cell) in header.iter().enumerate() {
        consider(cell, i);
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            consider(cell, i);
        }
    }

    let avail = (inner - spacing * columns.saturating_sub(1) as f32).max(48.0);
    let total: f32 = natural.iter().sum();
    if total <= avail || total <= 0.0 {
        // 内容装得下：按自然宽度，一格也不换行
        return natural.into_iter().map(|w| w.max(24.0)).collect();
    }
    // 装不下：按内容比例压缩；很窄的列保留其自然宽度，剩下的宽度按比例分给需要换行的列
    const MIN_COL: f32 = 40.0;
    let mut widths: Vec<f32> = natural
        .iter()
        .map(|n| if *n <= MIN_COL { *n } else { *n * avail / total })
        .collect();
    let sum: f32 = widths.iter().sum();
    if sum > avail {
        let k = avail / sum;
        for w in &mut widths {
            *w *= k;
        }
    }
    widths.into_iter().map(|w| w.max(24.0)).collect()
}

/// 本地新增（EasyMD）：表格结构类事件。
///
/// `parse_table` 会把「上一格的 End(TableCell)」和「本格的 Start(TableCell)」一起放进
/// 单元格的事件列表里；上游就是靠这个 End 在每个格子内部画一个 `"  "` 当作列间距
/// （`TagEnd::TableCell => ui.label("  ")`）。用 Grid 排版后列间距由单元格内边距负责，
/// 这个分隔符反而会吃掉 8px 宽度，让「明明放得下的文字被挤成两行」。
fn is_table_boundary(e: &pulldown_cmark::Event<'_>) -> bool {
    use pulldown_cmark::{Event, Tag, TagEnd};
    matches!(
        e,
        Event::Start(Tag::TableCell)
            | Event::End(TagEnd::TableCell)
            | Event::Start(Tag::TableRow)
            | Event::End(TagEnd::TableRow)
            | Event::Start(Tag::TableHead)
            | Event::End(TagEnd::TableHead)
            | Event::Start(Tag::Table(_))
            | Event::End(TagEnd::Table)
    )
}

/// 本地新增（EasyMD）：单元格「不换行」时的文本宽度（正文与行内代码用各自字体量）。
fn cell_natural_width(
    ui: &Ui,
    cell: &[(pulldown_cmark::Event<'_>, Range<usize>)],
    body: &egui::FontId,
    code: &egui::FontId,
) -> f32 {
    // 留 2px 容差：折行布局按浮点比较「放不放得下」，正好等于文字宽度时
    // 会因为亚像素舍入把最后一个字挤到下一行。
    let mut width = 2.0f32;
    for (event, _) in cell {
        // 表格边界事件不画内容（见 is_table_boundary）
        if is_table_boundary(event) {
            continue;
        }
        match event {
            pulldown_cmark::Event::Text(t) => width += measure_text(ui, t, body),
            pulldown_cmark::Event::Code(t) => width += measure_text(ui, t, code),
            pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                width += measure_text(ui, " ", body)
            }
            // 图片在单元格里很少见：给一个保守的占位宽度
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { .. }) => width += 64.0,
            _ => {}
        }
    }
    width
}

fn measure_text(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.fonts_mut(|f| {
        f.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    })
}
