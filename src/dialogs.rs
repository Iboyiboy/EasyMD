// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 对话框：新建、重命名、删除确认、消息提示、关于、快捷键说明。

use std::path::PathBuf;

use egui::{Context, RichText};

use crate::theme::Theme;

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    Delete(Vec<PathBuf>),
    SaveAndContinue,
    CloseWithoutSaving,
    OpenFolderFromMissing,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    ConfirmDelete {
        paths: Vec<PathBuf>,
    },
    NewFile {
        parent: PathBuf,
        name: String,
        error: Option<String>,
    },
    NewFolder {
        parent: PathBuf,
        name: String,
        error: Option<String>,
    },
    Message {
        title: String,
        text: String,
        is_error: bool,
    },
    About,
    Shortcuts,
    MissingFolder {
        path: PathBuf,
    },
    /// 关闭窗口时存在未保存修改
    CloseUnsaved,
}

#[derive(Debug, Clone)]
pub enum DialogOutcome {
    NewFile { parent: PathBuf, name: String },
    NewFolder { parent: PathBuf, name: String },
    Confirm(ConfirmAction),
    Dismissed,
}

#[derive(Default)]
pub struct Dialogs {
    pub current: Option<Dialog>,
}

impl Dialogs {
    pub fn open(&mut self, d: Dialog) {
        self.current = Some(d);
    }

    pub fn is_open(&self) -> bool {
        self.current.is_some()
    }

    pub fn message(&mut self, title: impl Into<String>, text: impl Into<String>, is_error: bool) {
        self.open(Dialog::Message {
            title: title.into(),
            text: text.into(),
            is_error,
        });
    }

    pub fn confirm_delete(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.open(Dialog::ConfirmDelete { paths });
    }

