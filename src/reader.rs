// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 右侧阅读面板：Markdown 渲染、标题锚点跳转、本地链接、图片、滚动位置记忆。

use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::config::AppConfig;
use crate::doc::Document;
use crate::theme::{BackgroundImage, Theme};

pub struct ReaderOutput {
    /// 被点击的链接目的地（需要打开仓库内文件）
    pub link_clicked: Option<String>,
    /// 当前滚动偏移（用于记忆阅读位置）
    pub scroll_offset: f32,
    /// 被点击文字块的「渲染文本」字节偏移（比纵坐标换算更精确）
    pub clicked_text_offset: Option<usize>,
    /// 本帧「源码字节偏移 → 行顶/行底」锚点表（左右分栏联动用）
    pub anchors: Vec<(usize, f32, f32)>,
    /// 在正文区域点击时，点击处的「内容相对纵坐标」
    pub clicked_at_y: Option<f32>,
}

/// 分栏联动相关的输入。
pub struct ReaderOpts<'a> {
    /// 需要滚动到的内容相对纵坐标（联动时由编辑器侧给出）。
    ///
    /// 注意这里只发出一个「动画滚动请求」（`Ui::scroll_to_rect`），
    /// **不会**直接改写 ScrollArea 的偏移：
    /// egui 内部有动画滚动目标，如果每帧去写死偏移，两者会互相拉扯导致永不收敛、
    /// 无限重绘（表现为滑动时卡死）。请求是幂等的，到达目标后自动失效。
    pub scroll_to_y: Option<f32>,
    /// 上一帧的锚点表：把「当前块」的高亮画在文字下方
    pub prev_anchors: &'a [(usize, f32, f32)],
    /// 需要高亮的源码字节范围
    pub highlight: Option<(usize, usize)>,
}

impl Default for ReaderOpts<'_> {
    fn default() -> Self {
        Self {
            scroll_to_y: None,
            prev_anchors: &[],
            highlight: None,
        }
    }
}

/// 「最后一个字节偏移 <= offset」的锚点：返回 (行顶, 行底)。
pub fn range_for_offset(anchors: &[(usize, f32, f32)], offset: usize) -> Option<(f32, f32)> {
    let idx = anchors.partition_point(|(o, _, _)| *o <= offset);
    if idx == 0 {
        anchors.first().map(|(_, t, b)| (*t, *b))
    } else {
        anchors.get(idx - 1).map(|(_, t, b)| (*t, *b))
    }
}

/// 「最后一个字节偏移 <= offset」的锚点行顶（该偏移所在行的顶部）。
pub fn y_for_offset(anchors: &[(usize, f32, f32)], offset: usize) -> Option<f32> {
    range_for_offset(anchors, offset).map(|(t, _)| t)
}

