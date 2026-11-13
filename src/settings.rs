// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 设置面板：外观 / 字体与排版 / 阅读 / 文件与目录。

use egui::{Context, RichText, Ui};

use crate::config::{AppConfig, ImageFit, SortBy};
use crate::fonts::FamilyInfo;
use crate::theme::{Theme, ThemeId};
use crate::ui_ext;

#[derive(Default, Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub theme: Option<ThemeId>,
    pub font_size: f32,
    pub line_height: f32,
    pub ui_density: f32,
    pub font_size_editor: f32,
    pub custom_bg: Option<String>,
    pub custom_text: Option<String>,
    pub custom_accent: Option<String>,
    pub font_prop: Option<String>,
    pub font_mono: Option<String>,
    pub bg_image: Option<std::path::PathBuf>,
}

impl SettingsSnapshot {
    pub fn of(cfg: &AppConfig) -> Self {
        Self {
            theme: Some(cfg.theme),
            font_size: cfg.font_size,
            line_height: cfg.line_height,
            ui_density: cfg.ui_density,
            font_size_editor: cfg.font_size_editor,
            custom_bg: cfg.custom_bg.clone(),
            custom_text: cfg.custom_text.clone(),
            custom_accent: cfg.custom_accent.clone(),
            font_prop: cfg.font_prop.clone(),
            font_mono: cfg.font_mono.clone(),
            bg_image: cfg.bg_image.clone(),
        }
    }
}

#[derive(Default)]
pub struct SettingsChange {
    /// 需要重新应用配色/间距
    pub restyle: bool,
    /// 需要重新安装字体
    pub fonts: bool,
    /// 需要重新载入背景图片
    pub bg_image: bool,
    pub close: bool,
}

