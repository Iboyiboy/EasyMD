// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 左侧资源管理器：文件树 / 大纲 / 全文检索三个页签。

use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::Op;
use crate::config::{AppConfig, SidebarTab, SortBy};
use crate::search::SearchState;
use crate::theme::Theme;
use crate::tree::{FlatRow, Tree};
use crate::ui_ext;

/// 拖拽负载。
#[derive(Clone, Debug)]
pub struct DragPayload {
    pub paths: Vec<PathBuf>,
}

pub struct RenameState {
    pub path: PathBuf,
    pub buffer: String,
    pub just_opened: bool,
}

#[derive(Default)]
pub struct ExplorerState {
    pub rename: Option<RenameState>,
    pub scroll_to_row: Option<usize>,
    pub search_input: String,
    pub search_case: bool,
    pub outline_filter: String,
    pub last_dir_click: Option<(PathBuf, Instant)>,
    /// 键盘导航的当前位置（相对于当前扁平行列表）
    pub focus_row: usize,
    /// 是否需要在下一帧聚焦搜索输入框
    pub focus_search: bool,
}

impl ExplorerState {
    pub fn start_rename(&mut self, path: PathBuf) {
        let name = crate::util::file_name(&path);
        self.rename = Some(RenameState {
            path,
            buffer: name,
            just_opened: true,
        });
    }

    pub fn cancel_rename(&mut self) {
        self.rename = None;
    }
}

/// 行高（随字体大小与行距自适应）。
pub fn row_height(cfg: &AppConfig) -> f32 {
    let text = cfg.font_size * 0.92;
    (text * cfg.line_height + 6.0).clamp(20.0, 56.0)
}

fn row_font(cfg: &AppConfig) -> FontId {
    FontId::proportional(cfg.font_size * 0.92)
}

// ---------------------------------------------------------------- 主入口

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    cfg: &AppConfig,
    theme: &Theme,
    tree: &mut Tree,
    st: &mut ExplorerState,
    search: &mut SearchState,
    headings: &[crate::doc::Heading],
    doc_path: Option<&Path>,
    ops: &mut Vec<Op>,
) {
    header(ui, cfg, theme, tree, ops);
    ui.add_space(2.0);

    let tabs = [
        (SidebarTab::Files, "文件"),
        (SidebarTab::Outline, "大纲"),
        (SidebarTab::Search, "搜索"),
    ];
    if let Some(t) = ui_ext::segmented(ui, cfg.sidebar_tab, &tabs, theme) {
        ops.push(Op::SetSidebarTab(t));
    }
    ui.add_space(4.0);

    match cfg.sidebar_tab {
        SidebarTab::Files => files_view(ui, cfg, theme, tree, st, doc_path, ops),
        SidebarTab::Outline => outline_view(ui, cfg, theme, headings, doc_path, st, ops),
        SidebarTab::Search => search_view(ui, cfg, theme, search, st, ops),
    }
}

// ---------------------------------------------------------------- 顶部工具条