/// 反查：纵向位置 y 落在哪一行，返回该行的源码字节偏移（预览 → 编辑器方向）。
pub fn offset_for_y(anchors: &[(usize, f32, f32)], y: f32) -> Option<usize> {
    let y = y.max(0.0);
    let idx = anchors.partition_point(|(_, t, _)| *t <= y);
    anchors.get(idx.saturating_sub(1)).map(|(o, _, _)| *o)
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    doc: &mut Document,
    cfg: &AppConfig,
    theme: &Theme,
    cache: &mut CommonMarkCache,
    bg: &BackgroundImage,
    restore_offset: Option<f32>,
    top_inset: f32,
    opts: &ReaderOpts<'_>,
) -> ReaderOutput {
    let mut out = ReaderOutput {
        link_clicked: None,
        scroll_offset: 0.0,
        anchors: Vec::new(),
        clicked_at_y: None,
        clicked_text_offset: None,
    };

    if let Some(err) = &doc.load_error {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(format!("无法打开文档：{err}"))
                    .color(theme.danger),
            );
        });
        return out;
    }
    if doc.binary {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("这是二进制文件，无法以 Markdown 阅读").color(theme.weak));
        });
        return out;
    }

    let row_h = ui.text_style_height(&egui::TextStyle::Body);
    let _ = row_h;

    // 滚动区域：每个文档独立记忆位置（id 与 offset 都按路径区分）。
    // 只做竖向滚动：宽表格由本地补丁在栏宽内换行、长代码行由渲染器软换行，因此不需要横向滚动。
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt(("easymd-read", &doc.path))
        .auto_shrink([false, false]);
    if let Some(off) = restore_offset {
        scroll = scroll.vertical_scroll_offset(off);
    }


    let avail = ui.available_width();
    let col_w = cfg.content_width.min(avail - 40.0).max(280.0);
    let pad = ((avail - col_w) * 0.5).max(12.0);

    // 待跳转锚点（大纲 / 文档内链接）
    if let Some(anchor) = doc.pending_anchor.take() {
        *cache.scroll_to_id_target_mut() = Some(anchor);
    }

    // 本地链接拦截：注册 hook，点击后由本程序处理（打开仓库内文件 / 外部浏览器）
    let targets = doc.link_targets.clone();
    for t in &targets {
        if !crate::doc::is_anchor(t) {
            cache.add_link_hook(t.clone());
        }
    }

    let doc_dir_prefix = crate::util::doc_uri_prefix(&doc.dir());
    let anchors_sink: std::cell::RefCell<Vec<(usize, f32, f32)>> =
        std::cell::RefCell::new(Vec::new());
    // 正文文字块的点击收集器（用于「点预览 → 编辑器光标跳过去」）
    let text_clicks: std::cell::RefCell<Vec<usize>> = std::cell::RefCell::new(Vec::new());
    let highlight = opts.highlight;
    let prev_anchors = opts.prev_anchors;
    let mut content_top_screen = 0.0f32;
    let mut content_rect = egui::Rect::NOTHING;
    let mut clicked_y: Option<f32> = None;

    let scroll_out = scroll.show(ui, |ui| {
        ui.add_space(18.0 + top_inset);
        // 联动：请求滚动到编辑器给出的位置（动画执行，不直接改偏移）
        if let Some(y) = opts.scroll_to_y {
            let top = ui.next_widget_position().y + y.max(0.0);
            ui.scroll_to_rect(
                egui::Rect::from_min_size(
                    egui::pos2(ui.max_rect().left(), top),
                    egui::vec2(1.0, 1.0),
                ),
                Some(egui::Align::TOP),
            );
        }
        ui.horizontal(|ui| {
            ui.add_space(pad);
            let card_bg = if cfg.bg_image.is_some() {
                // 有背景图时，给正文加一层半透明底板，保证可读性
                Some(egui::Color32::from_rgba_unmultiplied(
                    theme.bg.r(),
                    theme.bg.g(),
                    theme.bg.b(),
                    236,
                ))
            } else {
                None
            };
            let mut frame = egui::Frame::new().inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: 0,
            });
            if let Some(bg) = card_bg {
                frame = frame
                    .fill(bg)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(24, 20));
            }
            frame.show(ui, |ui| {
                // 正文内容原点（与锚点表同一参考系）
                content_top_screen = ui.next_widget_position().y;
                content_rect = ui.available_rect_before_wrap();
                // 光标所在块的高亮：先画，位于文字下方（位置取自上一帧锚点表）
                if let Some((start, end)) = highlight {
                    // 锚点表是「渲染文本」坐标，高亮范围来自原文，先换算过去
                    let start = doc.text_to_render_offset(start);
                    let end = doc.text_to_render_offset(end);
                    if let Some((y0, y0_bottom)) = range_for_offset(prev_anchors, start) {
                        // 块底取「块范围内所有锚点底边的最大值」：
                        // 比用末尾偏移查表更稳（末偏移可能落到下一块的行上）
                        let mut y1 = y0_bottom;
                        for (off, _top, bottom) in prev_anchors.iter() {
                            if *off >= start && *off <= end {
                                y1 = y1.max(*bottom);
                            }
                        }
                        let y1 = y1.max(y0 + ui.text_style_height(&egui::TextStyle::Body));
                        let top = content_top_screen + y0 - 2.0;
                        let bottom = (content_top_screen + y1 + 1.0).max(top + 14.0);
                        let right = (content_rect.left() + col_w).min(content_rect.right());
                        let rect = egui::Rect::from_min_max(
                            egui::pos2(content_rect.left(), top),
                            egui::pos2(right, bottom),
                        );
                        ui.painter().rect_filled(
                            rect,
                            egui::CornerRadius::same(4),
                            theme.selection.gamma_multiply(0.45),
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                rect.left_top(),
                                egui::vec2(2.0, rect.height()),
                            ),
                            egui::CornerRadius::same(1),
                            theme.accent,
                        );
                    }
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(col_w - if card_bg.is_some() { 48.0 } else { 0.0 }, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let mut viewer = CommonMarkViewer::new()
                            .default_implicit_uri_scheme(doc_dir_prefix.clone())
                            .max_image_width(Some(col_w as usize))
                            .default_width(Some(col_w as usize))
                            .enable_scroll_to_heading(true)
                            .show_alt_text_on_hover(true)
                            .block_positions(&anchors_sink)
                            .text_clicks(&text_clicks);
                        viewer = viewer.indentation_spaces(24);
                        // 正文区的点击检测。
                        //
                        // viewer 外层只是个 `scope`，它的 response 只 sense hover，
                        // `.clicked()` 永远为 false（所以以前点正文拿不到坐标）。
                        // 这里自己在内容矩形上注册一个 click 交互，必须**先于**渲染注册：
                        // egui 的命中测试是「后注册者（更靠上层）优先」，链接等内层控件
                        // 在渲染时注册得更晚，于是链接照旧可点，其余位置归这里。
                        // 注意：这个子 UI 是被 allocate 出来的零高矩形，`available_rect_before_wrap()`
                        // 高度为 0（命中测试查不到任何点）。纵向要用滚动视口（clip rect），
                        // 横向用正文栏宽。
                        let clip = ui.clip_rect();
                        let body_rect = egui::Rect::from_min_max(
                            egui::pos2(content_rect.left(), clip.top()),
                            egui::pos2(content_rect.left() + col_w, clip.bottom()),
                        );
                        let body_click = ui
                            .interact(
                                body_rect,
                                egui::Id::new(("easymd-body-click", &doc.path)),
                                egui::Sense::click(),
                            )
                            .clicked();
                        // 渲染补过锚点的文本，保证大纲与文档内锚点链接都能跳转
                        let _inner = viewer.show(ui, cache, &doc.render_text);
                        if body_click {
                            if let Some(pos) = ui.ctx().pointer_interact_pos() {
                                clicked_y = Some(pos.y - content_top_screen);
                            }
                        }
                    },
                );
            });
        });
        ui.add_space(48.0);
    });

    out.scroll_offset = scroll_out.state.offset.y;
    out.anchors = anchors_sink.into_inner();
    out.clicked_at_y = clicked_y;
    out.clicked_text_offset = text_clicks.into_inner().into_iter().next();

    // 处理链接点击
    for t in &targets {
        if crate::doc::is_anchor(t) {
            continue;
        }
        if cache.get_link_hook(t) == Some(true) {
            out.link_clicked = Some(t.clone());
            break;
        }
    }
    let _ = bg;

    out
}

