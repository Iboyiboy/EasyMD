// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! EasyMD —— 超轻量化的本地 Markdown 知识库阅读与管理工具。
//!
//! 用法：
//!   easymd              打开上次的仓库
//!   easymd <文件夹>      指定仓库
//!   easymd <文件.md>     打开文件（自动以其所在目录为仓库）

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod dialogs;
mod doc;
mod editor;
mod explorer;
mod fonts;
mod fs_ops;
mod icon;
mod loader;
mod reader;
mod search;
mod settings;
mod theme;
mod tree;
mod ui_ext;
mod util;
mod watch;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let cfg = config::AppConfig::load();
    let cli_path = parse_args();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("EasyMD")
        .with_inner_size([cfg.window.width, cfg.window.height])
        .with_min_inner_size([720.0, 480.0])
        .with_app_id("easymd");
    if cfg.window.x >= 0.0 && cfg.window.y >= 0.0 {
        viewport = viewport.with_position([cfg.window.x, cfg.window.y]);
    }
    if cfg.window.maximized {
        viewport = viewport.with_maximized(true);
    }
    viewport = viewport.with_icon(app_icon());

    let options = eframe::NativeOptions {
        viewport,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };

    eframe::run_native(
        "EasyMD",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, cfg, cli_path)))),
    )
}

fn parse_args() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        let s = a.to_string_lossy().to_string();
        if s.starts_with('-') {
            continue;
        }
        let p = PathBuf::from(a);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// 窗口图标：与 exe 文件图标共用 `src/icon.rs` 的绘制代码（文件图标见 `build.rs`）。
fn app_icon() -> egui::IconData {
    const S: usize = 64;
    egui::IconData {
        rgba: icon::draw(S),
        width: S as u32,
        height: S as u32,
    }
}