fn header(ui: &mut Ui, cfg: &AppConfig, theme: &Theme, tree: &Tree, ops: &mut Vec<Op>) {
    let root_name = crate::util::file_name(&tree.root);
    ui.horizontal(|ui| {
        // 标题先占位，剩余空间留给右侧按钮，避免两者重叠
        let title_w = (ui.available_width() - 44.0).clamp(40.0, 240.0);
        let title = RichText::new(crate::util::elide(&root_name, 12)).strong();
        let resp = ui
            .allocate_ui_with_layout(
                egui::vec2(title_w, 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| ui.add(egui::Button::new(title).frame(false)),
            )
            .inner
            .on_hover_text(format!(
                "{}（点击回到仓库根目录）",
                tree.root.display()
            ));
        if resp.clicked() {
            // 与点空白处一致：取消选中 = 当前操作目标回到仓库根目录
            ops.push(Op::SelectRoot);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.menu_button("…", |ui| {
                ui.set_min_width(190.0);
                if ui.button("打开文件夹…").clicked() {
                    ops.push(Op::PickFolder);
                    ui.close();
                }
                if ui.button("打开文件…").clicked() {
                    ops.push(Op::PickFile);
                    ui.close();
                }
                ui.separator();
                ui.label(RichText::new("排序方式").weak().small());
                if ui
                    .selectable_label(cfg.sort_by == SortBy::Name, "按名称")
                    .clicked()
                {
                    ops.push(Op::SetSort(SortBy::Name));
                    ui.close();
                }
                if ui
                    .selectable_label(cfg.sort_by == SortBy::Modified, "按修改时间")
                    .clicked()
                {
                    ops.push(Op::SetSort(SortBy::Modified));
                    ui.close();
                }
                if ui.selectable_label(cfg.sort_desc, "倒序").clicked() {
                    ops.push(Op::ToggleSortDesc);
                    ui.close();
                }
                if ui.selectable_label(cfg.dirs_first, "文件夹优先").clicked() {
                    ops.push(Op::ToggleDirsFirst);
                    ui.close();
                }
                ui.separator();
                if ui
                    .selectable_label(cfg.show_hidden, "显示隐藏文件")
                    .clicked()
                {
                    ops.push(Op::ToggleShowHidden);
                    ui.close();
                }
                if ui
                    .selectable_label(cfg.show_non_md, "显示非 Markdown 文件")
                    .clicked()
                {
                    ops.push(Op::ToggleShowNonMd);
                    ui.close();
                }
                ui.separator();
                if ui.button("全部展开").clicked() {
                    ops.push(Op::ExpandAll);
                    ui.close();
                }
                if ui.button("全部折叠").clicked() {
                    ops.push(Op::CollapseAll);
                    ui.close();
                }
                if ui.button("刷新目录树").clicked() {
                    ops.push(Op::RefreshTree);
                    ui.close();
                }
                ui.separator();
                if ui.button("设置…").clicked() {
                    ops.push(Op::OpenSettings);
                    ui.close();
                }
                if ui.button("快捷键…").clicked() {
                    ops.push(Op::ShowShortcuts);
                    ui.close();
                }
            });
        });
    });

    // 第二行：常用操作（单独一行，窄侧栏下也不会与标题挤在一起）
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui_ext::tool_button(ui, "＋文件", "新建文件 (Ctrl+N)", theme).clicked() {
            ops.push(Op::NewFile(target_dir(tree)));
        }
        if ui_ext::tool_button(ui, "＋目录", "新建文件夹 (Ctrl+Shift+N)", theme).clicked() {
            ops.push(Op::NewFolder(target_dir(tree)));
        }
        if ui_ext::tool_button(ui, "刷新", "刷新目录树 (F5)", theme).clicked() {
            ops.push(Op::RefreshTree);
        }
        if ui_ext::tool_button(ui, "全部折叠", "折叠全部目录", theme).clicked() {
            ops.push(Op::CollapseAll);
        }
    });
}

/// 新建操作的目标目录：选中目录则用它，选中文件则用其父目录。
pub fn target_dir(tree: &Tree) -> PathBuf {
    match &tree.selected {
        Some(p) => {
            if p.is_dir() {
                p.clone()
            } else {
                p.parent()
                    .map(|x| x.to_path_buf())
                    .unwrap_or(tree.root.clone())
            }
        }
        None => tree.root.clone(),
    }
}

// ---------------------------------------------------------------- 文件页签