/// 绘制窗口背景（主题底色 + 背景图片 + 蒙版）。
pub fn paint_background(
    ui: &egui::Ui,
    rect: egui::Rect,
    theme: &Theme,
    cfg: &AppConfig,
    bg: &BackgroundImage,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.bg);
    if cfg.bg_image.is_some() {
        bg.paint(
            &painter,
            rect,
            cfg.bg_image_fit,
            cfg.bg_image_opacity,
            cfg.bg_image_dim,
            theme.bg,
        );
    }
}


#[cfg(test)]
mod anchor_probe {
    /// 结构性检查锚点表：偏移单调不减、块首都有锚点、纵坐标不回退。
    /// （测试上下文里文本高度为 0，量不出真实像素，位置精度靠界面实测。）
    #[test]
    fn anchors_are_monotonic_and_cover_every_block() {
        let md = "# 标题一

第一段正文。

- 列表一
- 列表二

## 标题二

结束段。
";
        let sink = std::cell::RefCell::new(Vec::new());
        egui::__run_test_ui(|ui| {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new()
                .block_positions(&sink)
                .show(ui, &mut cache, md);
        });
        let anchors = sink.into_inner();
        assert!(!anchors.is_empty(), "应当记录到锚点");
        for w in anchors.windows(2) {
            assert!(w[1].0 >= w[0].0, "锚点偏移必须单调不减：{w:?}");
            assert!(w[1].1 >= w[0].1 - 0.5, "锚点纵坐标不应回退：{w:?}");
        }
        for needle in ["# 标题一", "第一段正文。", "- 列表一", "- 列表二", "## 标题二", "结束段。"] {
            let off = md.find(needle).unwrap();
            assert!(
                super::y_for_offset(&anchors, off).is_some(),
                "{needle} 处应能查到锚点"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors() -> Vec<(usize, f32, f32)> {
        // 每行一个锚点：(字节偏移, 行顶, 行底)
        vec![
            (0, 0.0, 26.0),
            (10, 30.0, 56.0),
            (25, 60.0, 86.0),
            (60, 90.0, 116.0),
            (80, 120.0, 146.0),
        ]
    }

    #[test]
    fn maps_offset_to_y() {
        let a = anchors();
        assert_eq!(y_for_offset(&a, 0), Some(0.0));
        assert_eq!(y_for_offset(&a, 5), Some(0.0));
        assert_eq!(y_for_offset(&a, 10), Some(30.0));
        assert_eq!(y_for_offset(&a, 24), Some(30.0), "落在行内应取该行顶部");
        assert_eq!(y_for_offset(&a, 999), Some(120.0), "超出范围取最后一行");
        assert_eq!(
            range_for_offset(&a, 24).map(|(_, b)| b),
            Some(56.0),
            "行底用于给高亮块收尾"
        );
        assert_eq!(range_for_offset(&a, 80), Some((120.0, 146.0)));
    }

    #[test]
    fn maps_y_back_to_offset() {
        let a = anchors();
        assert_eq!(offset_for_y(&a, 0.0), Some(0));
        assert_eq!(offset_for_y(&a, 29.0), Some(0), "还没到下一行就仍属于上一行");
        assert_eq!(offset_for_y(&a, 30.0), Some(10));
        assert_eq!(offset_for_y(&a, 1000.0), Some(80), "超出范围取最后一个");
        assert_eq!(offset_for_y(&[], 10.0), None);
    }

    #[test]
    fn round_trip_offset_y_offset() {
        let a = anchors();
        for (offset, y, _) in a.iter() {
            let y_back = y_for_offset(&a, *offset).unwrap();
            assert_eq!(y_back, *y);
            let off_back = offset_for_y(&a, y_back).unwrap();
            assert_eq!(off_back, *offset);
        }
    }
}

#[cfg(test)]
mod link_click_tests {
    /// 装一个真实系统字体（测试上下文默认字体缺失，文本高度会是 0，量不出可点击区域）。
    fn install_real_font(ctx: &egui::Context) {
        let candidates = [
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/simhei.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ];
        let mut defs = egui::FontDefinitions::default();
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let mut data = egui::FontData::from_owned(bytes);
                data.index = 0;
                defs.font_data
                    .insert("test-font".to_owned(), std::sync::Arc::new(data));
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    defs.families
                        .entry(family)
                        .or_default()
                        .insert(0, "test-font".to_owned());
                }
                break;
            }
        }
        ctx.set_fonts(defs);
    }