    /// 渲染当前对话框。
    pub fn show(&mut self, ctx: &Context, theme: &Theme) -> Option<DialogOutcome> {
        let mut dialog = self.current.take()?;
        let mut outcome: Option<DialogOutcome> = None;
        let mut keep_open = true;
        let id = egui::Id::new("easymd-dialog");
        let modal = egui::Modal::new(id)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .frame(
                egui::Frame::new()
                    .fill(theme.panel)
                    .stroke(egui::Stroke::new(1.0, theme.border))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(18))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 28,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(90),
                    }),
            );

        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));

        modal.show(ctx, |ui| {
            ui.set_width(if matches!(dialog, Dialog::Shortcuts) { 460.0 } else { 400.0 });
            match &mut dialog {
                Dialog::ConfirmDelete { paths } => {
                    ui.label(RichText::new("确认删除").strong().size(18.0).color(theme.danger));
                    ui.add_space(6.0);
                    ui.label(format!("以下 {} 个项目将被永久删除，无法撤销：", paths.len()));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for p in paths.iter().take(200) {
                                let is_dir = p.is_dir();
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        if is_dir { "📁" } else { "📄" },
                                        p.display()
                                    ))
                                    .color(theme.text)
                                    .small(),
                                );
                            }
                            if paths.len() > 200 {
                                ui.label(RichText::new("…").weak());
                            }
                        });
                    ui.add_space(10.0);
                    if paths.iter().any(|p| p.is_dir()) {
                        ui.label(
                            RichText::new("注意：其中包含文件夹，文件夹内的全部内容都会被删除。")
                                .color(theme.danger)
                                .small(),
                        );
                        ui.add_space(6.0);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("删除").color(egui::Color32::WHITE))
                                    .fill(theme.danger)
                                    .min_size(egui::Vec2::new(88.0, 28.0)),
                            )
                            .clicked()
                        {
                            outcome = Some(DialogOutcome::Confirm(ConfirmAction::Delete(
                                paths.clone(),
                            )));
                            keep_open = false;
                        }
                        if ui
                            .add(egui::Button::new("取消").min_size(egui::Vec2::new(88.0, 28.0)))
                            .clicked()
                        {
                            outcome = Some(DialogOutcome::Dismissed);
                            keep_open = false;
                        }
                    });
                }
                Dialog::NewFile {
                    parent,
                    name,
                    error,
                } => {
                    let title = format!("新建文件 · {}", short_dir(parent));
                    let (ok, cancel) = name_prompt(ui, theme, &title, "文件名", name, error, "例如：笔记.md");
                    if ok {
                        outcome = Some(DialogOutcome::NewFile {
                            parent: parent.clone(),
                            name: name.clone(),
                        });
                        keep_open = false;
                    }
                    if cancel {
                        outcome = Some(DialogOutcome::Dismissed);
                        keep_open = false;
                    }
                }
                Dialog::NewFolder {
                    parent,
                    name,
                    error,
                } => {
                    let title = format!("新建文件夹 · {}", short_dir(parent));
                    let (ok, cancel) = name_prompt(ui, theme, &title, "文件夹名", name, error, "例如：学习笔记");
                    if ok {
                        outcome = Some(DialogOutcome::NewFolder {
                            parent: parent.clone(),
                            name: name.clone(),
                        });
                        keep_open = false;
                    }
                    if cancel {
                        outcome = Some(DialogOutcome::Dismissed);
                        keep_open = false;
                    }
                }
                Dialog::Message {
                    title,
                    text,
                    is_error,
                } => {
                    ui.label(
                        RichText::new(title.as_str())
                            .strong()
                            .size(17.0)
                            .color(if *is_error { theme.danger } else { theme.strong }),
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new(text.as_str()).color(theme.text));
                    ui.add_space(14.0);
                    if ui
                        .add(egui::Button::new("知道了").min_size(egui::Vec2::new(88.0, 28.0)))
                        .clicked()
                    {
                        outcome = Some(DialogOutcome::Dismissed);
                        keep_open = false;
                    }
                }
                Dialog::CloseUnsaved => {
                    ui.label(RichText::new("有未保存的修改").strong().size(17.0));
                    ui.add_space(8.0);
                    ui.label("关闭窗口前是否保存当前文档？");
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new("保存并关闭")
                                    .min_size(egui::Vec2::new(104.0, 28.0)),
                            )
                            .clicked()
                        {
                            outcome = Some(DialogOutcome::Confirm(ConfirmAction::SaveAndContinue));
                            keep_open = false;
                        }
                        if ui
                            .add(egui::Button::new("不保存").min_size(egui::Vec2::new(88.0, 28.0)))
                            .clicked()
                        {
                            outcome =
                                Some(DialogOutcome::Confirm(ConfirmAction::CloseWithoutSaving));
                            keep_open = false;
                        }
                        if ui
                            .add(egui::Button::new("取消").min_size(egui::Vec2::new(72.0, 28.0)))
                            .clicked()
                        {
                            outcome = Some(DialogOutcome::Dismissed);
                            keep_open = false;
                        }
                    });
                }
                Dialog::MissingFolder { path } => {
                    ui.label(RichText::new("上次打开的文件夹已不存在").strong().size(17.0));
                    ui.add_space(8.0);
                    ui.label(RichText::new(path.display().to_string()).weak());
                    ui.add_space(10.0);
                    ui.label("请重新选择一个文档仓库文件夹。");
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("选择文件夹").min_size(egui::Vec2::new(104.0, 28.0)))
                            .clicked()
                        {
                            outcome = Some(DialogOutcome::Confirm(ConfirmAction::OpenFolderFromMissing));
                            keep_open = false;
                        }
                        if ui
                            .add(egui::Button::new("取消").min_size(egui::Vec2::new(88.0, 28.0)))
                            .clicked()
                        {
                            outcome = Some(DialogOutcome::Dismissed);
                            keep_open = false;
                        }
                    });
                }
                Dialog::About => {
                    ui.label(RichText::new("EasyMD").strong().size(20.0).color(theme.accent));
                    ui.label(
                        RichText::new(format!("版本 {}", env!("CARGO_PKG_VERSION")))
                            .weak()
                            .small(),
                    );
                    ui.add_space(10.0);
                    ui.label("超轻量化的本地 Markdown 知识库阅读与管理工具。");
                    ui.add_space(6.0);
                    ui.label(RichText::new("纯 Rust + egui 实现，启动快、内存占用低、跨平台（Windows / Linux）。").small());
                    ui.add_space(10.0);
                    ui.label(RichText::new("配置文件：").weak().small());
                    ui.label(
                        RichText::new(crate::config::AppConfig::config_path().display().to_string())
                            .small(),
                    );
                    ui.add_space(14.0);
                    if ui
                        .add(egui::Button::new("关闭").min_size(egui::Vec2::new(88.0, 28.0)))
                        .clicked()
                    {
                        outcome = Some(DialogOutcome::Dismissed);
                        keep_open = false;
                    }
                }
                Dialog::Shortcuts => {
                    ui.label(RichText::new("快捷键").strong().size(17.0));
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for (k, v) in SHORTCUTS {
                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::Vec2::new(180.0, 20.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(*k)
                                                    .monospace()
                                                    .color(theme.accent),
                                            );
                                        },
                                    );
                                    ui.label(RichText::new(*v).color(theme.text));
                                });
                            }
                        });
                    ui.add_space(12.0);
                    if ui
                        .add(egui::Button::new("关闭").min_size(egui::Vec2::new(88.0, 28.0)))
                        .clicked()
                    {
                        outcome = Some(DialogOutcome::Dismissed);
                        keep_open = false;
                    }
                }
            }
        });

        if esc || (enter && !matches!(dialog, Dialog::ConfirmDelete { .. })) {
            // Esc 一律取消；Enter 在输入类对话框中提交（由输入框自行处理部分情况）
            if esc {
                outcome = Some(DialogOutcome::Dismissed);
                keep_open = false;
            }
        }

        if keep_open {
            self.current = Some(dialog);
        }
        outcome
    }
}

