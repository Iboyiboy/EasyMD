// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 应用主状态与编排：面板布局、操作分发、快捷键、持久化。

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_commonmark::CommonMarkCache;

use crate::config::{AppConfig, SidebarTab, SortBy, ViewMode};
use crate::dialogs::{ConfirmAction, Dialog, DialogOutcome, Dialogs};
use crate::doc::Document;
use crate::explorer::{self, ExplorerState, NavKeys};
use crate::fonts::{self, FamilyInfo, FontScan, InstallReport};
use crate::fs_ops;
use crate::loader::EasyMdLoader;
use crate::reader;
use crate::search::SearchState;
use crate::settings::{self, SettingsSnapshot};
use crate::theme::{BackgroundImage, Theme, ThemeId};
use crate::tree::Tree;
use crate::ui_ext::Toasts;
use crate::watch::Watcher;

/// 分栏联动状态。
///
/// 两条硬规则：
/// 1. **从不直接改写滚动偏移**，只发 `Ui::scroll_to_rect` 这样的动画滚动请求——
///    egui 的 ScrollArea 内部有动画目标，若每帧强行写死偏移，两者会互相拉扯、永不收敛，
///    表现为无限重绘（滑动时界面卡死）。
/// 2. **只在确实发生用户操作时发请求**，静止时零请求。
#[derive(Default)]
pub struct SyncState {
    /// 上一帧的预览锚点表（源码字节偏移 → 行顶/行底）
    pub prev_anchors: Vec<(usize, f32, f32)>,
    /// 本帧要请求的预览滚动位置
    pub preview_to: Option<f32>,
    /// 本帧要请求的编辑器滚动位置
    pub editor_to: Option<f32>,
    /// 本帧要高亮的源码字节范围（编辑器光标所在块）
    pub highlight: Option<(usize, usize)>,
    /// 上一帧的光标位置（判断光标是否移动）
    pub cursor: usize,
    /// 上一帧读到的两侧滚动位置
    pub last_editor_pos: f32,
    pub last_preview_pos: f32,
    /// 最近一次读到的预览滚动位置（供判断光标块是否在视野内）
    pub preview_scroll: f32,
    /// 程序化滚动进行中（动画期间忽略该侧的偏移变化，避免被误判为用户滚动）
    prog_editor: bool,
    prog_preview: bool,
    /// 检测到需要同步、但当时锚点表还没建立起来（首帧），下一帧补上
    pending_preview_sync: bool,
    pending_editor_sync: bool,
}

/// 界面发出的操作请求，统一在帧末执行，避免借用冲突。
#[derive(Debug, Clone)]
pub enum Op {
    PickFolder,
    PickFile,
    OpenFolderPath(PathBuf),
    OpenFilePath(PathBuf),
    OpenFile(PathBuf),
    OpenFolderHere(PathBuf),
    CloseDoc,
    Save,
    ReloadDoc,
    NewFile(PathBuf),
    NewFolder(PathBuf),
    StartRename(PathBuf),
    CommitRename { path: PathBuf, name: String },
    Delete(Vec<PathBuf>),
    MoveInto { src: PathBuf, dest_dir: PathBuf },
    ExpandSubtree(PathBuf),
    SetExpand { path: PathBuf, expanded: bool },
    SelectAndToggle { path: PathBuf, focus: Option<usize> },
    ExpandAll,
    CollapseAll,
    RefreshTree,
    RevealInSidebar(PathBuf),
    /// 取消选中，把新建 / 拖放目标切回仓库根目录
    SelectRoot,
    RevealInOs(PathBuf),
    CopyToClipboard(String),
    SetSidebarTab(SidebarTab),
    ToggleSidebar,
    SetMode(ViewMode),
    ToggleZen,
    SetSort(SortBy),
    ToggleSortDesc,
    ToggleDirsFirst,
    ToggleShowHidden,
    ToggleShowNonMd,
    SetTheme(ThemeId),
    ToggleDarkLight,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    OpenSettings,
    ShowShortcuts,
    ShowAbout,
    StartFullTextSearch(String, bool),
    OpenSearchHit(PathBuf, usize),
    OutlineJump(String),
    /// 打开正文里被点击的链接（仓库内文件或外部地址）
    FollowLink(String),
    /// 在编辑模式中定位到指定行
    OpenAtLine(PathBuf, usize),
    Back,
    Forward,
}

pub struct App {
    pub cfg: AppConfig,
    /// 配置持久化目标路径（测试里指向临时文件，绝不写用户真实配置）
    pub config_path: PathBuf,
    pub theme: Theme,
    pub tree: Tree,
    pub explorer: ExplorerState,
    pub search: SearchState,
    pub doc: Option<Document>,
    pub cache: CommonMarkCache,
    pub bg: BackgroundImage,
    pub toasts: Toasts,
    pub dialogs: Dialogs,
    pub show_settings: bool,
    pub settings_snap: SettingsSnapshot,

    pub families: Vec<FamilyInfo>,
    pub font_scan: Option<Receiver<FontScan>>,
    pub font_report: InstallReport,
    pub default_fonts_picked: bool,
    pub line_gap_supported: bool,
    /// 当前已安装的字体（避免重复安装、白白多占一份内存）
    pub installed_fonts: (Option<String>, Option<String>),

    pub loader: Arc<EasyMdLoader>,
    pub watcher: Option<Watcher>,
    pub watch_root: Option<PathBuf>,

    pub history: Vec<PathBuf>,
    pub history_idx: usize,