    /// 点击 Markdown 里的链接，必须触发 link hook（否则本地跳转与外部浏览器都不会有反应）。
    #[test]
    fn clicking_a_link_fires_the_hook() {
        let ctx = egui::Context::default();
        install_real_font(&ctx);
        let md = "[点我](note.md)";
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let mut clicked = false;
        let mut text_rect = egui::Rect::NOTHING;

        // 多跑几遍：第一遍装字体、第二遍量位置、第三／四遍按下与抬起
        for pass in 0..5 {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(400.0, 300.0),
            ));
            if pass >= 3 && text_rect != egui::Rect::NOTHING {
                let click_pos = egui::pos2(text_rect.left() + 12.0, text_rect.center().y);
                input.events.push(egui::Event::PointerMoved(click_pos));
                input.events.push(egui::Event::PointerButton {
                    pos: click_pos,
                    button: egui::PointerButton::Primary,
                    pressed: pass == 3,
                    modifiers: Default::default(),
                });
            }
            let mut full = ctx.run_ui(input, |ui| {
                cache.add_link_hook("note.md");
                let resp = egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, md);
                if pass == 2 {
                    text_rect = resp.response.rect;
                }
                if cache.get_link_hook("note.md") == Some(true) {
                    clicked = true;
                }
            });
            // 无头环境没有渲染后端，纹理增量要显式清掉（否则 epaint 会 panic）
            full.textures_delta.clear();
        }
        assert!(
            clicked,
            "点击链接应触发 hook（内容矩形={text_rect:?}）"
        );
    }
}

#[cfg(test)]
mod link_e2e_tests {
    use super::*;