fn files_view(
    ui: &mut Ui,
    cfg: &AppConfig,
    theme: &Theme,
    tree: &mut Tree,
    st: &mut ExplorerState,
    doc_path: Option<&Path>,
    ops: &mut Vec<Op>,
) {
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut tree.query)
                .hint_text("搜索文件名…")
                .desired_width(ui.available_width() - 30.0)
                .margin(egui::Margin::symmetric(7, 4)),
        );
        if resp.changed() {
            tree.invalidate();
            st.focus_row = 0;
        }
        if !tree.query.is_empty() && ui_ext::tool_button(ui, "×", "清除搜索", theme).clicked() {
            tree.set_query(String::new());
        }
    });
    ui.add_space(4.0);

    let row_h = row_height(cfg);
    // 先注册「空白区域」交互（必须在绘制行之前注册，这样行的点击优先级更高）：
    // 点空白处 = 取消选中，也就意味着之后的新建 / 拖放都作用于仓库根目录，与 IDE 的手感一致。
    let viewport = ui.available_rect_before_wrap();
    let blank = ui.interact(
        viewport,
        egui::Id::new("easymd-tree-blank"),
        Sense::click(),
    );
    if blank.clicked() && !tree.is_searching() {
        tree.select(None);
        st.focus_row = 0;
        // 取消选中后，新建按钮的目标自动回到仓库根目录
    }
    blank.context_menu(|ui| {
        ui.set_min_width(180.0);
        ui.label(RichText::new("仓库根目录").weak().small());
        ui.separator();
        if ui.button("新建文件").clicked() {
            ops.push(Op::NewFile(tree.root.clone()));
            ui.close();
        }
        if ui.button("新建文件夹").clicked() {
            ops.push(Op::NewFolder(tree.root.clone()));
            ui.close();
        }
        ui.separator();
        if ui.button("全部展开").clicked() {
            ops.push(Op::ExpandAll);
            ui.close();
        }
        if ui.button("全部折叠").clicked() {
            ops.push(Op::CollapseAll);
            ui.close();
        }
        if ui.button("刷新目录树").clicked() {
            ops.push(Op::RefreshTree);
            ui.close();
        }
        ui.separator();
        if ui
            .selectable_label(cfg.show_hidden, "显示隐藏文件")
            .clicked()
        {
            ops.push(Op::ToggleShowHidden);
            ui.close();
        }
        if ui
            .selectable_label(cfg.show_non_md, "显示非 Markdown 文件")
            .clicked()
        {
            ops.push(Op::ToggleShowNonMd);
            ui.close();
        }
        if ui.button("在文件管理器中显示").clicked() {
            ops.push(Op::RevealInOs(tree.root.clone()));
            ui.close();
        }
    });

    let rows = tree.take_rows(cfg);
    if rows.is_empty() {
        tree.put_rows(rows, cfg);
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            if tree.is_searching() {
                ui.label(RichText::new("没有匹配的文件").weak());
            } else {
                ui.label(RichText::new("这个文件夹里还没有内容").weak());
                ui.add_space(6.0);
                if ui.button("在这里新建文件").clicked() {
                    ops.push(Op::NewFile(tree.root.clone()));
                }
            }
        });
        return;
    }

    let selected = tree.selected.clone();
    let open_doc = doc_path.map(|p| p.to_path_buf());

    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("easymd-tree")
        .auto_shrink([false, false]);
    if let Some(idx) = st.scroll_to_row.take() {
        let off = (idx as f32) * (row_h + 1.0) - row_h * 1.5;
        scroll = scroll.vertical_scroll_offset(off.max(0.0));
    }

    // 行数据被「取出」后 tree 处于空状态，绘制期间只读取必要信息
    let mut clicked: Vec<Op> = Vec::new();
    let mut new_drag: Option<PathBuf> = None;
    let mut release: Option<(DragPayload, PathBuf)> = None;
    let mut ctx_menu_done = false;
    let mut rename_commit: Option<(PathBuf, String)> = None;
    let mut rename_changed: Option<String> = None;
    let mut rename_cancel = false;

    let out = scroll.show_rows(ui, row_h, rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for i in range {
            let row = &rows[i];
            let r = draw_row(
                ui,
                cfg,
                theme,
                st,
                row,
                row_h,
                &selected,
                open_doc.as_deref(),
                tree.is_searching(),
                tree.query.trim(),
            );
            if let Some(op) = r.click {
                clicked.push(op);
            }
            if r.drag_started {
                new_drag = Some(row.path.clone());
            }
            if let Some(payload) = r.hover_release {
                if row.is_dir && release.is_none() {
                    release = Some((payload, row.path.clone()));
                }
            }
            if r.context_open {
                tree.select(Some(row.path.clone()));
                ctx_menu_done = true;
            }
            if let Some(text) = r.rename_text {
                rename_changed = Some(text);
            }
            if r.rename_commit {
                rename_commit = Some((
                    st.rename
                        .as_ref()
                        .map(|x| x.path.clone())
                        .unwrap_or_default(),
                    st.rename
                        .as_ref()
                        .map(|x| x.buffer.clone())
                        .unwrap_or_default(),
                ));
            }
            if r.rename_cancel {
                rename_cancel = true;
            }
        }
    });

    // 空白处释放：移动到仓库根目录
    let empty_resp = ui.interact(
        out.inner_rect,
        egui::Id::new("easymd-tree-empty-drop"),
        Sense::hover(),
    );
    if let Some(payload) = empty_resp.dnd_release_payload::<DragPayload>() {
        if release.is_none() {
            release = Some(((*payload).clone(), tree.root.clone()));
        }
    }

    tree.put_rows(rows, cfg);

    if let Some(path) = new_drag {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let _ = path;
    }
    if let Some((payload, dest)) = release {
        for src in payload.paths {
            if src != dest {
                ops.push(Op::MoveInto { src, dest_dir: dest.clone() });
            }
        }
    }
    if ctx_menu_done {
        tree.invalidate();
    }
    if let Some(text) = rename_changed {
        if let Some(r) = st.rename.as_mut() {
            r.buffer = text;
        }
    }
    if let Some((path, name)) = rename_commit {
        ops.push(Op::CommitRename { path, name });
    }
    if rename_cancel {
        st.cancel_rename();
    }
    ops.extend(clicked);

    draw_drag_ghost(ui, theme);
}