    pub ops: Vec<Op>,
    pub pending_scroll: Option<f32>,
    pub pending_cursor: Option<usize>,
    /// 最近一帧的滚动位置与光标位置（用于持久化与状态栏）
    pub last_scroll: Option<f32>,
    pub last_cursor: usize,
    pub closing_saved: bool,
    pub last_edit: Instant,
    pub last_save: Instant,
    pub need_restyle: bool,
    pub need_fonts: bool,
    pub top_bar_h: f32,
    /// 右上角悬浮按钮占用的高度（用于给内容区留白）
    pub floating_h: f32,
    pub zen_prev_sidebar: bool,
    pub show_help_hint: bool,
    /// 仓库内 Markdown 文件总数（打开仓库/刷新时统计一次）
    pub repo_doc_count: usize,
    /// 上一次设置的窗口标题（避免每帧重复发送命令）
    pub last_title: String,
    /// 编辑模式下的左右联动状态
    pub sync: SyncState,
    /// 编辑器本帧的可视信息（上一帧的结果用于反向联动）
    pub editor_view: Option<crate::editor::EditorView>,
    /// 分栏两栏的矩形（上一帧），用于判断指针落在哪一栏
    pub pane_rects: Option<(egui::Rect, egui::Rect)>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: AppConfig,
        cli_path: Option<PathBuf>,
    ) -> Self {
        Self::from_ctx(&cc.egui_ctx, cfg, cli_path)
    }

    /// 与 [`App::new`] 相同，但只依赖一个 `egui::Context`。
    /// 这样无头测试也能构造出完整应用（构造 `eframe::CreationContext` 需要真实窗口系统）。
    pub fn from_ctx(ctx: &egui::Context, mut cfg: AppConfig, cli_path: Option<PathBuf>) -> Self {
        let theme = Theme::from_id(cfg.theme).apply_overrides(&cfg);
        let mut line_gap_supported = false;
        apply_style_to_ctx(ctx, &theme, &cfg, line_gap_supported);
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
        });

        // 自定义资源加载器（相对路径图片）
        egui_extras::install_image_loaders(ctx);
        let loader = EasyMdLoader::install(ctx);

        // 启动即装好一套可用字体（含中文），后台再扫描系统字体列表供设置页选择
        let quick = fonts::quick_defaults();
        let mut quick_installed = false;
        if cfg.font_prop.is_none() {
            if let Some(p) = &quick.prop {
                cfg.font_prop = Some(p.name.clone());
                quick_installed = true;
            }
        }
        if cfg.font_mono.is_none() {
            if let Some(m) = &quick.mono {
                cfg.font_mono = Some(m.name.clone());
                quick_installed = true;
            }
        }
        if quick_installed {
            let scan = FontScan {
                families: quick.families(),
            };
            let report = fonts::install(
                ctx,
                &scan,
                cfg.font_prop.as_deref(),
                cfg.font_mono.as_deref(),
                cfg.line_height,
                cfg.font_size,
            );
            line_gap_supported = report.line_gap_applied;
        }
        let font_scan = Some(fonts::scan_async());

        let root = cfg
            .last_folder
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let tree = Tree::new(root.clone(), &cfg, &cfg.restored_expanded);
        let watcher = if cfg.watch_files {
            Watcher::new(&root)
        } else {
            None
        };

        let mut app = Self {
            cfg,
            config_path: AppConfig::config_path(),
            theme,
            tree,
            explorer: ExplorerState::default(),
            search: SearchState::default(),
            doc: None,
            cache: CommonMarkCache::default(),
            bg: BackgroundImage::default(),
            toasts: Toasts::default(),
            dialogs: Dialogs::default(),
            show_settings: false,
            settings_snap: SettingsSnapshot::default(),
            families: Vec::new(),
            font_scan,
            font_report: InstallReport::default(),
            default_fonts_picked: quick_installed,
            line_gap_supported,
            installed_fonts: (None, None),
            loader,
            watcher,
            watch_root: Some(root),
            history: Vec::new(),
            history_idx: 0,
            ops: Vec::new(),
            pending_scroll: None,
            pending_cursor: None,
            last_scroll: None,
            last_cursor: 0,
            closing_saved: false,
            last_edit: Instant::now(),
            last_save: Instant::now(),
            need_restyle: false,
            need_fonts: !quick_installed,
            top_bar_h: 34.0,
            floating_h: 46.0,
            zen_prev_sidebar: false,
            show_help_hint: true,
            repo_doc_count: 0,
            last_title: String::new(),
            sync: SyncState::default(),
            editor_view: None,
            pane_rects: None,
        };
        app.loader
            .set_current_dir(app.doc.as_ref().map(|d| d.dir()));
        app.startup(cli_path);
        app
    }

    fn startup(&mut self, cli_path: Option<PathBuf>) {
        let path = cli_path.or_else(|| self.cfg.open_file.clone());
        if let Some(p) = path {
            if p.is_dir() {
                self.set_root(p, true);
            } else if p.is_file() {
                if !crate::util::is_within(&p, &self.tree.root) {
                    if let Some(dir) = p.parent() {
                        self.set_root(dir.to_path_buf(), false);
                    }
                }
                self.open_file(p);
            }
        }
        // 上次的仓库不存在时提示
        if let Some(f) = &self.cfg.last_folder {
            if !f.exists() {
                self.dialogs.open(Dialog::MissingFolder { path: f.clone() });
            }
        }
        if crate::tree::count_markdown_files(&self.tree.root, 50_000) == 0 && self.cfg.last_folder.is_none() {
            self.show_help_hint = true;
        }
        self.recount_docs();
    }

    // ------------------------------------------------------------ 仓库

    pub fn set_root(&mut self, folder: PathBuf, announce: bool) {
        if !folder.is_dir() {
            self.toasts.error("选择的不是文件夹");
            return;
        }
        self.save_doc_if_needed(true);
        let expanded: Vec<PathBuf> = if self.cfg.last_folder.as_deref() == Some(folder.as_path()) {
            self.cfg.restored_expanded.clone()
        } else {
            Vec::new()
        };
        self.tree = Tree::new(folder.clone(), &self.cfg, &expanded);
        if self.cfg.expand_on_open {
            self.tree.expand_all(&self.cfg);
        }
        self.cfg.last_folder = Some(folder.clone());
        self.cfg.push_recent_folder(&folder);
        self.cfg.restored_expanded = self.tree.expanded_paths();
        self.watch_root = Some(folder.clone());
        self.watcher = if self.cfg.watch_files {
            Watcher::new(&folder)
        } else {
            None
        };
        self.search.cancel();
        self.recount_docs();
        if announce {
            let n = self.repo_doc_count;
            self.toasts.info(format!("已打开仓库，共 {n} 个 Markdown 文件"));
        }
        self.persist();
    }

    fn save_doc_if_needed(&mut self, force: bool) {
        let should = {
            let Some(doc) = &self.doc else { return };
            doc.is_dirty() && (force || self.cfg.autosave)
        };
        if !should {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            match doc.save() {
                Ok(()) => {
                    let name = doc.name();
                    let _ = name;
                }
                Err(e) => {
                    self.toasts.error(format!("保存失败：{e}"));
                }
            }
        }
    }

    pub fn open_file(&mut self, path: PathBuf) {
        if self.doc.as_ref().map(|d| d.path == path).unwrap_or(false) {
            return;
        }
        self.save_doc_if_needed(false);
        if !path.exists() {
            crate::config::log_line(&format!("open_file: 不存在 {}", path.display()));
            self.toasts.error(format!("文件不存在：{}", path.display()));
            self.tree.reload_all(&self.cfg);
            return;
        }
        match Document::load(&path) {
            Ok(doc) => {
                let restore = if self.cfg.restore_scroll {
                    Some(self.cfg.scroll_for(&doc.path))
                } else {
                    None
                };
                if restore.map(|r| r > 1.0).unwrap_or(false) {
                    // 恢复阅读位置
                    self.pending_scroll = restore;
                } else {
                    self.pending_scroll = Some(0.0);
                }
                self.doc = Some(doc);
                self.loader
                    .set_current_dir(self.doc.as_ref().map(|d| d.dir()));
                self.tree.select(Some(path.clone()));
                if self.cfg.sidebar_tab == SidebarTab::Files {
                    self.tree.reveal(&path, &self.cfg);
                }
                self.cfg.push_recent_file(&path);
                self.cfg.open_file = Some(path.clone());
                self.push_history(path.clone());
                self.persist();
            }
            Err(e) => {
                crate::config::log_line(&format!("open_file 失败: {e}"));
                self.toasts.error(format!("打开失败：{e}"));
                self.dialogs
                    .message("无法打开文件", e, true);
            }
        }
    }

    fn push_history(&mut self, path: PathBuf) {
        if self.history.get(self.history_idx) == Some(&path) {
            return;
        }
        self.history.truncate(self.history_idx + 1);
        self.history.push(path);
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.history_idx = self.history.len() - 1;
    }

    fn close_doc(&mut self) {
        self.save_doc_if_needed(true);
        if let Some(doc) = &self.doc {
            self.cfg.remember_scroll(&doc.path.clone(), 0.0);
        }
        self.doc = None;
        self.cfg.open_file = None;
        self.persist();
    }

    // ------------------------------------------------------------ 主题 / 字体

    fn apply_style(&mut self, ctx: &egui::Context) {
        apply_style_to_ctx(ctx, &self.theme, &self.cfg, self.line_gap_supported);
    }

    fn refresh_theme(&mut self, ctx: &egui::Context) {
        self.theme = Theme::from_id(self.cfg.theme).apply_overrides(&self.cfg);
        self.apply_style(ctx);
        self.need_restyle = false;
    }

    fn install_fonts(&mut self, ctx: &egui::Context) {
        if self.families.is_empty() {
            return;
        }
        let scan = FontScan {
            families: self.families.clone(),
        };
        let report = fonts::install(
            ctx,
            &scan,
            self.cfg.font_prop.as_deref(),
            self.cfg.font_mono.as_deref(),
            self.cfg.line_height,
            self.cfg.font_size,
        );
        self.line_gap_supported = report.line_gap_applied;
        for w in &report.warnings {
            self.toasts.warn(w.clone());
        }
        self.font_report = report;
        self.installed_fonts = (self.cfg.font_prop.clone(), self.cfg.font_mono.clone());
        self.need_fonts = false;
        // 字体变化后需要重新应用样式（行高补偿逻辑依赖 line_gap_supported）
        self.apply_style(ctx);
    }

    fn poll_font_scan(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.font_scan else { return };
        if let Ok(scan) = rx.try_recv() {
            self.families = scan.families.clone();
            if !self.default_fonts_picked {
                if self.cfg.font_prop.is_none() {
                    self.cfg.font_prop = fonts::pick_default_prop(&self.families);
                }
                if self.cfg.font_mono.is_none() {
                    self.cfg.font_mono = fonts::pick_default_mono(&self.families);
                }
                self.default_fonts_picked = true;
            }
            if let Some(f) = &self.cfg.font_prop {
                self.toasts.info(format!("已使用字体 {f}"));
            }
            self.font_scan = None;
            // 默认字体在启动时已经装好，只有用户指定的字体变了才需要重装
            let want = (self.cfg.font_prop.clone(), self.cfg.font_mono.clone());
            if want != self.installed_fonts {
                self.need_fonts = true;
                self.install_fonts(ctx);
            }
        }
    }

    // ------------------------------------------------------------ 滚动位置

    // ------------------------------------------------------------ 操作分发

    pub fn apply_ops(&mut self, ctx: &egui::Context) {
        let ops = std::mem::take(&mut self.ops);
        for op in ops {
            match op {
                Op::FollowLink(dest) => self.follow_link(&dest),
                Op::PickFolder => {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_title("选择文档仓库文件夹")
                        .pick_folder()
                    {
                        self.set_root(dir, true);
                    }
                }
                Op::PickFile => {
                    if let Some(file) = rfd::FileDialog::new()
                        .set_title("打开 Markdown 文件")
                        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "mkdn", "mdx", "txt"])
                        .pick_file()
                    {
                        if self.ensure_repo_for(&file) {
                            self.open_file(file);
                        }
                    }
                }
                Op::OpenFolderPath(p) => self.set_root(p, true),
                Op::OpenFilePath(p) => {
                    if self.ensure_repo_for(&p) {
                        self.open_file(p);
                    }
                }
                Op::OpenFile(p) => self.open_file(p),
                Op::OpenFolderHere(p) => {
                    self.tree.set_expanded(&p, true, &self.cfg);
                }
                Op::CloseDoc => self.close_doc(),
                Op::Save => {
                    self.save_doc_if_needed(true);
                    if let Some(doc) = &self.doc {
                        if !doc.is_dirty() {
                            self.toasts.ok("已保存");
                        }
                    }
                    self.persist();
                }
                Op::ReloadDoc => {
                    if let Some(doc) = self.doc.as_mut() {
                        match doc.reload_from_disk() {
                            Ok(()) => {
                                doc.force_refresh_outline();
                                self.toasts.info("已从磁盘重新载入");
                            }
                            Err(e) => self.toasts.error(e),
                        }
                    }
                }
                Op::NewFile(parent) => {
                    let name = crate::util::unique_child_name(&parent, "未命名", Some("md"));
                    self.dialogs.open(Dialog::NewFile {
                        parent,
                        name,
                        error: None,
                    });
                }
                Op::NewFolder(parent) => {
                    let name = crate::util::unique_child_name(&parent, "新建文件夹", None);
                    let _ = name;
                    self.dialogs.open(Dialog::NewFolder {
                        parent,
                        name: "新建文件夹".into(),
                        error: None,
                    });
                }
                Op::StartRename(path) => {
                    if path == self.tree.root {
                        self.toasts.warn("仓库根目录不能重命名");
                    } else {
                        self.explorer.start_rename(path);
                    }
                }
                Op::CommitRename { path, name } => {
                    match fs_ops::rename(&path, &name) {
                        Ok(new_path) => {
                            self.explorer.cancel_rename();
                            self.tree.reload_ancestors(&new_path, &self.cfg);
                            self.tree.select(Some(new_path.clone()));
                            // 当前打开的文档跟着改名
                            if let Some(doc) = self.doc.as_mut() {
                                if doc.path == path {
                                    doc.path = new_path.clone();
                                }
                            }
                            self.toasts.ok(format!("已重命名为 {name}"));
                        }
                        Err(e) => self.toasts.error(e),
                    }
                }
                Op::Delete(paths) => {
                    if self.cfg.confirm_delete {
                        self.dialogs.confirm_delete(paths);
                    } else {
                        self.do_delete(&paths);
                    }
                }
                Op::MoveInto { src, dest_dir } => match fs_ops::move_into(&src, &dest_dir) {
                    Ok(new_path) => {
                        self.tree.select(Some(new_path.clone()));
                        self.tree.reload_all(&self.cfg);
                        if let Some(doc) = self.doc.as_mut() {
                            if doc.path == src {
                                doc.path = new_path;
                            }
                        }
                        self.toasts.ok("移动完成");
                    }
                    Err(e) => self.toasts.warn(e),
                },
                Op::ExpandSubtree(path) => {
                    self.tree.set_expanded(&path, true, &self.cfg);
                    let mut stack = vec![path];
                    let mut guard = 0;
                    while let Some(p) = stack.pop() {
                        guard += 1;
                        if guard > 4000 {
                            break;
                        }
                        self.tree.set_expanded(&p, true, &self.cfg);
                        let children: Vec<PathBuf> = self
                            .tree
                            .find(&p)
                            .map(|e| {
                                e.children
                                    .iter()
                                    .filter(|c| c.is_dir)
                                    .map(|c| c.path.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        stack.extend(children);
                    }
                }
                Op::SetExpand { path, expanded } => {
                    self.tree.set_expanded(&path, expanded, &self.cfg);
                }
                Op::SelectAndToggle { path, focus } => {
                    self.tree.select(Some(path.clone()));
                    self.tree.toggle_expand(&path, &self.cfg);
                    if let Some(f) = focus {
                        self.explorer.focus_row = f;
                    }
                }
                Op::ExpandAll => {
                    self.tree.expand_all(&self.cfg);
                    self.toasts.info("已展开全部目录");
                }
                Op::CollapseAll => {
                    self.tree.collapse_all();
                }
                Op::RefreshTree => {
                    self.tree.reload_all(&self.cfg);
                    self.recount_docs();
                    self.toasts.info("目录树已刷新");
                }
                Op::SelectRoot => {
                    self.tree.select(None);
                    self.explorer.focus_row = 0;
                }
                Op::RevealInSidebar(path) => {
                    self.cfg.sidebar_tab = SidebarTab::Files;
                    self.cfg.show_sidebar = true;
                    self.tree.reveal(&path, &self.cfg);
                }
                Op::RevealInOs(path) => fs_ops::reveal_in_file_manager(&path),
                Op::CopyToClipboard(t) => {
                    ctx.copy_text(t);
                    self.toasts.info("已复制到剪贴板");
                }
                Op::SetSidebarTab(t) => {
                    self.cfg.sidebar_tab = t;
                    if t == SidebarTab::Search {
                        self.explorer.focus_search = true;
                    }
                }
                Op::ToggleSidebar => self.cfg.show_sidebar = !self.cfg.show_sidebar,
                Op::SetMode(m) => {
                    self.cfg.mode = m;
                    self.persist();
                    ctx.request_repaint();
                }
                Op::ToggleZen => {
                    let toggling_on = !self.cfg.zen_mode;
                    self.cfg.zen_mode = toggling_on;
                    if toggling_on {
                        self.zen_prev_sidebar = self.cfg.show_sidebar;
                        self.cfg.show_sidebar = false;
                    } else {
                        self.cfg.show_sidebar = self.zen_prev_sidebar;
                    }
                }
                Op::SetSort(s) => {
                    if self.cfg.sort_by == s {
                        self.cfg.sort_desc = !self.cfg.sort_desc;
                    } else {
                        self.cfg.sort_by = s;
                    }
                    self.tree.reload_all(&self.cfg);
                    self.persist();
                }
                Op::ToggleSortDesc => {
                    self.cfg.sort_desc = !self.cfg.sort_desc;
                    self.tree.reload_all(&self.cfg);
                }
                Op::ToggleDirsFirst => {
                    self.cfg.dirs_first = !self.cfg.dirs_first;
                    self.tree.reload_all(&self.cfg);
                }
                Op::ToggleShowHidden => {
                    self.cfg.show_hidden = !self.cfg.show_hidden;
                    self.tree.invalidate();
                    self.persist();
                }
                Op::ToggleShowNonMd => {
                    self.cfg.show_non_md = !self.cfg.show_non_md;
                    self.tree.invalidate();
                    self.persist();
                }
                Op::SetTheme(t) => {
                    self.cfg.theme = t;
                    self.cfg.custom_bg = None;
                    self.refresh_theme(ctx);
                    self.persist();
                }
                Op::ToggleDarkLight => {
                    self.cfg.theme = self.cfg.theme.next_light_dark();
                    self.cfg.custom_bg = None;
                    self.refresh_theme(ctx);
                    self.persist();
                }
                Op::ZoomIn => {
                    self.cfg.font_size = (self.cfg.font_size + 1.0).min(34.0);
                    self.cfg.font_size_editor = (self.cfg.font_size_editor + 1.0).min(30.0);
                    self.need_fonts = true;
                    self.persist();
                }
                Op::ZoomOut => {
                    self.cfg.font_size = (self.cfg.font_size - 1.0).max(12.0);
                    self.cfg.font_size_editor = (self.cfg.font_size_editor - 1.0).max(11.0);
                    self.need_fonts = true;
                    self.persist();
                }
                Op::ZoomReset => {
                    self.cfg.font_size = 17.0;
                    self.cfg.font_size_editor = 15.0;
                    self.need_fonts = true;
                    self.persist();
                }
                Op::OpenSettings => {
                    self.show_settings = true;
                    self.settings_snap = SettingsSnapshot::of(&self.cfg);
                }
                Op::ShowShortcuts => self.dialogs.open(Dialog::Shortcuts),
                Op::ShowAbout => self.dialogs.open(Dialog::About),
                Op::StartFullTextSearch(q, case) => {
                    self.cfg.sidebar_tab = SidebarTab::Search;
                    self.cfg.show_sidebar = true;
                    let root = self.tree.root.clone();
                    self.search.start(root, q, case, 2000);
                }
                Op::OpenSearchHit(path, line) => {
                    self.open_file(path);
                    if let Some(doc) = self.doc.as_mut() {
                        doc.goto_line(line);
                        self.pending_cursor = doc.pending_cursor;
                    }
                    // 搜索结果定位到编辑模式更直观
                    if self.cfg.mode == ViewMode::Read {
                        self.cfg.mode = ViewMode::Edit;
                    }
                }
                Op::OutlineJump(slug) => {
                    if let Some(doc) = self.doc.as_mut() {
                        doc.pending_anchor = Some(slug);
                    }
                    if self.cfg.mode == ViewMode::Edit {
                        self.cfg.mode = ViewMode::Read;
                    }
                }
                Op::OpenAtLine(path, line) => {
                    self.open_file(path);
                    if let Some(doc) = self.doc.as_mut() {
                        doc.goto_line(line);
                        self.pending_cursor = doc.pending_cursor;
                    }
                    self.cfg.mode = ViewMode::Edit;
                }
                Op::Back => self.navigate(-1),
                Op::Forward => self.navigate(1),
            }
        }
    }

    /// 渲染后统一决策：本帧读到的仍然是两侧的**真实**位置，
    /// 因此「上一帧 vs 本帧」的比较在这里做最可靠；算出的请求与高亮留到下一帧执行
    /// （1 帧延迟，肉眼不可见）。
    fn sync_end_frame(&mut self, doc: &Document) {
        let ed_now = self
            .editor_view
            .as_ref()
            .map(|v| v.scroll_offset)
            .unwrap_or(0.0);
        let pv_now = self.sync.preview_scroll;

        // 程序化动画结束判定：位置不再变化就解除标记
        if self.sync.prog_editor && (ed_now - self.sync.last_editor_pos).abs() < 0.5 {
            self.sync.prog_editor = false;
        }
        if self.sync.prog_preview && (pv_now - self.sync.last_preview_pos).abs() < 0.5 {
            self.sync.prog_preview = false;
        }

        // 用户是否在滚动某一侧
        let editor_moved = !self.sync.prog_editor && (ed_now - self.sync.last_editor_pos).abs() > 1.5;
        let preview_moved =
            !self.sync.prog_preview && (pv_now - self.sync.last_preview_pos).abs() > 1.5;
        self.sync.last_editor_pos = ed_now;
        self.sync.last_preview_pos = pv_now;

        self.sync.preview_to = None;
        self.sync.editor_to = None;
        if !self.cfg.sync_scrolling {
            self.sync.highlight = None;
            self.sync.cursor = self.last_cursor;
            return;
        }

        // 高亮：编辑器光标所在块（用本帧刚算出的锚点表，无延迟）
        let cursor_byte = char_to_byte(&doc.text, self.last_cursor);
        self.sync.highlight = Some(crate::doc::block_range_at(&doc.text, cursor_byte));

        let cursor_moved = self.last_cursor != self.sync.cursor;
        self.sync.cursor = self.last_cursor;
        if editor_moved || cursor_moved {
            self.sync.pending_preview_sync = true;
        }
        if preview_moved {
            self.sync.pending_editor_sync = true;
        }

        // 锚点表还没建立（首帧）：把待同步标记留着，下一帧再来
        if self.sync.prev_anchors.is_empty() {
            return;
        }
        let (_, preview_rect) = self
            .pane_rects
            .unwrap_or((egui::Rect::NOTHING, egui::Rect::NOTHING));

        // ---------- 编辑器 → 预览 ----------
        if self.sync.pending_preview_sync {
            if editor_moved {
                // 用户在滚编辑器：预览顶部对齐到编辑器视口顶部所在那一行
                if let Some(view) = &self.editor_view {
                    let top_byte = char_to_byte(&doc.text, view.top_char);
                    // 锚点表是「渲染文本」坐标，这里把原文偏移换算过去
                    let top_byte = doc.text_to_render_offset(top_byte);
                    if let Some(y) = crate::reader::y_for_offset(&self.sync.prev_anchors, top_byte) {
                        self.sync.preview_to = Some((y - 6.0).max(0.0));
                        self.sync.prog_preview = true;
                    }
                }
            } else if let Some(y) = crate::reader::y_for_offset(
                &self.sync.prev_anchors,
                doc.text_to_render_offset(cursor_byte),
            ) {
                // 光标所在块若不在预览视野内，把它带回来
                let view_h = preview_rect.height().max(120.0);
                let top = self.sync.preview_scroll;
                if y < top + 8.0 || y > top + view_h - 40.0 {
                    self.sync.preview_to = Some((y - view_h * 0.34).max(0.0));
                    self.sync.prog_preview = true;
                }
            }
            self.sync.pending_preview_sync = false;
        }

        // ---------- 预览 → 编辑器 ----------
        if self.sync.pending_editor_sync {
            let y = self.sync.preview_scroll + 8.0;
            if let Some(byte) = crate::reader::offset_for_y(&self.sync.prev_anchors, y) {
                let byte = doc.render_to_text_offset(byte);
                let char_index = byte_to_char(&doc.text, byte);
                if let Some(view) = &self.editor_view {
                    if let Some(line_y) = view.y_of_char(char_index) {
                        self.sync.editor_to = Some((line_y - 6.0).max(0.0));
                        self.sync.prog_editor = true;
                    }
                }
            }
            self.sync.pending_editor_sync = false;
        }
    }

    fn navigate(&mut self, delta: i32) {
        let idx = self.history_idx as i32 + delta;
        if idx < 0 || idx as usize >= self.history.len() {
            return;
        }
        let path = self.history[idx as usize].clone();
        self.history_idx = idx as usize;
        if path.exists() {
            self.open_file(path);
            // open_file 会重置历史索引，这里修正
            if let Some(pos) = self.history.iter().position(|p| p == &self.cfg.open_file.clone().unwrap_or_default()) {
                self.history_idx = pos;
            }
        } else {
            self.toasts.warn("文件已不存在");
        }
    }

    fn do_delete(&mut self, paths: &[PathBuf]) {
        for p in paths {
            if p == &self.tree.root {
                self.toasts.warn("不能删除仓库根目录");
                continue;
            }
            let was_open = self
                .doc
                .as_ref()
                .map(|d| d.path == *p || crate::util::is_within(&d.path, p))
                .unwrap_or(false);
            match fs_ops::delete(p) {
                Ok(()) => {
                    if was_open {
                        self.doc = None;
                        self.cfg.open_file = None;
                    }
                    let parent = p.parent().map(|x| x.to_path_buf());
                    if let Some(parent) = parent {
                        self.tree.reload_ancestors(&parent, &self.cfg);
                    }
                    self.toasts.ok(format!("已删除 {}", crate::util::file_name(p)));
                }
                Err(e) => self.toasts.error(e),
            }
        }
        self.tree.reload_all(&self.cfg);
    }

    /// 打开仓库外的单个文件时，把其父目录作为临时仓库。
    fn ensure_repo_for(&mut self, file: &std::path::Path) -> bool {
        let parent = match file.parent() {
            Some(p) => p.to_path_buf(),
            None => return false,
        };
        if !crate::util::is_within(file, &self.tree.root) {
            self.set_root(parent, false);
        }
        true
    }

    /// 统计仓库内 Markdown 文件数量（有上限，避免超大仓库卡顿）。
    fn recount_docs(&mut self) {
        self.repo_doc_count = crate::tree::count_markdown_files(&self.tree.root, 50_000);
    }

    fn persist(&mut self) {
        self.cfg.open_file = self.doc.as_ref().map(|d| d.path.clone());
        self.cfg.restored_expanded = self.tree.expanded_paths();
        if let Some(doc) = &self.doc {
            if self.cfg.restore_scroll {
                if let Some(s) = self.last_scroll {
                    let p = doc.path.clone();
                    self.cfg.remember_scroll(&p, s);
                }
            }
        }
        self.cfg.save_to(&self.config_path);
    }

    // ------------------------------------------------------------ 快捷键

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let alt = Modifiers::ALT;
        let text_focused = ctx.memory(|m| m.focused().is_some());

        ctx.input_mut(|i| {
            if i.consume_key(cmd, Key::O) {
                self.ops.push(Op::PickFolder);
            }
            if i.consume_key(shift, Key::O) {
                self.ops.push(Op::PickFile);
            }
            if i.consume_key(cmd, Key::S) {
                self.ops.push(Op::Save);
            }
            if i.consume_key(cmd, Key::N) {
                self.ops.push(Op::NewFile(explorer::target_dir(&self.tree)));
            }
            if i.consume_key(shift, Key::N) {
                self.ops.push(Op::NewFolder(explorer::target_dir(&self.tree)));
            }
            if i.consume_key(cmd, Key::B) {
                self.ops.push(Op::ToggleSidebar);
            }
            if i.consume_key(cmd, Key::E) {
                self.ops.push(Op::SetMode(ViewMode::Edit));
            }
            if i.consume_key(cmd, Key::R) {
                self.ops.push(Op::SetMode(ViewMode::Read));
            }
            if i.consume_key(cmd, Key::D) {
                self.ops.push(Op::ToggleDarkLight);
            }
            if i.consume_key(cmd, Key::W) {
                self.ops.push(Op::CloseDoc);
            }
            if i.consume_key(cmd, Key::Equals) || i.consume_key(cmd, Key::Plus) {
                self.ops.push(Op::ZoomIn);
            }
            if i.consume_key(cmd, Key::Minus) {
                self.ops.push(Op::ZoomOut);
            }
            if i.consume_key(cmd, Key::Num0) {
                self.ops.push(Op::ZoomReset);
            }
            if i.consume_key(cmd, Key::Slash) {
                self.ops.push(Op::ShowShortcuts);
            }
            if i.consume_key(cmd, Key::Comma) {
                self.ops.push(Op::OpenSettings);
            }
            if i.consume_key(cmd, Key::F) {
                if self.cfg.sidebar_tab == SidebarTab::Search {
                    self.ops.push(Op::StartFullTextSearch(
                        self.explorer.search_input.clone(),
                        self.explorer.search_case,
                    ));
                } else {
                    self.cfg.sidebar_tab = SidebarTab::Files;
                    self.cfg.show_sidebar = true;
                }
            }
            if i.consume_key(shift, Key::F) {
                self.cfg.sidebar_tab = SidebarTab::Search;
                self.cfg.show_sidebar = true;
                self.explorer.focus_search = true;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F5) {
                self.ops.push(Op::RefreshTree);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F2) {
                if let Some(p) = self.tree.selected.clone() {
                    self.ops.push(Op::StartRename(p));
                }
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Delete) && !text_focused {
                if let Some(p) = self.tree.selected.clone() {
                    self.ops.push(Op::Delete(vec![p]));
                }
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F11) {
                self.ops.push(Op::ToggleZen);
            }
            if i.consume_key(alt, Key::ArrowLeft) {
                self.ops.push(Op::Back);
            }
            if i.consume_key(alt, Key::ArrowRight) {
                self.ops.push(Op::Forward);
            }
            // Ctrl + 滚轮缩放
            let zoom = i.zoom_delta();
            if (zoom - 1.0).abs() > 0.001 && i.modifiers.command {
                if zoom > 1.0 {
                    self.ops.push(Op::ZoomIn);
                } else {
                    self.ops.push(Op::ZoomOut);
                }
            }
            if i.key_pressed(Key::Escape) {
                if self.cfg.zen_mode && !self.dialogs.is_open() {
                    self.ops.push(Op::ToggleZen);
                } else if !self.tree.query.is_empty() {
                    self.tree.set_query(String::new());
                }
            }
        });
    }

    fn nav_keys(&mut self, ctx: &egui::Context) -> NavKeys {
        let text_focused = ctx.memory(|m| m.focused().is_some());
        if text_focused || self.cfg.zen_mode || !self.cfg.show_sidebar {
            return NavKeys::NONE;
        }
        if self.cfg.sidebar_tab != SidebarTab::Files {
            return NavKeys::NONE;
        }
        if self.dialogs.is_open() || self.show_settings || self.explorer.rename.is_some() {
            return NavKeys::NONE;
        }
        ctx.input(|i| NavKeys {
            up: i.key_pressed(egui::Key::ArrowUp),
            down: i.key_pressed(egui::Key::ArrowDown),
            left: i.key_pressed(egui::Key::ArrowLeft),
            right: i.key_pressed(egui::Key::ArrowRight),
            enter: i.key_pressed(egui::Key::Enter),
            space: i.key_pressed(egui::Key::Space),
        })
    }

    // ------------------------------------------------------------ 文件监听

    fn poll_watcher(&mut self) {
        let Some(w) = self.watcher.as_mut() else { return };
        let changed = w.poll();
        if changed.is_empty() {
            return;
        }
        let current = self.doc.as_ref().map(|d| d.path.clone());
        for p in &changed {
            if let Some(dir) = if p.is_dir() {
                Some(p.clone())
            } else {
                p.parent().map(|x| x.to_path_buf())
            } {
                if self.tree.find(&dir).is_some() {
                    self.tree.load_dir(&dir, &self.cfg);
                }
            }
        }
        self.tree.invalidate();
        // 当前文档被写入：可能是其它程序改的，也可能是本程序自己保存触发的
        if let Some(cur) = current {
            if changed.iter().any(|p| p == &cur) {
                self.handle_external_write(&cur);
            }
        }
    }

    /// 处理当前文档的写入事件。
    fn handle_external_write(&mut self, path: &std::path::Path) {
        let Ok(loaded) = fs_ops::read_text(path) else {
            return;
        };
        let (memory, dirty) = match &self.doc {
            Some(doc) => (doc.text.clone(), doc.is_dirty()),
            None => return,
        };
        match crate::doc::classify_external_change(&loaded.text, &memory, dirty) {
            crate::doc::ExternalChange::Ignore => {}
            crate::doc::ExternalChange::Reload => {
                if let Some(doc) = self.doc.as_mut() {
                    let _ = doc.reload_from_disk();
                    doc.force_refresh_outline();
                }
                self.toasts.info("文档已在外部更新，已自动重新载入");
            }
            crate::doc::ExternalChange::KeepLocal => {
                // 不弹模态框打断输入，只提示；重新载入在「文件」菜单里
                self.toasts.warn(
                    "文件被其它程序修改，你的编辑已保留；可在「文件 → 重新载入当前文档」中放弃本地修改",
                );
            }
        }
    }

    fn poll_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let p = f.path();
            if p.as_os_str().is_empty() {
                continue;
            }
            if p.is_dir() {
                self.ops.push(Op::OpenFolderPath(p.to_path_buf()));
            } else if p.is_file() {
                self.ops.push(Op::OpenFilePath(p.to_path_buf()));
            }
        }
    }

    // ------------------------------------------------------------ 界面

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        egui::Panel::top("easymd-top")
            .exact_size(self.top_bar_h.max(30.0))
            .frame(
                egui::Frame::new()
                    .fill(self.theme.panel)
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .stroke(egui::Stroke::new(1.0, self.theme.border)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new("EasyMD")
                            .strong()
                            .color(self.theme.accent),
                    );
                    ui.separator();
                    ui.menu_button("文件", |ui| {
                        if ui.button("打开文件夹…\tCtrl+O").clicked() {
                            self.ops.push(Op::PickFolder);
                            ui.close();
                        }
                        if ui.button("打开文件…\tCtrl+Shift+O").clicked() {
                            self.ops.push(Op::PickFile);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("保存\tCtrl+S").clicked() {
                            self.ops.push(Op::Save);
                            ui.close();
                        }
                        if ui.button("重新载入当前文档").clicked() {
                            self.ops.push(Op::ReloadDoc);
                            ui.close();
                        }
                        if ui.button("关闭文档\tCtrl+W").clicked() {
                            self.ops.push(Op::CloseDoc);
                            ui.close();
                        }
                        ui.separator();
                        ui.menu_button("最近打开的仓库", |ui| {
                            let list = self.cfg.recent_folders.clone();
                            if list.is_empty() {
                                ui.label(egui::RichText::new("暂无记录").weak());
                            }
                            for f in list.iter().take(10) {
                                if ui
                                    .button(crate::util::elide_middle(&f.display().to_string(), 48))
                                    .clicked()
                                {
                                    self.ops.push(Op::OpenFolderPath(f.clone()));
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button("最近打开的文件", |ui| {
                            let list = self.cfg.recent_files.clone();
                            if list.is_empty() {
                                ui.label(egui::RichText::new("暂无记录").weak());
                            }
                            for f in list.iter().take(12) {
                                if ui
                                    .button(crate::util::elide_middle(&f.display().to_string(), 48))
                                    .clicked()
                                {
                                    self.ops.push(Op::OpenFilePath(f.clone()));
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        if ui.button("退出").clicked() {
                            self.ops.push(Op::Save);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("视图", |ui| {
                        for m in ViewMode::all() {
                            let label = format!(
                                "{}\t{}",
                                m.label(),
                                match m {
                                    ViewMode::Read => "Ctrl+R",
                                    ViewMode::Edit => "Ctrl+E",
                                }
                            );
                            if ui.selectable_label(self.cfg.mode == m, label).clicked() {
                                self.ops.push(Op::SetMode(m));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui
                            .selectable_label(self.cfg.show_sidebar, "显示侧栏\tCtrl+B")
                            .clicked()
                        {
                            self.ops.push(Op::ToggleSidebar);
                            ui.close();
                        }
                        if ui
                            .selectable_label(self.cfg.zen_mode, "禅模式\tF11")
                            .clicked()
                        {
                            self.ops.push(Op::ToggleZen);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("放大字体\tCtrl+=").clicked() {
                            self.ops.push(Op::ZoomIn);
                            ui.close();
                        }
                        if ui.button("缩小字体\tCtrl+-").clicked() {
                            self.ops.push(Op::ZoomOut);
                            ui.close();
                        }
                        if ui.button("重置字体\tCtrl+0").clicked() {
                            self.ops.push(Op::ZoomReset);
                            ui.close();
                        }
                        ui.separator();
                        ui.menu_button("主题", |ui| {
                            for id in ThemeId::all() {
                                if ui
                                    .selectable_label(self.cfg.theme == id, id.label())
                                    .clicked()
                                {
                                    self.ops.push(Op::SetTheme(id));
                                    ui.close();
                                }
                            }
                        });
                        if ui.button("切换深色 / 浅色\tCtrl+D").clicked() {
                            self.ops.push(Op::ToggleDarkLight);
                            ui.close();
                        }
                    });
                    ui.menu_button("帮助", |ui| {
                        if ui.button("快捷键…").clicked() {
                            self.ops.push(Op::ShowShortcuts);
                            ui.close();
                        }
                        if ui.button("设置…").clicked() {
                            self.ops.push(Op::OpenSettings);
                            ui.close();
                        }
                        if ui.button("关于 EasyMD").clicked() {
                            self.ops.push(Op::ShowAbout);
                            ui.close();
                        }
                    });

                    // 面包屑
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(doc) = &self.doc {
                            let path = doc.path.clone();
                            let root = self.tree.root.clone();
                            let rel = crate::util::elide_middle(
                                &doc.rel_display(Some(&root)),
                                70,
                            );
                            ui.label(
                                egui::RichText::new(rel)
                                    .color(self.theme.weak)
                                    .small(),
                            )
                            .on_hover_text(path.display().to_string());
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("easymd-status")
            .exact_size(24.0)
            .frame(
                egui::Frame::new()
                    .fill(self.theme.panel)
                    .inner_margin(egui::Margin::symmetric(10, 2))
                    .stroke(egui::Stroke::new(1.0, self.theme.border)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let small = |ui: &mut egui::Ui, t: String| {
                        ui.label(egui::RichText::new(t).small().color(self.theme.weak));
                    };
                    small(
                        ui,
                        format!(
                            "仓库：{}",
                            crate::util::elide_middle(&self.tree.root.display().to_string(), 26)
                        ),
                    );
                    small(ui, format!("{} 篇", self.repo_doc_count));
                    if let Some(doc) = &self.doc {
                        let stats = crate::util::text_stats(&doc.text);
                        if let Some((line, col)) = self.cursor_line_col() {
                            small(ui, format!("第 {line} 行 第 {col} 列"));
                        }
                        small(ui, format!("{} 字", stats.chars));
                        small(ui, crate::util::reading_time_text(&stats));
                        small(ui, crate::util::fmt_size(doc.bytes_on_disk));
                        small(ui, doc.encoding.label().to_string());
                        if doc.is_dirty() {
                            ui.label(
                                egui::RichText::new("● 未保存").small().color(self.theme.accent),
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        small(ui, "自动保存已开启".to_string());
                        if self.search.is_running() {
                            ui.label(
                                egui::RichText::new(format!("检索中 {}", self.search.progress.scanned))
                                    .small()
                                    .color(self.theme.weak),
                            );
                        } else if !self.search.hits.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("{} 处搜索结果", self.search.hits.len()))
                                    .small()
                                    .color(self.theme.weak),
                            );
                        }

                    });
                });
            });
    }

    pub fn cursor_line_col(&self) -> Option<(usize, usize)> {
        let doc = self.doc.as_ref()?;
        let idx = self.pending_cursor.unwrap_or(self.last_cursor);
        Some(doc.line_col_of_char_index(idx))
    }

    fn floating_toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        let theme = self.theme.clone();
        let dirty = self.doc.as_ref().map(|d| d.is_dirty()).unwrap_or(false);
        let small = (self.cfg.font_size * 0.72).round().max(11.0);
        egui::Area::new(egui::Id::new("easmd-floating"))
            .anchor(
                egui::Align2::RIGHT_TOP,
                egui::vec2(-14.0, self.top_bar_h + 10.0),
            )
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme.panel.gamma_multiply(0.98))
                    .stroke(egui::Stroke::new(1.0, theme.border))
                    .corner_radius(egui::CornerRadius::same(7))
                    .inner_margin(egui::Margin::symmetric(4, 3))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 2],
                        blur: 10,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(60),
                    })
                    .show(ui, |ui| {
                        let before = ui.cursor().top();
                        // 自己排版：所有项共用同一高度与顶边，避免布局交叉轴带来的错位
                        let mut tb = crate::ui_ext::Toolbar::begin(ui, &theme, small);
                        let opts = [(ViewMode::Read, "阅读"), (ViewMode::Edit, "编辑")];
                        if let Some(m) = tb.segments(ui, self.cfg.mode, &opts) {
                            self.ops.push(Op::SetMode(m));
                        }
                        if dirty && tb.button_clicked(ui, "保存", "保存 (Ctrl+S)", true) {
                            self.ops.push(Op::Save);
                        }
                        if tb.button_clicked(ui, "缩小", "缩小字体 (Ctrl+-)", false) {
                            self.ops.push(Op::ZoomOut);
                        }
                        if tb.button_clicked(ui, "放大", "放大字体 (Ctrl+=)", false) {
                            self.ops.push(Op::ZoomIn);
                        }
                        if tb.button_clicked(
                            ui,
                            "侧栏",
                            "显示 / 隐藏侧栏 (Ctrl+B)",
                            self.cfg.show_sidebar && !self.cfg.zen_mode,
                        ) {
                            self.ops.push(Op::ToggleSidebar);
                        }
                        if tb.button_clicked(
                            ui,
                            if theme.dark { "浅色" } else { "深色" },
                            "切换深色 / 浅色 (Ctrl+D)",
                            false,
                        ) {
                            self.ops.push(Op::ToggleDarkLight);
                        }
                        if tb.button_clicked(ui, "设置", "设置 (Ctrl+,)", false) {
                            self.ops.push(Op::OpenSettings);
                        }
                        tb.end(ui);
                        let after = ui.cursor().top();
                        self.floating_h = (after - before).max(28.0);
                    });
            });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(self.theme.bg))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                reader::paint_background(ui, rect, &self.theme, &self.cfg, &self.bg);

                if self.doc.is_none() {
                    welcome(ui, &self.theme, &self.cfg, &mut self.ops);
                    return;
                }

                let mode = self.cfg.mode;
                let doc = self.doc.take();
                let mut doc = doc.unwrap();

                match mode {
                    ViewMode::Read => {
                        let out = reader::show(
                            ui,
                            &mut doc,
                            &self.cfg,
                            &self.theme,
                            &mut self.cache,
                            &self.bg,
                            self.pending_scroll.take(),
                            self.floating_h + 16.0,
                            &crate::reader::ReaderOpts::default(),
                        );
                        self.handle_reader_output(&mut doc, out);
                    }
                    ViewMode::Edit => {
                        // 左：源码编辑；右：实时预览。两栏联动滚动、光标所在块高亮

                        let total = ui.available_width();
                        let gap = 9.0;
                        let editor_w =
                            ((total - gap) * self.cfg.editor_split).clamp(120.0, total - 120.0);
                        let h = ui.available_height();
                        let inset = self.floating_h + 16.0;
                        ui.horizontal(|ui| {
                            let editor_rect = ui.available_rect_before_wrap();
                            ui.allocate_ui_with_layout(
                                egui::vec2(editor_w, h),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    let cursor = doc.pending_cursor.take();
                                    let out = crate::editor::show(
                                        ui,
                                        &mut doc,
                                        &self.cfg,
                                        &self.theme,
                                        cursor,
                                        false,
                                        0.0,
                                        self.sync.editor_to.take(),
                                    );
                                    self.last_cursor = out.cursor_char;
                                    if out.changed {
                                        self.last_edit = Instant::now();
                                        doc.mark_edited();
                                    }
                                    if out.save_requested {
                                        self.ops.push(Op::Save);
                                    }
                                    self.editor_view = Some(out.view);
                                },
                            );
                            crate::ui_ext::drag_splitter(
                                ui,
                                egui::Id::new("easmd-split"),
                                total,
                                &mut self.cfg.editor_split,
                                0.15,
                                &self.theme,
                            );
                            let preview_rect = ui.available_rect_before_wrap();
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), h),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    let out = reader::show(
                                        ui,
                                        &mut doc,
                                        &self.cfg,
                                        &self.theme,
                                        &mut self.cache,
                                        &self.bg,
                                        self.pending_scroll.take(),
                                        inset,
                                        &crate::reader::ReaderOpts {
                                            scroll_to_y: self.sync.preview_to.take(),
                                            prev_anchors: &self.sync.prev_anchors,
                                            highlight: self.sync.highlight.take(),
                                        },
                                    );
                                    self.handle_reader_output(&mut doc, out);
                                },
                            );
                            self.pane_rects = Some((editor_rect, preview_rect));
                            self.sync_end_frame(&doc);
                        });
                    }
                }
                self.doc = Some(doc);
            });
    }

    /// 处理阅读面板的输出。
    ///
    /// `doc` 必须由调用方传入：渲染期间文档被 `take()` 出了 `self.doc`，
    /// 此时任何 `self.doc` 查询都是 `None`，会静默吞掉交互（曾经导致
    /// 「点链接没反应」和「点预览不跳光标」两个 bug）。
    fn handle_reader_output(&mut self, doc: &mut Document, out: reader::ReaderOutput) {
        self.last_scroll = Some(out.scroll_offset);
        self.sync.preview_scroll = out.scroll_offset;
        if !out.anchors.is_empty() {
            self.sync.prev_anchors = out.anchors;
        }
        // 在预览里点击 → 编辑器光标跳到对应的行（这样两侧始终知道对方在哪）
        if self.cfg.mode == ViewMode::Edit {
            // 优先用「被点击文字块的源码偏移」：精确到块，不受行距/空白影响；
            // 点在行间空白（没有文字块）时退回「纵坐标 → 锚点」的换算。
            let byte = out
                .clicked_text_offset
                .or_else(|| out.clicked_at_y.and_then(|y| reader::offset_for_y(&self.sync.prev_anchors, y)));
            if let Some(byte) = byte {
                let byte = doc.render_to_text_offset(byte);
                let char_index = byte_to_char(&doc.text, byte);
                doc.pending_cursor = Some(char_index);
                self.pending_cursor = Some(char_index);
                self.last_cursor = char_index;
                // 点预览 → 编辑器光标跳过去：只请求滚动一次，随后进入静默期
                if let Some(view) = &self.editor_view {
                    if let Some(line_y) = view.y_of_char(char_index) {
                        self.sync.editor_to = Some((line_y - 6.0).max(0.0));
                    }
                }
            }
        }
        // 链接点击同样要等到帧末（`follow_link` 需要 `self.doc` 在场）
        if let Some(link) = out.link_clicked {
            self.ops.push(Op::FollowLink(link));
        }
    }

    fn follow_link(&mut self, dest: &str) {
        let Some(base) = self.doc.as_ref().map(|d| d.dir()) else {
            return;
        };
        if crate::doc::is_external(dest) {
            fs_ops::open_external(dest);
            return;
        }
        let path = crate::util::resolve_relative(&base, dest);
        let anchor = dest.split('#').nth(1).map(|s| s.to_string());
        if path.exists() {
            self.open_file(path.clone());
            if let Some(a) = anchor {
                if let Some(doc) = self.doc.as_mut() {
                    doc.pending_anchor = Some(a);
                }
                self.cfg.mode = ViewMode::Read;
            }
            self.pending_scroll = if self.cfg.restore_scroll {
                let p = path.clone();
                Some(self.cfg.scroll_for(&p))
            } else {
                None
            };
        } else {
            let mut text = format!("链接指向的文件不存在：\n{}", path.display());
            if !crate::util::is_markdown(&path) {
                text.push_str("\n\n（该链接可能指向非 Markdown 文件或被移动过）");
            }
            self.dialogs.message("无法打开链接", text, true);
        }
    }

    // ------------------------------------------------------------ 生命周期

    fn apply_dialog_outcome(&mut self, ctx: &egui::Context, outcome: DialogOutcome) {
        match outcome {
            DialogOutcome::Dismissed => {}
            DialogOutcome::NewFile { parent, name } => {
                match fs_ops::create_file(&parent, &name, "") {
                    Ok(path) => {
                        self.tree.reload_ancestors(&path, &self.cfg);
                        self.tree.select(Some(path.clone()));
                        self.open_file(path);
                        self.cfg.mode = ViewMode::Edit;
                        self.toasts.ok("已创建文件");
                    }
                    Err(e) => {
                        self.dialogs.open(Dialog::NewFile {
                            parent,
                            name,
                            error: Some(e),
                        });
                    }
                }
            }
            DialogOutcome::NewFolder { parent, name } => match fs_ops::create_dir(&parent, &name) {
                Ok(path) => {
                    self.tree.reload_ancestors(&path, &self.cfg);
                    self.tree.set_expanded(&path, true, &self.cfg);
                    self.tree.select(Some(path));
                    self.toasts.ok("已创建文件夹");
                }
                Err(e) => {
                    self.dialogs.open(Dialog::NewFolder {
                        parent,
                        name,
                        error: Some(e),
                    });
                }
            },
            DialogOutcome::Confirm(action) => match action {
                ConfirmAction::Delete(paths) => self.do_delete(&paths),
                ConfirmAction::SaveAndContinue => {
                    self.save_doc_if_needed(true);
                    self.persist();
                    self.closing_saved = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ConfirmAction::CloseWithoutSaving => {
                    self.persist();
                    self.closing_saved = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ConfirmAction::OpenFolderFromMissing => self.ops.push(Op::PickFolder),
            },
        }
    }

    fn autosave_tick(&mut self) {
        if !self.cfg.autosave {
            return;
        }
        let dirty = self.doc.as_ref().map(|d| d.is_dirty()).unwrap_or(false);
        if !dirty {
            return;
        }
        if self.last_edit.elapsed() > Duration::from_secs(3) {
            self.save_doc_if_needed(true);
            self.last_save = Instant::now();
        }
    }

    fn remember_window(&mut self, ctx: &egui::Context) {
        let info = ctx.input(|i| {
            let vp = i.viewport();
            (
                vp.outer_rect,
                vp.maximized.unwrap_or(false),
                vp.minimized.unwrap_or(false),
            )
        });
        if let Some(r) = info.0 {
            if !info.1 && !info.2 {
                self.cfg.window.width = r.width();
                self.cfg.window.height = r.height();
                self.cfg.window.x = r.left();
                self.cfg.window.y = r.top();
            }
        }
        self.cfg.window.maximized = info.1;
    }
}

/// 把主题写入 egui 的明暗两套样式（本程序完全接管配色，两套保持一致）。
pub(crate) fn apply_style_to_ctx(
    ctx: &egui::Context,
    theme: &Theme,
    cfg: &AppConfig,
    line_gap_supported: bool,
) {
    let text_styles = theme.text_styles(cfg);
    // 字体不支持改行距时，至少让编辑器（TextEdit）的行距可调
    let extra_line_spacing = if line_gap_supported {
        0.0
    } else {
        (cfg.line_height - 1.32).max(0.0) * cfg.font_size * 0.5
    };
    ctx.all_styles_mut(|style| {
        theme.apply(style, cfg);
        style.text_styles = text_styles.clone();
        style.spacing.extra_text_line_spacing = extra_line_spacing;
    });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl App {
    /// 一帧的全部界面与状态推进（`eframe::App::ui` 的实体，测试可直接调用）。
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        // ---- 一次性初始化 ----
        self.poll_font_scan(ctx);
        if self.need_fonts {
            self.apply_style(ctx);
        }
        // 编辑停顿后刷新大纲与链接索引（避免每次按键都重新解析）
        if self.last_edit.elapsed() > Duration::from_millis(700) {
            if let Some(doc) = self.doc.as_mut() {
                doc.refresh_outline_if_needed();
            }
        }
        if self.need_restyle {
            self.refresh_theme(ctx);
        }

        // ---- 背景图片（按窗口大小缩放，控制内存） ----
        let max_px = {
            let r = ctx.content_rect();
            let ppp = ctx.pixels_per_point();
            ((r.width().max(r.height()) * ppp * 1.5) as u32).clamp(640, 4096)
        };
        let bg_path = self.cfg.bg_image.clone();
        self.bg.update(ctx, bg_path.as_deref(), max_px);

        // ---- 事件轮询 ----
        self.poll_dropped_files(ctx);
        self.poll_watcher();
        self.search.poll();
        self.autosave_tick();

        // ---- 关闭前保存 ----
        if ctx.input(|i| i.viewport().close_requested()) && !self.closing_saved {
            let dirty = self.doc.as_ref().map(|d| d.is_dirty()).unwrap_or(false);
            if dirty && !self.cfg.autosave {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.dialogs.open(Dialog::CloseUnsaved);
            } else {
                self.closing_saved = true;
                self.save_doc_if_needed(true);
                self.remember_window(ctx);
                self.persist();
            }
        }

        // ---- 快捷键 ----
        self.handle_shortcuts(ctx);
        let keys = self.nav_keys(ctx);
        if keys != NavKeys::NONE {
            explorer::keyboard_nav(
                &self.cfg,
                &mut self.tree,
                &mut self.explorer,
                keys,
                &mut self.ops,
            );
        }

        // ---- 面板 ----
        let zen = self.cfg.zen_mode;
        if !zen {
            self.top_bar(ui);
        }

        if self.cfg.show_sidebar && !zen {
            let panel = egui::Panel::left("easmd-sidebar")
                .resizable(true)
                .default_size(self.cfg.sidebar_width)
                .size_range(190.0..=720.0)
                .frame(
                    egui::Frame::new()
                        .fill(self.theme.panel)
                        .inner_margin(egui::Margin::symmetric(8, 8))
                        .stroke(egui::Stroke::new(1.0, self.theme.border)),
                )
                .show(ui, |ui| {
                    let headings: Vec<crate::doc::Heading> = self
                        .doc
                        .as_ref()
                        .map(|d| d.headings.clone())
                        .unwrap_or_default();
                    let doc_path = self.doc.as_ref().map(|d| d.path.clone());
                    explorer::show(
                        ui,
                        &self.cfg,
                        &self.theme,
                        &mut self.tree,
                        &mut self.explorer,
                        &mut self.search,
                        &headings,
                        doc_path.as_deref(),
                        &mut self.ops,
                    );
                });
            self.cfg.sidebar_width = panel.response.rect.width().max(190.0);
        }

        if !zen {
            self.status_bar(ui);
        }

        self.central(ui);
        self.floating_toolbar(ui);

        // ---- 设置窗口 ----
        if self.show_settings {
            let bg_err = self.bg.error.clone();
            let families_ready = !self.families.is_empty();
            let fams = self.families.clone();
            let change = settings::show(
                ctx,
                &mut self.cfg,
                &mut self.show_settings,
                &self.theme,
                &fams,
                families_ready,
                &mut self.settings_snap,
                bg_err.as_deref(),
            );
            if change.restyle {
                self.refresh_theme(ctx);
            }
            if change.fonts {
                self.need_fonts = true;
                self.install_fonts(ctx);
            }
            if change.bg_image {
                let p = self.cfg.bg_image.clone();
                self.bg
                    .update(ctx, p.as_deref(), 2048);
            }
            if change.close {
                self.show_settings = false;
            }
        }

        // ---- 对话框 ----
        if self.dialogs.is_open() {
            if let Some(outcome) = self.dialogs.show(ctx, &self.theme) {
                self.apply_dialog_outcome(ctx, outcome);
            }
        }

        // ---- 提示条 ----
        self.toasts.show(ctx, &self.theme);


        // ---- 窗口标题：随当前文档变化 ----
        let title = match &self.doc {
            Some(doc) => format!("{} — EasyMD", doc.name()),
            None => "EasyMD".to_string(),
        };
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        // ---- 帧末操作 ----
        self.apply_ops(ctx);
        self.remember_window(ctx);
        self.cfg.window.maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

        // 滚动位置定期落盘（避免每帧写盘）
        if self.last_save.elapsed() > Duration::from_secs(20) {
            self.persist();
            self.last_save = Instant::now();
        }
    }
}

/// 字符索引 → 字节偏移。
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// 字节偏移 → 字符索引（偏移非法时收拢到最近的字符边界，绝不 panic）。
fn byte_to_char(text: &str, byte: usize) -> usize {
    let byte = crate::doc::clamp_to_char_boundary(text, byte);
    text[..byte].chars().count()
}

/// 欢迎页（未打开文档时）。
fn welcome(ui: &mut egui::Ui, theme: &Theme, cfg: &AppConfig, ops: &mut Vec<Op>) {
    let _ = cfg;
    ui.add_space(ui.available_height() * 0.18);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("EasyMD")
                .size(34.0)
                .strong()
                .color(theme.accent),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("轻量、流畅的本地 Markdown 阅读与知识库管理工具")
                .color(theme.weak),
        );
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            let w = ui.available_width();
            ui.add_space((w - 340.0).max(0.0) * 0.5);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("  打开文档仓库  ").size(16.0))
                        .min_size(egui::vec2(150.0, 38.0)),
                )
                .clicked()
            {
                ops.push(Op::PickFolder);
            }
            ui.add_space(10.0);
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("  打开单个文件  ").size(16.0))
                        .min_size(egui::vec2(150.0, 38.0)),
                )
                .clicked()
            {
                ops.push(Op::PickFile);
            }
        });
        ui.add_space(26.0);
        ui.label(
            egui::RichText::new(
                "也可以直接把 Markdown 文件或整个文件夹拖进窗口\n支持 Windows / Linux · 自动记忆上次打开的仓库与阅读位置",
            )
            .color(theme.weak)
            .small(),
        );
    });
}