    /// 点击正文文字块，应回报「该块的源码偏移」（点预览定位编辑器光标靠它）。
    /// 三段的偏移集合与纵坐标顺序都要对得上。
    #[test]
    fn clicking_text_reports_block_offsets_in_order() {
        let dir = std::env::temp_dir().join(format!("easymd-probe-y-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("three.md");
        let md = "第一段文字在这里。\n\n第二段文字在中间。\n\n第三段文字在下面。\n";
        std::fs::write(&main, md).unwrap();

        let ctx = egui::Context::default();
        install_font(&ctx);
        let cfg = crate::config::AppConfig::default();
        let theme = crate::theme::Theme::from_id(cfg.theme).apply_overrides(&cfg);
        let bg = crate::theme::BackgroundImage::default();
        let mut doc = crate::doc::Document::load(&main).unwrap();
        let mut cache = CommonMarkCache::default();
        frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![]);
        frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![]);

        let mut seen: Vec<usize> = Vec::new();
        for y in (30..460).step_by(4) {
            for x in [30.0f32, 90.0, 160.0] {
                let pos = egui::pos2(x, y as f32);
                let mv = egui::Event::PointerMoved(pos);
                let down = egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                };
                let up = egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                };
                frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![mv.clone()]);
                frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![mv, down]);
                let out = frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![up]);
                if let Some(off) = out.clicked_text_offset {
                    if seen.last() != Some(&off) {
                        seen.push(off);
                    }
                }
            }
        }
        println!("点击到的块偏移（按纵坐标顺序）= {seen:?}");

        let expected: Vec<usize> = ["第一段文字在这里。", "第二段文字在中间。", "第三段文字在下面。"]
            .iter()
            .map(|s| doc.render_text.find(s).expect("三段都应在渲染文本里"))
            .collect();
        for off in &seen {
            assert!(
                expected.contains(off),
                "回报的偏移 {off} 不属于任何文字块（期望之一：{expected:?}）"
            );
        }
        let order: Vec<usize> = seen
            .iter()
            .filter(|o| expected.contains(o))
            .map(|o| expected.iter().position(|e| e == o).unwrap())
            .collect();
        assert_eq!(
            order,
            (0..order.len()).collect::<Vec<_>>(),
            "点击越靠下，回报的块应越靠后：{seen:?}"
        );
        assert_eq!(seen.len(), 3, "三段文字都应能被点中：{seen:?}");
    }




    fn install_font(ctx: &egui::Context) {
        let candidates = [
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/simhei.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ];
        let mut defs = egui::FontDefinitions::default();
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let mut data = egui::FontData::from_owned(bytes);
                data.index = 0;
                defs.font_data
                    .insert("test-font".to_owned(), std::sync::Arc::new(data));
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    defs.families
                        .entry(family)
                        .or_default()
                        .insert(0, "test-font".to_owned());
                }
                break;
            }
        }
        ctx.set_fonts(defs);
    }

    /// 跑一帧完整 reader::show（可带输入事件），返回这一帧的 ReaderOutput。
    #[allow(clippy::too_many_arguments)]
    fn frame(
        ctx: &egui::Context,
        doc: &mut Document,
        cfg: &AppConfig,
        theme: &Theme,
        cache: &mut CommonMarkCache,
        bg: &BackgroundImage,
        events: Vec<egui::Event>,
    ) -> ReaderOutput {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(700.0, 500.0),
        ));
        input.events = events;
        let mut out: Option<ReaderOutput> = None;
        let mut full = ctx.run_ui(input, |ui| {
            out = Some(super::show(
                ui,
                doc,
                cfg,
                theme,
                cache,
                bg,
                None,
                0.0,
                &ReaderOpts::default(),
            ));
        });
        // 无头环境没有渲染后端，纹理增量要显式清掉（否则 epaint 会 panic）
        full.textures_delta.clear();
        out.unwrap()
    }

    /// 在指定位置按下+抬起，返回是否命中某个链接目的地。
    #[allow(clippy::too_many_arguments)]
    fn click_at(
        ctx: &egui::Context,
        doc: &mut Document,
        cfg: &AppConfig,
        theme: &Theme,
        cache: &mut CommonMarkCache,
        bg: &BackgroundImage,
        pos: egui::Pos2,
    ) -> Option<String> {
        let mv = egui::Event::PointerMoved(pos);
        let down = egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        };
        let up = egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        };
        frame(ctx, doc, cfg, theme, cache, bg, vec![mv.clone()]);
        frame(ctx, doc, cfg, theme, cache, bg, vec![mv, down]);
        let out = frame(ctx, doc, cfg, theme, cache, bg, vec![up]);
        out.link_clicked
    }

    /// 端到端：完整 reader::show 面板里，点击正文链接必须回报 link_clicked。
    #[test]
    fn reader_surface_reports_link_clicks() {
        let dir = std::env::temp_dir().join(format!("easymd-link-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.md");
        std::fs::write(
            &main,
            "# 标题\n\n[本地](note.md) 与 [外部](https://example.com) 两个链接。\n",
        )
        .unwrap();
        std::fs::write(dir.join("note.md"), "# 目标\n").unwrap();

        let ctx = egui::Context::default();
        install_font(&ctx);
        let cfg = crate::config::AppConfig::default();
        let theme = crate::theme::Theme::from_id(cfg.theme).apply_overrides(&cfg);
        let bg = crate::theme::BackgroundImage::default();
        let mut doc = crate::doc::Document::load(&main).unwrap();
        let mut cache = CommonMarkCache::default();

        // 先空跑两帧让字体和布局稳定
        frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![]);
        frame(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, vec![]);

        let mut hits: Vec<(egui::Pos2, String)> = Vec::new();
        let xs = [21.0f32, 25.0, 29.0, 33.0, 37.0, 42.0, 50.0, 62.0, 80.0, 110.0, 150.0, 200.0];
        'scan: for y in (6..200).step_by(2) {
            for x in xs {
                let pos = egui::pos2(x, y as f32);
                if let Some(dest) =
                    click_at(&ctx, &mut doc, &cfg, &theme, &mut cache, &bg, pos)
                {
                    println!("命中 {pos:?} → {dest}");
                    if !hits.iter().any(|(_, d)| d == &dest) {
                        hits.push((pos, dest));
                    }
                    if hits.iter().any(|(_, d)| d == "note.md")
                        && hits.iter().any(|(_, d)| d.starts_with("https://"))
                    {
                        break 'scan;
                    }
                }
            }
        }
        println!("命中集合：{hits:?}");
        assert!(
            hits.iter().any(|(_, d)| d == "note.md"),
            "完整面板里本地链接应可点击并回报：{hits:?}"
        );
        assert!(
            hits.iter().any(|(_, d)| d.starts_with("https://")),
            "完整面板里外部链接应可点击并回报：{hits:?}"
        );
    }
}

