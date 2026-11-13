// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 仓库内全文检索：后台线程扫描，边搜边出结果，避免界面卡顿。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Hit {
    pub path: PathBuf,
    pub line: usize,
    /// 命中行的文本（已截断）
    pub preview: String,
    /// 命中片段在预览中的字符范围
    pub hit_range: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct SearchProgress {
    pub scanned: usize,
    pub hits: usize,
    pub finished: bool,
    pub message: Option<String>,
}

pub struct SearchState {
    pub query: String,
    pub hits: Vec<Hit>,
    pub progress: SearchProgress,
    /// 是否区分大小写
    pub case_sensitive: bool,
    running: Option<Running>,
    pub scope: PathBuf,
    pub selected: usize,
}

struct Running {
    cancel: Arc<Mutex<bool>>,
    rx: Receiver<Msg>,
}

enum Msg {
    Hit(Hit),
    Progress(usize, usize),
    Done(usize, usize, Option<String>),
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            hits: Vec::new(),
            progress: SearchProgress {
                scanned: 0,
                hits: 0,
                finished: true,
                message: None,
            },
            case_sensitive: false,
            running: None,
            scope: PathBuf::new(),
            selected: 0,
        }
    }
}

impl SearchState {
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn start(&mut self, root: PathBuf, query: String, case_sensitive: bool, limit: usize) {
        self.cancel();
        self.query = query.clone();
        self.case_sensitive = case_sensitive;
        self.hits.clear();
        self.selected = 0;
        self.progress = SearchProgress {
            scanned: 0,
            hits: 0,
            finished: false,
            message: None,
        };
        self.scope = root.clone();
        if query.trim().is_empty() {
            self.progress.finished = true;
            return;
        }
        let (tx, rx) = channel::<Msg>();
        let cancel = Arc::new(Mutex::new(false));
        let cancel_thread = cancel.clone();
        let q = query.clone();
        std::thread::Builder::new()
            .name("easymd-search".into())
            .spawn(move || {
                run_search(root, q, case_sensitive, limit, tx, cancel_thread);
            })
            .ok();
        self.running = Some(Running { cancel, rx });
    }

    pub fn cancel(&mut self) {
        if let Some(r) = self.running.take() {
            if let Ok(mut c) = r.cancel.lock() {
                *c = true;
            }
        }
        if !self.progress.finished {
            self.progress.finished = true;
        }
    }