/// 单行绘制结果。
#[derive(Default)]
struct RowResult {
    click: Option<Op>,
    drag_started: bool,
    hover_release: Option<DragPayload>,
    context_open: bool,
    rename_text: Option<String>,
    rename_commit: bool,
    rename_cancel: bool,
}

#[allow(clippy::too_many_arguments)]
fn draw_row(
    ui: &mut Ui,
    cfg: &AppConfig,
    theme: &Theme,
    st: &mut ExplorerState,
    row: &FlatRow,
    row_h: f32,
    selected: &Option<PathBuf>,
    open_doc: Option<&Path>,
    search_mode: bool,
    query: &str,
) -> RowResult {
    let mut res = RowResult::default();
    let full_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(full_w, row_h), Sense::click_and_drag());
    let is_selected = selected.as_deref() == Some(row.path.as_path());
    let is_open_doc = open_doc == Some(row.path.as_path()) && !row.is_dir;

    if is_open_doc {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme.subtle);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(2.5, rect.height())),
            CornerRadius::same(2),
            theme.accent.gamma_multiply(0.55),
        );
    }
    if is_selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme.selected);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(2.5, rect.height())),
            CornerRadius::same(2),
            theme.accent,
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme.hover.gamma_multiply(0.7));
    }

    // 拖放高亮
    if resp.dnd_hover_payload::<DragPayload>().is_some() && row.is_dir {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.5, theme.accent),
            egui::StrokeKind::Inside,
        );
    }

    let indent = 6.0 + row.depth as f32 * 14.0;
    let mut x = rect.left() + indent;

    if row.is_dir {
        let arrow_rect =
            Rect::from_center_size(egui::pos2(x + 6.0, rect.center().y), Vec2::new(13.0, 13.0));
        // 空目录的箭头淡化显示
        let arrow_color = if resp.hovered() {
            theme.strong
        } else if row.has_children {
            theme.weak
        } else {
            theme.weak.gamma_multiply(0.35)
        };
        paint_triangle(ui, arrow_rect, row.expanded, arrow_color);
        x += 15.0;
    } else {
        x += 3.0;
    }

    let icon = if row.is_dir {
        if row.expanded {
            "▣"
        } else {
            "▢"
        }
    } else if crate::util::is_markdown(&row.path) {
        "◆"
    } else {
        "◇"
    };
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        icon,
        row_font(cfg),
        if row.is_dir { theme.accent } else { theme.weak },
    );
    x += 17.0;

    // 内联重命名
    let editing = st.rename.as_ref().map(|r| r.path == row.path).unwrap_or(false);
    if editing {
        let width = (rect.right() - x - 6.0).max(60.0);
        let edit_rect = Rect::from_min_size(
            egui::pos2(x, rect.top() + 1.0),
            Vec2::new(width, rect.height() - 2.0),
        );
        let mut name = st.rename.as_ref().map(|r| r.buffer.clone()).unwrap_or_default();
        let id = egui::Id::new(("easymd-rename", &row.path));
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(edit_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                let te = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .id(id)
                        .desired_width(width)
                        .margin(egui::Margin::symmetric(4, 2)),
                );
                if st.rename.as_ref().map(|r| r.just_opened).unwrap_or(false) {
                    te.request_focus();
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                        let stem_len = Path::new(&name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().chars().count())
                            .unwrap_or_else(|| name.chars().count());
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(egui::text::CharIndex(0)),
                            egui::text::CCursor::new(egui::text::CharIndex(stem_len)),
                        )));
                        state.store(ui.ctx(), id);
                    }
                    if let Some(r) = st.rename.as_mut() {
                        r.just_opened = false;
                    }
                }
                if te.lost_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        res.rename_commit = true;
                    } else {
                        res.rename_cancel = true;
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    res.rename_cancel = true;
                }
            },
        );
        res.rename_text = Some(name);
        return res;
    }

    // 名称
    let text_right = rect.right() - 6.0;
    let hint = row.hint.as_deref();
    let hint_chars = hint
        .map(|h| h.chars().count().min(22))
        .unwrap_or(0) as f32;
    let hint_w = if hint.is_some() {
        (hint_chars * cfg.font_size * 0.48 + 10.0).min((text_right - x) * 0.5)
    } else {
        0.0
    };
    let name_w = (text_right - x - hint_w).max(20.0);
    let name_chars = (name_w / (cfg.font_size * 0.56)).max(4.0) as usize;
    let display = crate::util::elide(&row.name, name_chars);
    let color = if row.is_dir { theme.text } else { theme.text };
    let color = if !row.is_dir && !crate::util::is_markdown(&row.path) {
        theme.weak
    } else {
        color
    };
    let display = display;
    let font = row_font(cfg);
    if row.matched && !query.is_empty() {
        let ranges = crate::util::match_positions_ignore_case(&display, query, 8);
        let job = ui_ext::highlight_job(&display, &ranges, font.clone(), color, theme.selection, name_w);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(
            egui::pos2(x, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    } else {
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            &display,
            font,
            color,
        );
    }
    if let Some(h) = hint {
        ui.painter().text(
            egui::pos2(text_right, rect.center().y),
            Align2::RIGHT_CENTER,
            crate::util::elide(h, 24),
            FontId::proportional(cfg.font_size * 0.72),
            theme.weak.gamma_multiply(0.85),
        );
    }

    // ---------------------------------------------------------- 交互
    let path = row.path.clone();
    let is_dir = row.is_dir;
    let mut consumed_click = false;
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    resp.clone().dnd_set_drag_payload(DragPayload {
        paths: vec![path.clone()],
    });
    if resp.drag_started() {
        res.drag_started = true;
    }
    if let Some(payload) = resp.dnd_release_payload::<DragPayload>() {
        res.hover_release = Some((*payload).clone());
    }

    if resp.clicked() {
        consumed_click = true;
        if is_dir {
            let now = Instant::now();
            let dbl = st
                .last_dir_click
                .as_ref()
                .map(|(p, t)| p == &path && t.elapsed().as_millis() < 500)
                .unwrap_or(false);
            if dbl {
                st.last_dir_click = None;
                res.click = Some(Op::OpenFolderHere(path.clone()));
            } else {
                st.last_dir_click = Some((path.clone(), now));
                res.click = Some(Op::SelectAndToggle {
                    path: path.clone(),
                    focus: None,
                });
            }
        } else {
            res.click = Some(Op::OpenFile(path.clone()));
        }
    }
    if !consumed_click && resp.double_clicked() && search_mode && !is_dir {
        res.click = Some(Op::OpenFile(path.clone()));
    }

    if resp.secondary_clicked() {
        res.context_open = true;
    }
    res.context_open |= resp.secondary_clicked();

    resp.context_menu(|ui| {
        ui.set_min_width(170.0);
        if is_dir {
            if ui.button("在此新建文件").clicked() {
                res.click = Some(Op::NewFile(path.clone()));
                ui.close();
            }
            if ui.button("在此新建文件夹").clicked() {
                res.click = Some(Op::NewFolder(path.clone()));
                ui.close();
            }
            if ui.button("展开全部下级").clicked() {
                res.click = Some(Op::ExpandSubtree(path.clone()));
                ui.close();
            }
            ui.separator();
        } else if ui.button("打开").clicked() {
            res.click = Some(Op::OpenFile(path.clone()));
            ui.close();
        }
        if ui.button("重命名\tF2").clicked() {
            res.click = Some(Op::StartRename(path.clone()));
            ui.close();
        }
        if ui.button("删除\tDelete").clicked() {
            res.click = Some(Op::Delete(vec![path.clone()]));
            ui.close();
        }
        ui.separator();
        if ui.button("复制路径").clicked() {
            res.click = Some(Op::CopyToClipboard(path.display().to_string()));
            ui.close();
        }
        if ui.button("在文件管理器中显示").clicked() {
            res.click = Some(Op::RevealInOs(path.clone()));
            ui.close();
        }
        if !is_dir && ui.button("在目录树中定位").clicked() {
            res.click = Some(Op::RevealInSidebar(path.clone()));
            ui.close();
        }
    });

    // 悬停提示
    let mut tip = path.display().to_string();
    if let Some(e) = &row.error {
        tip = e.clone();
    } else if let Some(m) = row.modified {
        tip = format!(
            "{tip}\n修改时间：{}（{}）",
            crate::util::format_datetime(m),
            crate::util::format_relative(m)
        );
    }
    resp.on_hover_text(tip);
    res
}