pub fn show(
    ctx: &Context,
    cfg: &mut AppConfig,
    open: &mut bool,
    theme: &Theme,
    families: &[FamilyInfo],
    families_ready: bool,
    snap: &mut SettingsSnapshot,
    bg_error: Option<&str>,
) -> SettingsChange {
    let mut change = SettingsChange::default();
    if !*open {
        return change;
    }
    let mut is_open = true;
    egui::Window::new("设置")
        .open(&mut is_open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(620.0)
        .min_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(
            egui::Frame::new()
                .fill(theme.panel)
                .inner_margin(egui::Margin::same(14)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // ------------------------------------------------ 外观
                    ui_ext::section_title(ui, "主题", theme);
                    ui.horizontal_wrapped(|ui| {
                        for id in ThemeId::all() {
                            let t = Theme::from_id(id);
                            let selected = cfg.theme == id && cfg.custom_bg.is_none();
                            let resp = ui_ext::color_swatch(ui, t.bg, selected, theme);
                            if resp.clicked() {
                                cfg.theme = id;
                                cfg.custom_bg = None;
                                cfg.custom_text = None;
                            }
                            resp.on_hover_text(format!(
                                "{}（{}）",
                                id.label(),
                                if id.is_dark() { "深色" } else { "浅色" }
                            ));
                        }
                        ui.add_space(8.0);
                        if ui.button("跟随明暗自动").on_hover_text("浅色 ⇄ 深色 快速切换 (Ctrl+D)").clicked() {
                            cfg.theme = cfg.theme.next_light_dark();
                            cfg.custom_bg = None;
                        }
                    });

                    ui.add_space(6.0);
                    ui_ext::setting_row(ui, "阅读区背景", Some("留空使用主题默认"), |ui| {
                        let mut hex = cfg.custom_bg.clone().unwrap_or_default();
                        if color_field(ui, &mut hex, theme) {
                            cfg.custom_bg = if hex.trim().is_empty() { None } else { Some(hex) };
                        }
                    });
                    ui_ext::setting_row(ui, "正文颜色", Some("留空使用主题默认"), |ui| {
                        let mut hex = cfg.custom_text.clone().unwrap_or_default();
                        if color_field(ui, &mut hex, theme) {
                            cfg.custom_text = if hex.trim().is_empty() { None } else { Some(hex) };
                        }
                    });
                    ui_ext::setting_row(ui, "强调色", Some("标题、选中态与链接"), |ui| {
                        let mut hex = cfg.custom_accent.clone().unwrap_or_default();
                        if color_field(ui, &mut hex, theme) {
                            cfg.custom_accent = if hex.trim().is_empty() { None } else { Some(hex) };
                        }
                    });

                    // ------------------------------------------------ 背景图片
                    ui_ext::section_title(ui, "背景图片", theme);
                    ui_ext::setting_row(ui, "图片文件", None, |ui| {
                        ui.horizontal(|ui| {
                            let label = cfg
                                .bg_image
                                .as_ref()
                                .map(|p| crate::util::file_name(p))
                                .unwrap_or_else(|| "未设置".into());
                            ui.label(RichText::new(crate::util::elide(&label, 28)).color(theme.text));
                            if ui.button("选择…").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
                                    .pick_file()
                                {
                                    cfg.bg_image = Some(p);
                                }
                            }
                            if cfg.bg_image.is_some() && ui.button("清除").clicked() {
                                cfg.bg_image = None;
                            }
                        });
                    });
                    if let Some(err) = bg_error {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("无法载入背景图片：{err}")).color(theme.danger).small());
                        });
                    }
                    ui.add_enabled_ui(cfg.bg_image.is_some(), |ui| {
                        ui_ext::setting_row(ui, "不透明度", None, |ui| {
                            ui.add(
                                egui::Slider::new(&mut cfg.bg_image_opacity, 0.05..=1.0)
                                    .show_value(true)
                                    .fixed_decimals(2),
                            );
                        });
                        ui_ext::setting_row(ui, "显示方式", None, |ui| {
                            egui::ComboBox::from_id_salt("bgfit")
                                .selected_text(cfg.bg_image_fit.label())
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for f in ImageFit::all() {
                                        ui.selectable_value(&mut cfg.bg_image_fit, f, f.label());
                                    }
                                });
                        });
                        ui_ext::setting_row(ui, "文字蒙版", Some("提高复杂图片上的可读性"), |ui| {
                            ui.add(
                                egui::Slider::new(&mut cfg.bg_image_dim, 0.0..=0.85)
                                    .show_value(true)
                                    .fixed_decimals(2),
                            );
                        });
                    });

                    // ------------------------------------------------ 字体与排版
                    ui_ext::section_title(ui, "字体与排版", theme);
                    if !families_ready {
                        ui.label(RichText::new("正在扫描系统字体…").weak().small());
                    }
                    ui_ext::setting_row(ui, "正文字体", None, |ui| {
                        font_combo(ui, "prop", cfg.font_prop.clone(), families, false, theme, |v| {
                            cfg.font_prop = v;
                        });
                    });
                    ui_ext::setting_row(ui, "等宽字体", Some("代码与编辑区"), |ui| {
                        font_combo(ui, "mono", cfg.font_mono.clone(), families, true, theme, |v| {
                            cfg.font_mono = v;
                        });
                    });
                    ui_ext::setting_row(ui, "正文字号", None, |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.font_size, 12.0..=34.0)
                                .suffix(" px")
                                .fixed_decimals(0),
                        );
                    });
                    ui_ext::setting_row(ui, "编辑区字号", None, |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.font_size_editor, 11.0..=30.0)
                                .suffix(" px")
                                .fixed_decimals(0),
                        );
                    });
                    ui_ext::setting_row(ui, "行距", Some("影响阅读与编辑的舒适度"), |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.line_height, 1.1..=2.0)
                                .fixed_decimals(2)
                                .suffix(" 倍"),
                        );
                    });
                    ui_ext::setting_row(ui, "阅读栏宽", Some("超过该宽度自动留白（0 = 自适应）"), |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.content_width, 0.0..=2000.0)
                                .suffix(" px")
                                .fixed_decimals(0),
                        );
                    });
                    if cfg.content_width < 100.0 {
                        cfg.content_width = 100.0;
                    }
                    ui_ext::setting_row(ui, "界面紧凑度", None, |ui| {
                        ui.add(
                            egui::Slider::new(&mut cfg.ui_density, 0.8..=1.3)
                                .fixed_decimals(2),
                        );
                    });

                    // ------------------------------------------------ 阅读
                    ui_ext::section_title(ui, "阅读", theme);
                    ui.checkbox(&mut cfg.restore_scroll, "切换回文档时恢复阅读位置");
                    ui.checkbox(&mut cfg.autosave, "自动保存（切换文档、退出时）");
                    ui.checkbox(
                        &mut cfg.sync_scrolling,
                        "编辑模式下左右两栏联动（滚动同步 + 高亮光标所在块）",
                    );
                    ui.checkbox(&mut cfg.watch_files, "监听文件变化并自动刷新");
                    ui.checkbox(&mut cfg.expand_on_open, "打开文件夹时展开全部目录");

                    // ------------------------------------------------ 文件与目录
                    ui_ext::section_title(ui, "文件与目录", theme);
                    ui.checkbox(&mut cfg.show_hidden, "显示隐藏文件");
                    ui.checkbox(&mut cfg.show_non_md, "显示非 Markdown 文件");
                    ui.checkbox(&mut cfg.dirs_first, "文件夹排在文件前面");
                    ui.horizontal(|ui| {
                        ui.label("排序方式");
                        egui::ComboBox::from_id_salt("sortby")
                            .selected_text(match cfg.sort_by {
                                SortBy::Name => "按名称",
                                SortBy::Modified => "按修改时间",
                            })
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut cfg.sort_by, SortBy::Name, "按名称");
                                ui.selectable_value(
                                    &mut cfg.sort_by,
                                    SortBy::Modified,
                                    "按修改时间",
                                );
                            });
                        ui.checkbox(&mut cfg.sort_desc, "倒序");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Tab 宽度");
                        ui.add(egui::Slider::new(&mut cfg.tab_spaces, 1..=8).fixed_decimals(0));
                    });
                    ui.checkbox(&mut cfg.soft_wrap, "编辑区自动换行");
                    ui.checkbox(&mut cfg.confirm_delete, "删除前确认（强烈建议开启）");

                    ui.add_space(12.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("恢复默认外观").clicked() {
                            cfg.theme = ThemeId::Paper;
                            cfg.custom_bg = None;
                            cfg.custom_text = None;
                            cfg.custom_accent = None;
                            cfg.font_size = 17.0;
                            cfg.line_height = 1.55;
                            cfg.content_width = 880.0;
                            cfg.ui_density = 1.0;
                            cfg.font_size_editor = 15.0;
                        }
                        if ui.button("打开配置文件所在目录").clicked() {
                            let p = AppConfig::config_path();
                            if let Some(dir) = p.parent() {
                                crate::fs_ops::reveal_in_file_manager(dir);
                            }
                        }
                    });
                    ui.add_space(6.0);
                });
        });
    if !is_open {
        change.close = true;
        *open = false;
    }

    // 变更检测
    let now = SettingsSnapshot::of(cfg);
    if snap.theme.is_none() {
        *snap = now;
        return change;
    }
    change.restyle = snap.theme != now.theme
        || snap.custom_bg != now.custom_bg
        || snap.custom_text != now.custom_text
        || snap.custom_accent != now.custom_accent
        || (snap.ui_density - now.ui_density).abs() > f32::EPSILON;
    change.fonts = snap.font_prop != now.font_prop
        || snap.font_mono != now.font_mono
        || (snap.font_size - now.font_size).abs() > f32::EPSILON
        || (snap.line_height - now.line_height).abs() > 0.001
        || (snap.font_size_editor - now.font_size_editor).abs() > f32::EPSILON;
    change.bg_image = snap.bg_image != now.bg_image;
    *snap = now;
    change
}