#[cfg(test)]
mod link_dest_probe {
    use crate::doc::{is_anchor, local_link_targets};

    fn parser_options() -> pulldown_cmark::Options {
        let mut opts = pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_TASKLISTS
            | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_FOOTNOTES
            | pulldown_cmark::Options::ENABLE_DEFINITION_LIST
            | pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;
        opts.insert(pulldown_cmark::Options::ENABLE_MATH);
        opts
    }

    /// 解析任意 Markdown 文本里的链接/图片目的地（渲染器看到的那一套）。
    fn rendered_dests(md: &str) -> Vec<String> {
        use pulldown_cmark::{Event, Parser, Tag};
        let mut out = Vec::new();
        for event in Parser::new_ext(md, parser_options()) {
            if let Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) = event
            {
                out.push(dest_url.to_string());
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// 注册的 hook key 必须与渲染器解析出的目的地完全一致，
    /// 否则点击会退回 `hyperlink_to`（egui 的 OpenUrl 没人处理 → 点了没反应）。
    #[test]
    fn registry_matches_renderer_for_example_doc() {
        let path = std::path::Path::new("examples/格式示例.md");
        let text = std::fs::read_to_string(path).expect("示例文档应存在");
        let registered = local_link_targets(&text);

        let doc = crate::doc::Document::load(path).expect("示例文档应能加载");
        let seen = rendered_dests(&doc.render_text);

        for t in registered.iter().filter(|t| !is_anchor(t)) {
            assert!(
                seen.contains(t),
                "注册了 hook 的 {t:?} 在渲染文本里找不到 → 点击永远不会命中"
            );
        }
    }

    /// 畸形构造（链接文本里塞标题、表格、跨行）不能把正常链接带坏。
    #[test]
    fn hostile_doc_keeps_links_addressable() {
        let md = "## 图片![\n### | **********列 1 | 列 2 | 列 3 |\n| --- | --- | --- |\n| 内容 | 内容 | 内容 |\n\n说明](路径)\n\n正常链接：[仓库说明](README.md)\n\n外部：[站点](https://www.rust-lang.org)\n";
        let registered = local_link_targets(md);
        let seen = rendered_dests(md);
        assert!(registered.contains(&"README.md".to_string()));
        assert!(seen.contains(&"README.md".to_string()));
        assert!(seen.contains(&"https://www.rust-lang.org".to_string()));
    }
}

#[cfg(test)]
mod table_layout_tests {
    /// 表格列宽的行为约束：
    /// 1. 内容放得下时**不许换行**，表格宽度贴合内容（不再被压成"可用宽度 ÷ 列数"）；
    /// 2. 内容超宽时按比例压缩到刚好铺满栏宽（换行不可避免，但右边不该留大片空白）；
    /// 3. 无论哪种情况，表格都不能超出栏宽。
    #[test]
    fn tables_size_columns_by_content() {
        let col_w = 660.0f32;
        let fits = [
            ("三列短内容", "| 列一 | 列二 | 列三 |\n| --- | --- | --- |\n| 甲 | 乙 | 丙 |\n"),
            (
                "三列中等内容",
                "| 说明 | 参数 | 备注 |\n| --- | --- | --- |\n| 这是一个比较长的中文单元格内容 | 参数名 | 备注文字 |\n",
            ),
            ("两列", "| 项目 | 说明 |\n| --- | --- |\n| 甲 | 乙 |\n"),
        ];
        for (label, md) in fits {
            let (width, max_rows, max_right) = measure_table(md, col_w);
            println!("{label}: 表宽={width:.1} 行数={max_rows}");
            assert_eq!(max_rows, 1, "{label}：放得下就不该换行");
            assert!(
                max_right <= col_w + 0.5,
                "{label}：表格不应超出栏宽（右边界 {max_right:.1}）"
            );
        }

        // 列与列之间要有明显间距（用户反馈"挨得太近不好读"）
        let gap = text_gap(
            "| 列一 | 列二 | 列三 |
| --- | --- | --- |
| 甲 | 乙 | 丙 |
",
            col_w,
            "列一",
            "列二",
        );
        println!("列间距：{gap:?}");
        assert!(
            gap.map(|g| g >= 12.0).unwrap_or(false),
            "相邻两列文字之间应留出间距，实际 {:?}",
            gap
        );

        // 多行表格：行与行之间不得重叠（曾经因为单元格宽度没写进布局游标、
        // 行高算成 0 而叠在一起）
        let rows_md = "| 列一 | 列二 |\n| --- | --- |\n| 第一行 | 甲 |\n| 第二行 | 乙 |\n| 第三行 | 丙 |\n";
        let bands = text_bands(rows_md, col_w);
        println!("多行表格文本带：{bands:?}");
        assert!(bands.len() >= 4, "应当有表头 + 三行：{bands:?}");
        for pair in bands.windows(2) {
            assert!(
                pair[1].0 >= pair[0].1 - 0.5,
                "行与行不应重叠：{:?} 与 {:?}",
                pair[0],
                pair[1]
            );
        }

        // 单元格换行 + 多行混排时也不能重叠（行高必须按最高的单元格算）
        let wrap_md = "| 说明 | 参数 |
| --- | --- |
| 这段内容很长会在单元格里换成好几行显示 | 短 |
| 第二行 | 乙 |
| 第三行 | 丙 |
";
        let bands = text_bands(wrap_md, 300.0);
        println!("换行混排文本带（栏宽 300）：{bands:?}");
        for pair in bands.windows(2) {
            assert!(
                pair[1].0 >= pair[0].1 - 0.5,
                "换行单元格所在的行也不应重叠：{:?} 与 {:?}",
                pair[0],
                pair[1]
            );
        }

        // 超宽内容：必须压缩到铺满栏宽附近，且右边界不越界
        let long = "这是一段相当长的中文单元格内容，长到一页里根本放不下，只能换行显示才行呢。";
        let md = format!("| 说明 | 参数 | 备注 |\n| --- | --- | --- |\n| {long} | 参数名 | 备注文字 |\n");
        let (width, max_rows, max_right) = measure_table(&md, col_w);
        println!("超长内容: 表宽={width:.1} 行数={max_rows}");
        assert!(max_rows >= 2, "超宽内容应当换行");
        assert!(
            width >= col_w * 0.8,
            "超宽内容时表格应铺满栏宽附近，实际只用了 {width:.1}/{col_w}"
        );
        assert!(
            max_right <= col_w + 0.5,
            "超宽内容时也不应越过栏宽（右边界 {max_right:.1}）"
        );
    }

    /// 渲染一段 markdown，返回绘制形状（每次调用都新建上下文，保证字体与样式一致）
    fn render_shapes(md: &str, col_w: f32) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        let mut warm = egui::RawInput::default();
        warm.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        ));
        ctx.run_ui(warm, |_ui| {}).textures_delta.clear();
        let quick = crate::fonts::quick_defaults();
        let scan = crate::fonts::FontScan {
            families: quick.families(),
        };
        let cfg = crate::config::AppConfig::default();
        crate::fonts::install(
            &ctx,
            &scan,
            quick.prop.as_ref().map(|f| f.name.as_str()),
            quick.mono.as_ref().map(|f| f.name.as_str()),
            cfg.line_height,
            cfg.font_size,
        );
        let theme = crate::theme::Theme::from_id(cfg.theme).apply_overrides(&cfg);
        crate::app::apply_style_to_ctx(&ctx, &theme, &cfg, false);
        let mut warm2 = egui::RawInput::default();
        warm2.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        ));
        ctx.run_ui(warm2, |_ui| {}).textures_delta.clear();

        let mut cache = egui_commonmark::CommonMarkCache::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        ));
        let mut full = ctx.run_ui(input, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(col_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui_commonmark::CommonMarkViewer::new()
                        .default_width(Some(col_w as usize))
                        .max_image_width(Some(col_w as usize))
                        .show(ui, &mut cache, md);
                },
            );
        });
        full.textures_delta.clear();
        full.shapes
    }

    /// 渲染一段 markdown 表格，返回 (表格占宽, 单元格最大行数, 右边界)
    fn measure_table(md: &str, col_w: f32) -> (f32, usize, f32) {
        let mut texts: Vec<(f32, f32, usize, String)> = Vec::new();
        for clipped in render_shapes(md, col_w) {
            collect_texts(&clipped.shape, &mut texts);
        }
        let left = texts.iter().map(|t| t.0).fold(f32::MAX, f32::min);
        let right = texts.iter().map(|t| t.1).fold(f32::MIN, f32::max);
        // 只看有内容的文本（空 galley 是排版占位，行数没意义）
        let max_rows = texts
            .iter()
            .filter(|t| !t.3.trim().is_empty())
            .map(|t| t.2)
            .max()
            .unwrap_or(0);
        (right - left, max_rows, right)
    }

    /// 同一行里两段文字的横向间距（用来检查列间距）
    fn text_gap(md: &str, col_w: f32, a: &str, b: &str) -> Option<f32> {
        let mut texts: Vec<(f32, f32, usize, String)> = Vec::new();
        for clipped in render_shapes(md, col_w) {
            collect_texts(&clipped.shape, &mut texts);
        }
        let a = texts.iter().find(|t| t.3.trim() == a)?;
        let b = texts.iter().find(|t| t.3.trim() == b)?;
        Some(b.0 - a.1)
    }

    /// 按纵坐标把绘制出的文本分组成"行带"，用于检查行与行是否重叠
    fn text_bands(md: &str, col_w: f32) -> Vec<(f32, f32)> {
        let mut ys: Vec<(f32, f32)> = Vec::new();
        for clipped in render_shapes(md, col_w) {
            collect_band(&clipped.shape, &mut ys);
        }
        ys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut bands: Vec<(f32, f32)> = Vec::new();
        for (top, bottom) in ys {
            match bands.last_mut() {
                Some(last) if top <= last.1 + 1.0 => last.1 = last.1.max(bottom),
                _ => bands.push((top, bottom)),
            }
        }
        bands
    }

    fn collect_band(shape: &egui::Shape, out: &mut Vec<(f32, f32)>) {
        match shape {
            egui::Shape::Text(t) => {
                if t.galley.size().x > 0.5 && t.galley.size().y > 0.5 {
                    out.push((t.pos.y, t.pos.y + t.galley.size().y));
                }
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    collect_band(s, out);
                }
            }
            _ => {}
        }
    }

    fn collect_texts(shape: &egui::Shape, out: &mut Vec<(f32, f32, usize, String)>) {
        match shape {
            egui::Shape::Text(t) => {
                let text: String = t
                    .galley
                    .rows
                    .iter()
                    .flat_map(|r| r.row.glyphs.iter().map(|g| g.chr))
                    .collect();
                out.push((t.pos.x, t.pos.x + t.galley.size().x, t.galley.rows.len(), text));
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    collect_texts(s, out);
                }
            }
            _ => {}
        }
    }
}