fn paint_triangle(ui: &Ui, rect: Rect, expanded: bool, color: Color32) {
    let c = rect.center();
    let s = 3.2;
    let pts = if expanded {
        vec![
            egui::pos2(c.x - s, c.y - s * 0.5),
            egui::pos2(c.x + s, c.y - s * 0.5),
            egui::pos2(c.x, c.y + s * 0.8),
        ]
    } else {
        vec![
            egui::pos2(c.x - s * 0.5, c.y - s),
            egui::pos2(c.x - s * 0.5, c.y + s),
            egui::pos2(c.x + s * 0.8, c.y),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn draw_drag_ghost(ui: &Ui, theme: &Theme) {
    if let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ui.ctx()) {
        let Some(pos) = ui.ctx().pointer_hover_pos() else {
            return;
        };
        let name = payload
            .paths
            .first()
            .map(|p| crate::util::file_name(p))
            .unwrap_or_default();
        egui::Area::new(egui::Id::new("easymd-drag-ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + Vec2::new(14.0, 10.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theme.panel)
                    .stroke(Stroke::new(1.0, theme.accent))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(7, 3))
                    .show(ui, |ui| {
                        ui.label(RichText::new(crate::util::elide(&name, 30)).color(theme.text));
                    });
            });
    }
}

// ---------------------------------------------------------------- 大纲页签

fn outline_view(
    ui: &mut Ui,
    cfg: &AppConfig,
    theme: &Theme,
    headings: &[crate::doc::Heading],
    doc_path: Option<&Path>,
    st: &mut ExplorerState,
    ops: &mut Vec<Op>,
) {
    if doc_path.is_none() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("打开一篇文档后显示大纲").weak());
        });
        return;
    }
    if headings.is_empty() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("这篇文档还没有标题").weak());
        });
        return;
    }
    ui.add(
        egui::TextEdit::singleline(&mut st.outline_filter)
            .hint_text("过滤标题…")
            .desired_width(ui.available_width())
            .margin(egui::Margin::symmetric(7, 4)),
    );
    ui.add_space(4.0);

    let filter = st.outline_filter.trim().to_lowercase();
    let row_h = (cfg.font_size * 0.92 * cfg.line_height + 4.0).max(20.0);
    egui::ScrollArea::vertical()
        .id_salt("easymd-outline")
        .auto_shrink([false, false])
        .show_rows(ui, row_h, headings.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for i in range {
                let h = &headings[i];
                if !filter.is_empty() && !h.text.to_lowercase().contains(&filter) {
                    continue;
                }
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(5),
                        theme.hover.gamma_multiply(0.7),
                    );
                }
                let indent = 4.0 + (h.level.saturating_sub(1)) as f32 * 12.0;
                let color = match h.level {
                    1 => theme.strong,
                    2 => theme.text,
                    _ => theme.weak,
                };
                let font = if h.level <= 2 {
                    FontId::proportional(cfg.font_size * 0.95)
                } else {
                    FontId::proportional(cfg.font_size * 0.88)
                };
                let text = if h.text.is_empty() {
                    "(未命名标题)".to_string()
                } else {
                    h.text.clone()
                };
                let max_chars =
                    ((rect.width() - indent - 8.0) / (cfg.font_size * 0.56)).max(4.0) as usize;
                ui.painter().text(
                    egui::pos2(rect.left() + indent, rect.center().y),
                    Align2::LEFT_CENTER,
                    crate::util::elide(&text, max_chars),
                    font,
                    color,
                );
                if resp.clicked() {
                    ops.push(Op::OutlineJump(h.slug.clone()));
                }
                if let Some(path) = doc_path {
                    resp.context_menu(|ui| {
                        ui.set_min_width(170.0);
                        if ui.button("跳转到该标题").clicked() {
                            ops.push(Op::OutlineJump(h.slug.clone()));
                            ui.close();
                        }
                        if ui.button("在编辑模式中定位").clicked() {
                            ops.push(Op::OpenAtLine(path.to_path_buf(), h.line));
                            ui.close();
                        }
                    });
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
}

