// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 仓库文件监听：外部改动（编辑器、同步工具等）时自动刷新目录树与当前文档。
//! 使用 debounce 合并密集事件，避免频繁刷新。

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher as NotifyWatcher};

pub struct Watcher {
    _inner: notify::RecommendedWatcher,
    rx: Receiver<notify::Result<notify::Event>>,
    pending: Vec<PathBuf>,
    last_event: Instant,
    pub enabled: bool,
}

impl Watcher {
    pub fn new(root: &std::path::Path) -> Option<Self> {
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .ok()?;
        watcher.watch(root, RecursiveMode::Recursive).ok()?;
        Some(Self {
            _inner: watcher,
            rx,
            pending: Vec::new(),
            last_event: Instant::now(),
            enabled: true,
        })
    }

    /// 收集事件并做防抖：安静 400ms 后才认为改动稳定。
    /// 返回本次需要刷新的路径（已去重）。
    pub fn poll(&mut self) -> Vec<PathBuf> {
        if !self.enabled {
            return Vec::new();
        }
        let mut got = false;
        while let Ok(ev) = self.rx.try_recv() {
            if let Ok(ev) = ev {
                for p in ev.paths {
                    self.pending.push(p);
                }
                got = true;
            }
        }
        if got {
            self.last_event = Instant::now();
            return Vec::new();
        }
        if self.pending.is_empty() {
            return Vec::new();
        }
        if self.last_event.elapsed() < Duration::from_millis(400) {
            return Vec::new();
        }
        let mut out: Vec<PathBuf> = std::mem::take(&mut self.pending);
        // 过滤掉临时文件
        out.retain(|p| {
            let n = crate::util::file_name(p);
            !n.ends_with("easymd-tmp") && !n.ends_with(".swp") && !n.ends_with('~')
        });
        out.sort();
        out.dedup();
        out.truncate(64);
        out
    }
}
