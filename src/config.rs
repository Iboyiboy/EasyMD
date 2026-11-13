// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 配置持久化：记住上次打开的文件夹、视图模式、主题、字体、缩放、滚动位置等。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::ThemeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// 阅读模式（默认）
    Read,
    /// 编辑模式：左侧源码编辑 + 右侧实时预览（旧配置里的 split 也会映射到这里）
    #[serde(alias = "split")]
    Edit,
}

impl Default for ViewMode {
    fn default() -> Self {
        Self::Read
    }
}

impl ViewMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "阅读",
            Self::Edit => "编辑",
        }
    }
    pub fn all() -> [Self; 2] {
        [Self::Read, Self::Edit]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Name,
    Modified,
}

impl Default for SortBy {
    fn default() -> Self {
        Self::Name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarTab {
    Files,
    Outline,
    Search,
}

impl Default for SidebarTab {
    fn default() -> Self {
        Self::Files
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// 铺满（可能裁切）
    Cover,
    /// 完整显示
    Contain,
    /// 平铺
    Tile,
    /// 原尺寸居中
    Center,
}

impl Default for ImageFit {
    fn default() -> Self {
        Self::Cover
    }
}

impl ImageFit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cover => "铺满",
            Self::Contain => "适应",
            Self::Tile => "平铺",
            Self::Center => "原尺寸",
        }
    }
    pub fn all() -> [Self; 4] {
        [Self::Cover, Self::Contain, Self::Tile, Self::Center]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1220.0,
            height: 800.0,
            x: -1.0,
            y: -1.0,
            maximized: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub config_version: u32,

    // ---- 仓库 ----
    pub last_folder: Option<PathBuf>,
    pub recent_folders: Vec<PathBuf>,
    pub recent_files: Vec<PathBuf>,
    pub sidebar_tab: SidebarTab,
    pub sidebar_width: f32,
    pub show_sidebar: bool,
    pub restored_expanded: Vec<PathBuf>,
    pub open_file: Option<PathBuf>,

    // ---- 视图 ----
    pub mode: ViewMode,
    pub editor_split: f32,
    pub zen_mode: bool,
    pub restore_scroll: bool,

    // ---- 外观 ----
    pub theme: ThemeId,
    pub custom_bg: Option<String>,
    pub custom_text: Option<String>,
    pub custom_accent: Option<String>,
    pub bg_image: Option<PathBuf>,
    pub bg_image_opacity: f32,
    pub bg_image_fit: ImageFit,
    pub bg_image_dim: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub content_width: f32,
    pub ui_density: f32,

    // ---- 字体 ----
    pub font_prop: Option<String>,
    pub font_mono: Option<String>,

    // ---- 目录树 ----
    pub show_hidden: bool,
    pub show_non_md: bool,
    pub sort_by: SortBy,
    pub sort_desc: bool,
    pub dirs_first: bool,
    pub confirm_delete: bool,
    pub expand_on_open: bool,

    // ---- 编辑 / 保存 ----
    pub autosave: bool,
    /// 编辑模式下左右两栏联动滚动 / 高亮当前块
    pub sync_scrolling: bool,
    pub tab_spaces: usize,
    pub soft_wrap: bool,
    pub font_size_editor: f32,

    // ---- 其它 ----
    pub watch_files: bool,
    pub scroll_memory: HashMap<String, f32>,
    pub window: WindowState,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: 1,
            last_folder: None,
            recent_folders: Vec::new(),
            recent_files: Vec::new(),
            sidebar_tab: SidebarTab::Files,
            sidebar_width: 268.0,
            show_sidebar: true,
            restored_expanded: Vec::new(),
            open_file: None,

            mode: ViewMode::Read,
            editor_split: 0.5,
            zen_mode: false,
            restore_scroll: true,

            theme: ThemeId::Paper,
            custom_bg: None,
            custom_text: None,
            custom_accent: None,
            bg_image: None,
            bg_image_opacity: 0.35,
            bg_image_fit: ImageFit::Cover,
            bg_image_dim: 0.15,
            font_size: 17.0,
            line_height: 1.55,
            content_width: 880.0,
            ui_density: 1.0,

            font_prop: None,
            font_mono: None,

            show_hidden: false,
            show_non_md: false,
            sort_by: SortBy::Name,
            sort_desc: false,
            dirs_first: true,
            confirm_delete: true,
            expand_on_open: false,

            autosave: true,
            sync_scrolling: true,
            tab_spaces: 4,
            soft_wrap: true,
            font_size_editor: 15.0,

            watch_files: true,
            scroll_memory: HashMap::new(),
            window: WindowState::default(),
        }
    }
}