#[cfg(test)]
mod link_navigation_tests {
    use super::*;

    /// 端到端：完整 App（含悬浮工具栏浮层）里点击正文链接，
    /// 目标文档必须真的被打开。
    ///
    /// 这条测试盯的是一个真实 bug：`central()` 渲染前会把文档 `take()` 出
    /// `self.doc`，此时 `follow_link` 查 `self.doc` 得到 `None` 就直接返回，
    /// 于是「点链接毫无反应」。只有应用级测试能挡住这类静默失败。
    #[test]
    fn clicking_a_body_link_opens_the_target_document() {
        let dir = std::env::temp_dir().join(format!("easymd-app-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.md");
        std::fs::write(&main, "# 标题\n\n[本地链接](note.md) 在这里。\n").unwrap();
        let note = dir.join("note.md");
        std::fs::write(&note, "# 目标文档\n\n正文。\n").unwrap();

        let ctx = egui::Context::default();
        let mut cfg = AppConfig::default();
        cfg.watch_files = false;
        cfg.autosave = false;
        cfg.restore_scroll = false;
        cfg.show_sidebar = false;
        cfg.mode = ViewMode::Read;
        cfg.last_folder = Some(dir.clone());
        cfg.open_file = Some(main.clone());
        cfg.window.width = 900.0;
        cfg.window.height = 600.0;

        let mut app = App::from_ctx(&ctx, cfg, None);
        app.config_path = dir.join("cfg.json");
        assert_eq!(
            app.doc.as_ref().map(|d| d.path.clone()),
            Some(main.clone()),
            "启动后应打开 main.md"
        );

        let mut frames = 0usize;
        let mut clicked = false;
        let xs = [22.0f32, 26.0, 30.0, 34.0, 38.0, 44.0, 52.0, 64.0, 80.0, 100.0, 130.0];
        'scan: for y in (30..300).step_by(3) {
            for x in xs {
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
                run_frame(&ctx, &mut app, vec![mv.clone()]);
                run_frame(&ctx, &mut app, vec![mv, down]);
                run_frame(&ctx, &mut app, vec![up]);
                frames += 3;
                assert!(
                    app.doc.is_some(),
                    "任何一帧结束后都必须恢复当前文档（第 {frames} 帧丢了文档）"
                );
                if app.doc.as_ref().map(|d| d.path == note).unwrap_or(false) {
                    clicked = true;
                    break 'scan;
                }
            }
        }
        assert!(
            clicked,
            "点击正文里的链接应打开 note.md（扫描 {frames} 帧后仍是 {:?}）",
            app.doc.as_ref().map(|d| d.path.clone())
        );
    }