// ---------------------------------------------------------------- 搜索页签

fn search_view(
    ui: &mut Ui,
    cfg: &AppConfig,
    theme: &Theme,
    search: &mut SearchState,
    st: &mut ExplorerState,
    ops: &mut Vec<Op>,
) {
    let mut start = false;
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut st.search_input)
                .hint_text("搜索全文内容，回车开始")
                .desired_width(ui.available_width() - 62.0)
                .margin(egui::Margin::symmetric(7, 4)),
        );
        if st.focus_search {
            resp.request_focus();
            st.focus_search = false;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            start = true;
        }
        if ui.button("搜索").clicked() {
            start = true;
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut st.search_case, "区分大小写");
        if search.is_running() && ui_ext::tool_button(ui, "停止", "停止搜索", theme).clicked() {
            search.cancel();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let msg = search.progress.message.clone().unwrap_or_default();
            let text = if search.is_running() {
                format!("检索中… 已扫描 {} 个文件", search.progress.scanned)
            } else if !msg.is_empty() {
                msg
            } else {
                format!("{} 处结果", search.hits.len())
            };
            ui.label(RichText::new(text).weak().small());
        });
    });
    if search.is_running() {
        let frac = (search.progress.scanned as f32 / 400.0).min(0.95);
        ui_ext::thin_progress(ui, frac, theme);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    }
    ui.add_space(4.0);

    if start {
        ops.push(Op::StartFullTextSearch(
            st.search_input.clone(),
            st.search_case,
        ));
    }

    if search.hits.is_empty() {
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            if search.progress.finished && !search.query.is_empty() {
                ui.label(RichText::new("没有找到匹配内容").weak());
            } else {
                ui.label(RichText::new("在整个仓库中搜索文本内容").weak().small());
            }
        });
        return;
    }

    let row_h = (cfg.font_size * 0.82 * cfg.line_height + 20.0).max(36.0);
    let mut clicked: Option<(PathBuf, usize)> = None;
    let mut new_selected: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("easymd-search-results")
        .auto_shrink([false, false])
        .show_rows(ui, row_h, search.hits.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for i in range {
                let hit_path = search.hits[i].path.clone();
                let hit_line = search.hits[i].line;
                let preview = search.hits[i].preview.clone();
                let hit_range = search.hits[i].hit_range;
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                let is_sel = search.selected == i;
                if resp.hovered() || is_sel {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::same(5),
                        if is_sel {
                            theme.selected
                        } else {
                            theme.hover.gamma_multiply(0.7)
                        },
                    );
                }
                let name = crate::util::file_name(&hit_path);
                ui.painter().text(
                    egui::pos2(rect.left() + 6.0, rect.top() + row_h * 0.32),
                    Align2::LEFT_CENTER,
                    crate::util::elide(&format!("{}  ·  第 {} 行", name, hit_line + 1), 64),
                    FontId::proportional(cfg.font_size * 0.85),
                    theme.strong,
                );
                let shown = crate::util::elide(&preview, 76);
                let ranges = scale_ranges(&hit_range, &preview, &shown);
                let job = ui_ext::highlight_job(
                    &shown,
                    &ranges,
                    FontId::proportional(cfg.font_size * 0.8),
                    theme.weak,
                    theme.selection,
                    rect.width() - 8.0,
                );
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                ui.painter().galley(
                    egui::pos2(rect.left() + 6.0, rect.top() + row_h * 0.60),
                    galley,
                    theme.weak,
                );
                if resp.clicked() {
                    new_selected = Some(i);
                    clicked = Some((hit_path, hit_line));
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
    if let Some(i) = new_selected {
        search.selected = i;
    }
    if let Some((p, l)) = clicked {
        ops.push(Op::OpenSearchHit(p, l));
    }
}

/// 预览被截断后重新计算高亮范围。
fn scale_ranges(orig: &(usize, usize), full: &str, shown: &str) -> Vec<(usize, usize)> {
    if shown.chars().count() < full.chars().count() {
        let end = shown.chars().count().saturating_sub(1);
        let s = orig.0.min(end);
        let e = orig.1.min(end);
        if s < e {
            vec![(s, e)]
        } else {
            Vec::new()
        }
    } else {
        vec![*orig]
    }
}

/// 键盘导航：在目录树中移动选择、展开折叠、打开文件。
/// `keys` 由主循环在读取键盘输入后传入（仅在目录树页签且输入框未聚焦时）。
pub fn keyboard_nav(
    cfg: &AppConfig,
    tree: &mut Tree,
    st: &mut ExplorerState,
    keys: NavKeys,
    ops: &mut Vec<Op>,
) {
    if keys == NavKeys::NONE {
        return;
    }
    let rows = tree.take_rows(cfg);
    if rows.is_empty() {
        tree.put_rows(rows, cfg);
        return;
    }
    let mut idx = rows
        .iter()
        .position(|r| Some(r.path.as_path()) == tree.selected.as_deref())
        .unwrap_or_else(|| st.focus_row.min(rows.len() - 1));
    let mut changed = false;

    if keys.down {
        idx = (idx + 1).min(rows.len() - 1);
        changed = true;
    } else if keys.up {
        idx = idx.saturating_sub(1);
        changed = true;
    } else if keys.right {
        let row = &rows[idx];
        if row.is_dir && !row.expanded {
            ops.push(Op::SetExpand {
                path: row.path.clone(),
                expanded: true,
            });
            st.scroll_to_row = Some(idx);
            changed = true;
        }
    } else if keys.left {
        let row = &rows[idx];
        if row.is_dir && row.expanded {
            ops.push(Op::SetExpand {
                path: row.path.clone(),
                expanded: false,
            });
            st.scroll_to_row = Some(idx);
            changed = true;
        } else {
            // 收起上级目录并把焦点移到父目录
            if let Some(parent) = row.path.parent() {
                if let Some(pi) = rows.iter().position(|r| r.path == parent) {
                    idx = pi;
                    changed = true;
                    if rows[pi].is_dir && rows[pi].expanded {
                        ops.push(Op::SetExpand {
                            path: parent.to_path_buf(),
                            expanded: false,
                        });
                        st.scroll_to_row = Some(pi);
                    }
                }
            }
        }
    } else if keys.space || keys.enter {
        let row = &rows[idx];
        if row.is_dir {
            ops.push(Op::SetExpand {
                path: row.path.clone(),
                expanded: !row.expanded,
            });
            st.scroll_to_row = Some(idx);
        } else {
            ops.push(Op::OpenFile(row.path.clone()));
        }
        changed = true;
    }

    if changed {
        st.focus_row = idx;
        tree.select(Some(rows[idx].path.clone()));
        if keys.down || keys.up {
            st.scroll_to_row = Some(idx);
        }
        tree.invalidate();
    }
    tree.put_rows(rows, cfg);
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct NavKeys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub space: bool,
}

impl NavKeys {
    pub const NONE: Self = Self {
        up: false,
        down: false,
        left: false,
        right: false,
        enter: false,
        space: false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    /// 新建文件 / 文件夹的目标目录：未选中或选中根目录时都要落在仓库根目录
    #[test]
    fn target_dir_falls_back_to_root() {
        let dir = std::env::temp_dir().join(format!("easymd-target-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        let cfg = AppConfig::default();
        let mut tree = Tree::new(dir.clone(), &cfg, &[]);

        // 没有任何选中项 -> 根目录
        assert_eq!(target_dir(&tree), dir);

        // 选中根目录（点第一行）-> 仍为根目录，这样展开子目录后也能回到根目录新建
        tree.select(Some(dir.clone()));
        assert_eq!(target_dir(&tree), dir);

        // 选中子目录 -> 该子目录
        tree.select(Some(sub.clone()));
        assert_eq!(target_dir(&tree), sub);

        // 选中文件 -> 文件所在目录
        tree.select(Some(dir.join("a.md")));
        assert_eq!(target_dir(&tree), dir);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