fn color_field(ui: &mut Ui, hex: &mut String, theme: &Theme) -> bool {
    let before = hex.clone();
    ui.horizontal(|ui| {
        let mut color = crate::util::hex_to_color(hex)
            .unwrap_or(theme.bg);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *hex = crate::util::color_to_hex(color);
        }
        ui.add(
            egui::TextEdit::singleline(hex)
                .desired_width(96.0)
                .hint_text("#RRGGBB")
                .margin(egui::Margin::symmetric(6, 3)),
        );
    });
    *hex != before
}

fn font_combo(
    ui: &mut Ui,
    id: &str,
    current: Option<String>,
    families: &[FamilyInfo],
    mono: bool,
    theme: &Theme,
    mut set: impl FnMut(Option<String>),
) {
    let label = current.clone().unwrap_or_else(|| "系统默认".into());
    let mut filter_state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(egui::Id::new(("fontfilter", id))))
        .unwrap_or_default();
    egui::ComboBox::from_id_salt(("fontcombo", id))
        .selected_text(RichText::new(crate::util::elide(&label, 26)).color(theme.text))
        .width(220.0)
        .show_ui(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut filter_state)
                    .hint_text("筛选字体…")
                    .desired_width(200.0),
            );
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    if ui.selectable_label(current.is_none(), "系统默认").clicked() {
                        set(None);
                        ui.close();
                    }
                    let f = filter_state.to_lowercase();
                    for fam in families
                        .iter()
                        .filter(|f2| mono == f2.mono || !mono)
                        .filter(|f2| {
                            f.is_empty() || f2.name.to_lowercase().contains(&f)
                        })
                        .take(400)
                    {
                        let selected = current.as_deref() == Some(fam.name.as_str());
                        let text = if fam.cjk {
                            format!("{}  · 中文", fam.name)
                        } else {
                            fam.name.clone()
                        };
                        if ui.selectable_label(selected, text).clicked() {
                            set(Some(fam.name.clone()));
                            ui.close();
                        }
                    }
                });
        });
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new(("fontfilter", id)), filter_state));
}