    /// 每帧调用：收取后台线程的结果。
    pub fn poll(&mut self) {
        let mut finished = false;
        if let Some(running) = &self.running {
            while let Ok(msg) = running.rx.try_recv() {
                match msg {
                    Msg::Hit(h) => {
                        self.hits.push(h);
                        self.progress.hits = self.hits.len();
                    }
                    Msg::Progress(scanned, hits) => {
                        self.progress.scanned = scanned;
                        self.progress.hits = hits;
                    }
                    Msg::Done(scanned, hits, msg) => {
                        self.progress.scanned = scanned;
                        self.progress.hits = hits;
                        self.progress.message = msg;
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.progress.finished = true;
            self.running = None;
        }
    }
}

fn run_search(
    root: PathBuf,
    query: String,
    case_sensitive: bool,
    limit: usize,
    tx: Sender<Msg>,
    cancel: Arc<Mutex<bool>>,
) {
    let needle = if case_sensitive {
        query.clone()
    } else {
        query.to_lowercase()
    };
    let mut files: Vec<PathBuf> = Vec::new();
    collect(&root, &mut files, 20_000, &cancel);
    let total = files.len();
    let mut scanned = 0usize;
    let mut hits = 0usize;
    let mut last_report = std::time::Instant::now();
    for f in files {
        if cancel.lock().map(|c| *c).unwrap_or(false) {
            let _ = tx.send(Msg::Done(scanned, hits, Some("已取消".into())));
            return;
        }
        scanned += 1;
        let Ok(bytes) = std::fs::read(&f) else { continue };
        if bytes.len() > 4 * 1024 * 1024 {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else { continue };
        for (i, line) in text.lines().enumerate() {
            let hay = if case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };
            if let Some(pos) = hay.find(&needle) {
                // 全部按「字符」计算，避免小写化改变字节长度导致切片越界
                let line_chars: Vec<char> = line.chars().collect();
                let hit_start = hay[..pos.min(hay.len())].chars().count();
                let lead = line_chars.len() - line.trim_start().chars().count();
                let start = hit_start
                    .saturating_sub(lead)
                    .min(line_chars.len());
                let hit_len = query.chars().count();
                let end = (start + hit_len).min(line_chars.len());
                let preview = crate::util::elide(line.trim(), 160);
                let _ = tx.send(Msg::Hit(Hit {
                    path: f.clone(),
                    line: i,
                    preview,
                    hit_range: (start, end),
                }));
                hits += 1;
                if hits >= limit {
                    let _ = tx.send(Msg::Done(scanned, hits, Some("结果过多，已截断".into())));
                    return;
                }
                break; // 每个文件只报第一条命中，避免刷屏
            }
        }
        if last_report.elapsed().as_millis() > 120 {
            last_report = std::time::Instant::now();
            let _ = tx.send(Msg::Progress(scanned, hits));
        }
    }
    let _ = tx.send(Msg::Done(scanned, total, None));
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, limit: usize, cancel: &Arc<Mutex<bool>>) {
    if out.len() >= limit {
        return;
    }
    if cancel.lock().map(|c| *c).unwrap_or(false) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = crate::util::file_name(&path);
            // 跳过常见的大体积/无关目录
            if matches!(
                name.as_str(),
                ".git" | "node_modules" | "target" | ".svn" | ".hg" | "__pycache__" | ".idea" | ".vscode" | "dist" | "build"
            ) {
                continue;
            }
            if name.starts_with('.') {
                continue;
            }
            collect(&path, out, limit, cancel);
        } else if crate::util::is_markdown(&path) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例用独立目录，避免并发测试互相删除
    fn temp_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("easymd-search-{tag}-{}", std::process::id()));
        let sub = dir.join("子目录");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            dir.join("a.md"),
            "# 标题\n\n这里提到了 cargo build 命令。\n其他内容\n",
        )
        .unwrap();
        std::fs::write(sub.join("b.md"), "第二篇\nCargo 大写也要能搜到\n").unwrap();
        dir
    }

    fn wait_done(st: &mut SearchState) {
        let start = std::time::Instant::now();
        while st.is_running() && start.elapsed().as_secs() < 10 {
            st.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        st.poll();
    }

    #[test]
    fn finds_matches_case_insensitively() {
        let repo = temp_repo("ci");
        let mut st = SearchState::default();
        st.start(repo.clone(), "cargo".to_string(), false, 100);
        wait_done(&mut st);
        assert!(st.progress.finished, "检索应当结束");
        assert_eq!(st.hits.len(), 2, "两篇文档各命中一次");
        let names: Vec<String> = st
            .hits
            .iter()
            .map(|h| crate::util::file_name(&h.path))
            .collect();
        assert!(names.contains(&"a.md".to_string()));
        assert!(names.contains(&"b.md".to_string()));
        let b = st.hits.iter().find(|h| h.path.ends_with("b.md")).unwrap();
        assert_eq!(b.line, 1, "命中行号应为第 2 行（0 基为 1）");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn respects_case_sensitive_flag() {
        let repo = temp_repo("cs");
        let mut st = SearchState::default();
        st.start(repo.clone(), "Cargo".to_string(), true, 100);
        wait_done(&mut st);
        assert_eq!(st.hits.len(), 1, "区分大小写时只应命中 b.md");
        assert!(st.hits[0].path.ends_with("b.md"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn empty_query_finishes_immediately() {
        let mut st = SearchState::default();
        st.start(PathBuf::from("."), "   ".to_string(), false, 100);
        assert!(st.progress.finished);
        assert!(st.hits.is_empty());
    }
}