    /// 端到端：编辑模式下点预览面板，编辑器光标要跳到对应行。
    /// 与上一条测试同源：这段逻辑以前查 `self.doc`，而渲染期间它正是 `None`。
    #[test]
    fn clicking_the_preview_moves_the_editor_cursor() {
        let dir = std::env::temp_dir().join(format!("easymd-app-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("long.md");
        let mut text = String::from("# 长文档

");
        for i in 1..=40 {
            text.push_str(&format!("第 {i} 行：这里是一些用来撑开版面的正文。

"));
        }
        std::fs::write(&main, text).unwrap();

        let ctx = egui::Context::default();
        let mut cfg = AppConfig::default();
        cfg.watch_files = false;
        cfg.autosave = false;
        cfg.restore_scroll = false;
        cfg.show_sidebar = false;
        cfg.mode = ViewMode::Edit;
        cfg.last_folder = Some(dir.clone());
        cfg.open_file = Some(main.clone());

        let mut app = App::from_ctx(&ctx, cfg, None);
        app.config_path = dir.join("cfg.json");
        run_frame(&ctx, &mut app, vec![]);
        run_frame(&ctx, &mut app, vec![]);

        // 点右侧预览面板的中下部（编辑区占左侧一半），那里一定有正文
        let pos = egui::pos2(675.0, 330.0);
        let mv = egui::Event::PointerMoved(pos);
        run_frame(&ctx, &mut app, vec![mv.clone()]);
        run_frame(
            &ctx,
            &mut app,
            vec![
                mv.clone(),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        run_frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );

        assert!(
            app.pending_cursor.is_some(),
            "点预览应把编辑区光标定位过去，但没有产生光标请求"
        );
        assert!(
            app.last_cursor > 20,
            "点在中下部，光标应落在文档靠后的位置，实际只到字符 {}",
            app.last_cursor
        );
    }

    fn run_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 600.0),
        ));
        input.events = events;
        let mut full = ctx.run_ui(input, |ui| app.draw(ui));
        full.textures_delta.clear();
        app.apply_ops(ctx);
    }
}

#[cfg(test)]
mod drag_select_scroll_tests {
    use super::*;

    fn run_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> f32 {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 600.0),
        ));
        input.events = events;
        let mut full = ctx.run_ui(input, |ui| app.draw(ui));
        full.textures_delta.clear();
        // 编辑区第一行文字的屏幕 y：视图下滚时它会往上移，用来判断有没有滚动
        let mut top = f32::MAX;
        for clipped in &full.shapes {
            collect_top(&clipped.shape, &mut top);
        }
        app.apply_ops(ctx);
        top
    }

    /// 编辑区正文那个「很高的」文本块（整篇文档是一个 galley）的屏幕 y。
    /// 视图往下滚时它会往上移。（工具栏的文字是单行，靠高度就能排除掉。）
    fn collect_top(shape: &egui::Shape, top: &mut f32) {
        match shape {
            egui::Shape::Text(t) => {
                if t.pos.x < 400.0 && t.galley.size().y > 200.0 {
                    *top = if *top == f32::MAX {
                        t.pos.y
                    } else {
                        (*top).min(t.pos.y)
                    };
                }
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    collect_top(s, top);
                }
            }
            _ => {}
        }
    }

    fn long_doc(dir: &std::path::Path) -> PathBuf {
        let mut text = String::from("# 长文档\n\n");
        for i in 1..=80 {
            text.push_str(&format!("第 {i} 行：用来撑开编辑器高度的正文内容。\n"));
        }
        let p = dir.join("long.md");
        std::fs::write(&p, text).unwrap();
        p
    }

    /// 在编辑器里按下并拖动；返回 (按下后的首行 y, 拖动后的首行 y, 按下后的选区终点, 拖动后的选区终点)。
    /// 全部用「相对变化」做断言，避免依赖具体字体度量（Windows 与 Linux 的字面宽度不同）。
    fn drag_from_to(
        ctx: &egui::Context,
        app: &mut App,
        doc: &PathBuf,
        xs: f32,
        start_y: f32,
        targets: &[f32],
    ) -> (f32, f32, usize, usize) {
        let id = egui::Id::new(("easmd-editor", doc));
        let sel_end = |ctx: &egui::Context| {
            egui::TextEdit::load_state(ctx, id)
                .and_then(|s| s.cursor.char_range())
                .map(|r| r.primary.index.0)
                .unwrap_or(0)
        };
        let start = egui::pos2(xs, start_y);
        run_frame(ctx, app, vec![egui::Event::PointerMoved(start)]);
        let top_at_press = run_frame(
            ctx,
            app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        let sel_at_press = sel_end(ctx);
        let mut top = top_at_press;
        for y in targets {
            top = run_frame(ctx, app, vec![egui::Event::PointerMoved(egui::pos2(xs, *y))]);
        }
        let sel_after = sel_end(ctx);
        run_frame(
            ctx,
            app,
            vec![egui::Event::PointerButton {
                pos: egui::pos2(xs, *targets.last().unwrap_or(&start_y)),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        (top_at_press, top, sel_at_press, sel_after)
    }

    fn edit_mode_app(dir: &std::path::Path, main: &PathBuf) -> (egui::Context, App) {
        let ctx = egui::Context::default();
        let mut cfg = AppConfig::default();
        cfg.watch_files = false;
        cfg.autosave = false;
        cfg.show_sidebar = false;
        cfg.mode = ViewMode::Edit;
        cfg.last_folder = Some(dir.to_path_buf());
        cfg.open_file = Some(main.clone());
        let mut app = App::from_ctx(&ctx, cfg, None);
        app.config_path = dir.join("cfg.json");
        run_frame(&ctx, &mut app, vec![]);
        run_frame(&ctx, &mut app, vec![]);
        (ctx, app)
    }

    /// 编辑模式下拖拽选择：指针越过可视区域时视图必须跟着滚，
    /// 否则选区虽然在增长却看不见，长文档没法一次选完。
    #[test]
    fn drag_select_autoscrolls_past_the_viewport() {
        let dir = std::env::temp_dir().join(format!("easymd-drag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = long_doc(&dir);
        let (ctx, mut app) = edit_mode_app(&dir, &main);

        // 拖到窗口下方（视口之外）
        let targets: Vec<f32> = (1..=6).map(|i| 600.0 + i as f32 * 40.0).collect();
        let (top0, top, sel0, sel_end) =
            drag_from_to(&ctx, &mut app, &main, 60.0, 300.0, &targets);
        println!("拖出视口: 首行 y {top0:.1} → {top:.1}，选区 {sel0} → {sel_end}");

        assert!(
            top < top0 - 20.0,
            "指针拖到视口下方时视图应当滚动（内容上移），实际 {top0:.1} → {top:.1}"
        );
        assert!(
            sel_end > sel0 + 50,
            "滚动过程中选区应继续增长，实际 {sel0} → {sel_end}"
        );
    }

    /// 视口内拖动不应该触发自动滚动（否则正常选词会乱跳）。
    #[test]
    fn drag_inside_viewport_does_not_autoscroll() {
        let dir = std::env::temp_dir().join(format!("easymd-drag-in-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = long_doc(&dir);
        let (ctx, mut app) = edit_mode_app(&dir, &main);

        let targets: Vec<f32> = (1..=6).map(|i| 300.0 + i as f32 * 30.0).collect();
        let (top0, top, sel0, sel_end) =
            drag_from_to(&ctx, &mut app, &main, 60.0, 300.0, &targets);
        println!("视口内拖动: 首行 y {top0:.1} → {top:.1}，选区 {sel0} → {sel_end}");

        assert!(
            (top - top0).abs() < 1.0,
            "视口内拖动不应滚动视图，实际 {top0:.1} → {top:.1}"
        );
        assert!(
            sel_end > sel0,
            "视口内拖动仍应正常选区，实际 {sel0} → {sel_end}"
        );
    }
}