impl AppConfig {
    /// 配置文件路径。优先使用程序目录下的 `easymd.json`（便携模式），否则用系统配置目录。
    pub fn config_path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let portable = dir.join("easymd.json");
                if portable.exists() {
                    return portable;
                }
            }
        }
        let dir = crate::util::config_dir().join("EasyMD");
        dir.join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Self>(&text) {
                Ok(cfg) => {
                    log_line(&format!("已加载配置 {}", path.display()));
                    cfg.sanitize()
                }
                Err(e) => {
                    log_line(&format!("配置解析失败({e})，使用默认配置"));
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// 写入指定路径（调用方持有路径，测试与便携模式都不会动到用户真实配置）。
    pub fn save_to(&self, path: &Path) {
        let path = path.to_path_buf();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// 修正越界或明显不合理的值，避免坏配置导致界面异常。
    fn sanitize(mut self) -> Self {
        self.font_size = self.font_size.clamp(10.0, 40.0);
        self.font_size_editor = self.font_size_editor.clamp(10.0, 34.0);
        self.line_height = self.line_height.clamp(1.0, 2.2);
        self.content_width = self.content_width.clamp(420.0, 2400.0);
        self.sidebar_width = self.sidebar_width.clamp(180.0, 720.0);
        self.editor_split = self.editor_split.clamp(0.15, 0.85);
        self.bg_image_opacity = self.bg_image_opacity.clamp(0.0, 1.0);
        self.bg_image_dim = self.bg_image_dim.clamp(0.0, 1.0);
        self.ui_density = self.ui_density.clamp(0.75, 1.4);
        self.tab_spaces = self.tab_spaces.clamp(1, 8);
        self.window.width = self.window.width.clamp(600.0, 20000.0);
        self.window.height = self.window.height.clamp(400.0, 20000.0);
        self.recent_folders.dedup();
        self.recent_files.dedup();
        self
    }

    /// 记录滚动位置（限制条目数量，避免配置无限膨胀）。
    pub fn remember_scroll(&mut self, path: &Path, offset: f32) {
        if offset <= 0.5 {
            self.scroll_memory.remove(&path.to_string_lossy().to_string());
            return;
        }
        if self.scroll_memory.len() > 600 {
            self.scroll_memory.retain(|_, v| *v > 1.0);
        }
        self.scroll_memory
            .insert(path.to_string_lossy().to_string(), offset);
    }

    pub fn scroll_for(&self, path: &Path) -> f32 {
        self.scroll_memory
            .get(&path.to_string_lossy().to_string())
            .copied()
            .unwrap_or(0.0)
    }

    pub fn push_recent_folder(&mut self, path: &Path) {
        let p = path.to_path_buf();
        self.recent_folders.retain(|x| x != &p);
        self.recent_folders.insert(0, p);
        self.recent_folders.truncate(12);
    }

    pub fn push_recent_file(&mut self, path: &Path) {
        let p = path.to_path_buf();
        self.recent_files.retain(|x| x != &p);
        self.recent_files.insert(0, p);
        self.recent_files.truncate(24);
    }
}

/// 极简日志：写入配置目录下的 easymd.log（出问题时便于排查）。
pub fn log_line(msg: &str) {
    use std::io::Write;
    let path = AppConfig::config_path().with_file_name("easymd.log");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[{}] {}", crate::util::format_datetime(std::time::SystemTime::now()), msg);
    }
}