fn short_dir(p: &std::path::Path) -> String {
    crate::util::elide_middle(&p.display().to_string(), 40)
}

/// 单行输入对话框（返回 (确认, 取消)）。
fn name_prompt(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    label: &str,
    name: &mut String,
    error: &Option<String>,
    placeholder: &str,
) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.label(RichText::new(title).strong().size(16.0));
    ui.add_space(10.0);
    ui.label(RichText::new(label).weak().small());
    ui.add_space(3.0);
    let resp = ui.add(
        egui::TextEdit::singleline(name)
            .desired_width(f32::INFINITY)
            .hint_text(placeholder)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    // 自动全选，方便直接改名
    if !ui.memory(|m| m.has_focus(resp.id)) && error.is_none() {
        resp.request_focus();
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(egui::text::CharIndex(0)),
                egui::text::CCursor::new(egui::text::CharIndex(name.chars().count())),
            )));
            state.store(ui.ctx(), resp.id);
        }
    }
    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        ok = true;
    }
    if let Some(err) = error {
        ui.add_space(4.0);
        ui.label(RichText::new(err).color(theme.danger).small());
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new("确定").min_size(egui::Vec2::new(88.0, 28.0)))
            .clicked()
        {
            ok = true;
        }
        if ui
            .add(egui::Button::new("取消").min_size(egui::Vec2::new(88.0, 28.0)))
            .clicked()
        {
            cancel = true;
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Enter 确定 · Esc 取消").weak().small());
    });
    (ok, cancel)
}

pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+O", "打开文档仓库（文件夹）"),
    ("Ctrl+Shift+O", "打开单个 Markdown 文件"),
    ("Ctrl+S", "保存当前文档"),
    ("Ctrl+N", "在选中位置新建文件"),
    ("Ctrl+Shift+N", "在选中位置新建文件夹"),
    ("F2", "重命名选中项"),
    ("Delete", "删除选中项（需确认）"),
    ("F5", "刷新目录树"),
    ("Ctrl+F", "搜索文件名 / 切到搜索面板"),
    ("Ctrl+Shift+F", "仓库全文检索"),
    ("Ctrl+B", "显示 / 隐藏左侧栏"),
    ("Ctrl+R", "阅读模式（沉浸阅读全文）"),
    ("Ctrl+E", "编辑模式（左源码 + 右实时预览，可拖动中间分隔条）"),
    ("Ctrl+D", "切换深色 / 浅色主题"),
    ("Ctrl+= / Ctrl+-", "放大 / 缩小字体"),
    ("Ctrl+0", "重置字体大小"),
    ("Ctrl+W", "关闭当前文档"),
    ("Alt+← / Alt+→", "后退 / 前进（浏览历史）"),
    ("Ctrl+滚轮", "缩放字体"),
    ("↑ / ↓", "在目录树中上下移动选择"),
    ("← / →", "折叠 / 展开文件夹"),
    ("Enter", "打开选中文件"),
    ("Space", "展开 / 折叠文件夹"),
    ("Esc", "关闭对话框 / 退出搜索"),
    ("F11", "禅模式（专注阅读）"),
    ("Ctrl+/", "显示本快捷键列表"),
];
