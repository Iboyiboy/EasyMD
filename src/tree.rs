//! 文档仓库目录树：按需加载（懒加载）、排序、过滤、扁平化供虚拟列表渲染。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::{AppConfig, SortBy};
use crate::util;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
    pub size: u64,
    /// 目录是否已读取过子项
    pub loaded: bool,
    pub expanded: bool,
    pub children: Vec<Entry>,
    pub error: Option<String>,
}

impl Entry {
    fn new(path: PathBuf, is_dir: bool) -> Self {
        let name = util::file_name(&path);
        Self {
            path,
            name,
            is_dir,
            modified: None,
            size: 0,
            loaded: false,
            expanded: false,
            children: Vec::new(),
            error: None,
        }
    }
}

/// 扁平化后的一行，供虚拟列表绘制。
#[derive(Debug, Clone)]
pub struct FlatRow {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    pub has_children: bool,
    /// 搜索模式下的附属说明（相对路径）
    pub hint: Option<String>,
    pub matched: bool,
    pub error: Option<String>,
    pub modified: Option<SystemTime>,
}

pub struct Tree {
    pub root: PathBuf,
    pub root_entry: Entry,
    pub selected: Option<PathBuf>,
    /// 搜索关键词（文件名快速搜索）
    pub query: String,
    flat_cache: Option<Vec<FlatRow>>,
    /// 搜索模式下的扁平结果
    search_cache: Option<Vec<FlatRow>>,
}

impl Tree {
    pub fn new(root: PathBuf, cfg: &AppConfig, expanded: &[PathBuf]) -> Self {
        let mut root_entry = Entry::new(root.clone(), true);
        root_entry.expanded = true;
        load_children(&mut root_entry, cfg);
        let expanded_set: HashSet<PathBuf> = expanded.iter().cloned().collect();
        apply_expanded(&mut root_entry, &expanded_set, cfg);
        Self {
            root,
            root_entry,
            selected: None,
            query: String::new(),
            flat_cache: None,
            search_cache: None,
        }
    }

    pub fn invalidate(&mut self) {
        self.flat_cache = None;
        self.search_cache = None;
    }

    pub fn set_query(&mut self, q: String) {
        if self.query != q {
            self.query = q;
            self.invalidate();
        }
    }

    pub fn is_searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// 当前展开的目录集合（用于写入配置）。
    pub fn expanded_paths(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        collect_expanded(&self.root_entry, &mut out);
        out.truncate(400);
        out
    }

    // ------------------------------------------------------------ 查询

    pub fn find(&self, path: &Path) -> Option<&Entry> {
        find_entry(&self.root_entry, path)
    }

    pub fn find_mut(&mut self, path: &Path) -> Option<&mut Entry> {
        find_entry_mut(&mut self.root_entry, path)
    }

    fn rebuild(&mut self, cfg: &AppConfig) {
        let mut rows = Vec::with_capacity(256);
        // 根目录本身不出现在列表里（界面上的空白区域即代表它），
        // 但它同样可以被折叠，折叠后列表为空。
        if self.root_entry.expanded {
            for child in &self.root_entry.children {
                flatten(child, 0, cfg, &mut rows, "", false);
            }
        }
        self.flat_cache = Some(rows);

        if self.is_searching() {
            let q = self.query.trim().to_lowercase();
            let mut out = Vec::new();
            let mut count = 0usize;
            collect_search(
                &self.root_entry,
                &self.root,
                &q,
                cfg,
                &mut out,
                &mut count,
                2000,
            );
            self.search_cache = Some(out);
        } else {
            self.search_cache = None;
        }
    }

    /// 取得当前要显示的行（文件模式或搜索模式）。
    ///
    /// 为了在绘制过程中自由修改其它状态，这里把行数据「取走」，
    /// 绘制结束后由 [`put_rows`](Self::put_rows) 放回，避免每帧克隆。
    pub fn take_rows(&mut self, cfg: &AppConfig) -> Vec<FlatRow> {
        if self.flat_cache.is_none() {
            self.rebuild(cfg);
        }
        if self.is_searching() {
            self.search_cache.take().unwrap_or_default()
        } else {
            self.flat_cache.take().unwrap_or_default()
        }
    }

    pub fn put_rows(&mut self, rows: Vec<FlatRow>, cfg: &AppConfig) {
        if self.flat_cache.is_none() && !self.is_searching() {
            self.flat_cache = Some(rows);
        } else if self.search_cache.is_none() && self.is_searching() {
            self.search_cache = Some(rows);
        } else if self.flat_cache.is_none() && self.is_searching() {
            // 搜索状态下若文件缓存尚未建立，忽略
            let _ = rows;
            let _ = cfg;
        }
    }

    // ------------------------------------------------------------ 展开 / 折叠

    pub fn toggle_expand(&mut self, path: &Path, cfg: &AppConfig) {
        let mut need_load = None;
        if let Some(e) = self.find_mut(path) {
            e.expanded = !e.expanded;
            if e.expanded && !e.loaded {
                need_load = Some(e.path.clone());
            }
        }
        if let Some(p) = need_load {
            self.load_dir(&p, cfg);
        }
        self.invalidate();
    }

    pub fn set_expanded(&mut self, path: &Path, expanded: bool, cfg: &AppConfig) {
        let mut need_load = None;
        if let Some(e) = self.find_mut(path) {
            e.expanded = expanded;
            if expanded && !e.loaded {
                need_load = Some(e.path.clone());
            }
        }
        if let Some(p) = need_load {
            self.load_dir(&p, cfg);
        }
        self.invalidate();
    }

    pub fn expand_all(&mut self, cfg: &AppConfig) {
        self.expand_recursive(&self.root.clone(), cfg);
        self.invalidate();
    }

    fn expand_recursive(&mut self, path: &Path, cfg: &AppConfig) {
        self.load_dir(path, cfg);
        let children: Vec<PathBuf> = match self.find(path) {
            Some(e) => e
                .children
                .iter()
                .filter(|c| c.is_dir)
                .map(|c| c.path.clone())
                .collect(),
            None => Vec::new(),
        };
        if let Some(e) = self.find_mut(path) {
            e.expanded = true;
        }
        for c in children {
            self.expand_recursive(&c, cfg);
        }
    }

    pub fn collapse_all(&mut self) {
        collapse_recursive(&mut self.root_entry);
        self.root_entry.expanded = true;
        self.invalidate();
    }

    /// 读取某个目录（懒加载），保留已有的展开状态。
    pub fn load_dir(&mut self, path: &Path, cfg: &AppConfig) {
        let old_expanded: HashSet<PathBuf> = {
            let mut set = HashSet::new();
            if let Some(e) = self.find(path) {
                for c in &e.children {
                    if c.is_dir && c.expanded {
                        set.insert(c.path.clone());
                    }
                }
            }
            set
        };
        if let Some(e) = self.find_mut(path) {
            load_children(e, cfg);
            for c in &mut e.children {
                if old_expanded.contains(&c.path) {
                    c.expanded = true;
                }
            }
        }
        // 之前展开过的子目录内容也需要重新拉取
        let subs: Vec<PathBuf> = old_expanded.into_iter().collect();
        for s in subs {
            self.load_dir(&s, cfg);
        }
        self.invalidate();
    }

    /// 所有已加载目录重新读取（外部改动 / 手动刷新）。
    pub fn reload_all(&mut self, cfg: &AppConfig) {
        let mut dirs = Vec::new();
        collect_loaded_dirs(&self.root_entry, &mut dirs);
        for d in dirs {
            self.load_dir(&d, cfg);
        }
        self.invalidate();
    }

    /// 沿路径向上刷新（文件新建/删除后使用，代价很小）。
    pub fn reload_ancestors(&mut self, path: &Path, cfg: &AppConfig) {
        let mut cur = if path.is_dir() {
            Some(path.to_path_buf())
        } else {
            path.parent().map(|p| p.to_path_buf())
        };
        while let Some(p) = cur {
            if self.find(&p).is_some() {
                self.load_dir(&p, cfg);
            }
            if p == self.root {
                break;
            }
            cur = p.parent().map(|x| x.to_path_buf());
        }
        self.invalidate();
    }

    pub fn select(&mut self, path: Option<PathBuf>) {
        self.selected = path;
    }

    /// 选中某文件并展开其所有父目录（用于跳转、历史前进后退）。
    pub fn reveal(&mut self, path: &Path, cfg: &AppConfig) {
        let mut chain: Vec<PathBuf> = Vec::new();
        let mut cur = path.parent().map(|p| p.to_path_buf());
        while let Some(p) = cur {
            chain.push(p.clone());
            if p == self.root {
                break;
            }
            cur = p.parent().map(|x| x.to_path_buf());
        }
        chain.reverse();
        for dir in chain {
            self.load_dir(&dir, cfg);
            if let Some(e) = self.find_mut(&dir) {
                e.expanded = true;
            }
        }
        self.select(Some(path.to_path_buf()));
        self.invalidate();
    }

}

/// 统计目录下的 Markdown 文件数量（按文件系统实际内容，带数量上限）。
pub fn count_markdown_files(root: &Path, limit: usize) -> usize {
    fn walk(dir: &Path, limit: usize, count: &mut usize, depth: usize) {
        if *count >= limit || depth > 24 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                walk(&entry.path(), limit, count, depth + 1);
            } else if util::is_markdown(&entry.path()) {
                *count += 1;
                if *count >= limit {
                    return;
                }
            }
        }
    }
    let mut count = 0usize;
    walk(root, limit, &mut count, 0);
    count
}

fn load_children(entry: &mut Entry, cfg: &AppConfig) {
    entry.children.clear();
    entry.error = None;
    let rd = match std::fs::read_dir(&entry.path) {
        Ok(rd) => rd,
        Err(e) => {
            entry.error = Some(format!("无法读取目录: {e}"));
            entry.loaded = true;
            return;
        }
    };
    for item in rd.flatten() {
        let path = item.path();
        let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let mut e = Entry::new(path, is_dir);
        if let Ok(meta) = item.metadata() {
            e.modified = meta.modified().ok();
            e.size = if is_dir { 0 } else { meta.len() };
        }
        entry.children.push(e);
    }
    sort_children(&mut entry.children, cfg);
    entry.loaded = true;
}

fn sort_children(children: &mut [Entry], cfg: &AppConfig) {
    let dirs_first = cfg.dirs_first;
    let desc = cfg.sort_desc;
    let by_modified = cfg.sort_by == SortBy::Modified;
    children.sort_by(|a, b| {
        if dirs_first && a.is_dir != b.is_dir {
            return b.is_dir.cmp(&a.is_dir);
        }
        let ord = if by_modified {
            // 最近修改在前
            b.modified
                .cmp(&a.modified)
                .then_with(|| natural_cmp(&a.name, &b.name))
        } else {
            natural_cmp(&a.name, &b.name)
        };
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
}

/// 自然排序：数字按数值比较（`第2章` 排在 `第10章` 前）。
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let al: Vec<char> = a.to_lowercase().chars().collect();
    let bl: Vec<char> = b.to_lowercase().chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    while i < al.len() && j < bl.len() {
        let ca = al[i];
        let cb = bl[j];
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let mut na = String::new();
            while i < al.len() && al[i].is_ascii_digit() {
                na.push(al[i]);
                i += 1;
            }
            let mut nb = String::new();
            while j < bl.len() && bl[j].is_ascii_digit() {
                nb.push(bl[j]);
                j += 1;
            }
            let va: u128 = na.parse().unwrap_or(0);
            let vb: u128 = nb.parse().unwrap_or(0);
            match va.cmp(&vb) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }
        match ca.cmp(&cb) {
            Ordering::Equal => {
                i += 1;
                j += 1;
            }
            ord => return ord,
        }
    }
    al.len().cmp(&bl.len())
}

fn visible(entry: &Entry, cfg: &AppConfig) -> bool {
    if entry.is_dir {
        cfg.show_hidden || !util::is_hidden(&entry.path)
    } else {
        let hidden_ok = cfg.show_hidden || !util::is_hidden(&entry.path);
        let kind_ok = cfg.show_non_md || util::is_markdown(&entry.path);
        hidden_ok && kind_ok
    }
}

fn flatten(
    entry: &Entry,
    depth: usize,
    cfg: &AppConfig,
    out: &mut Vec<FlatRow>,
    _hint: &str,
    force_expand: bool,
) {
    if !visible(entry, cfg) {
        return;
    }
    let has_children = entry.is_dir && !entry.children.is_empty();
    out.push(FlatRow {
        path: entry.path.clone(),
        name: entry.name.clone(),
        depth,
        is_dir: entry.is_dir,
        expanded: entry.expanded || force_expand,
        has_children,
        hint: None,
        matched: false,
        error: entry.error.clone(),
        modified: entry.modified,
    });
    if entry.is_dir && (entry.expanded || force_expand) {
        for c in &entry.children {
            flatten(c, depth + 1, cfg, out, _hint, force_expand);
        }
    }
}

fn collect_search(
    entry: &Entry,
    root: &Path,
    query: &str,
    cfg: &AppConfig,
    out: &mut Vec<FlatRow>,
    count: &mut usize,
    limit: usize,
) {
    for c in &entry.children {
        if *count >= limit {
            return;
        }
        if !visible(c, cfg) {
            continue;
        }
        if c.is_dir {
            collect_search(c, root, query, cfg, out, count, limit);
        } else if c.name.to_lowercase().contains(query) {
            *count += 1;
            let rel = c
                .path
                .parent()
                .map(|p| {
                    p.strip_prefix(root)
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            out.push(FlatRow {
                path: c.path.clone(),
                name: c.name.clone(),
                depth: 0,
                is_dir: false,
                expanded: false,
                has_children: false,
                hint: if rel.is_empty() { None } else { Some(rel) },
                matched: true,
                error: None,
                modified: c.modified,
            });
        }
    }
}

fn find_entry<'a>(entry: &'a Entry, path: &Path) -> Option<&'a Entry> {
    if entry.path == path {
        return Some(entry);
    }
    if !path.starts_with(&entry.path) {
        return None;
    }
    for c in &entry.children {
        if let Some(f) = find_entry(c, path) {
            return Some(f);
        }
    }
    None
}

fn find_entry_mut<'a>(entry: &'a mut Entry, path: &Path) -> Option<&'a mut Entry> {
    if entry.path == path {
        return Some(entry);
    }
    if !path.starts_with(&entry.path) {
        return None;
    }
    for c in &mut entry.children {
        if let Some(f) = find_entry_mut(c, path) {
            return Some(f);
        }
    }
    None
}

fn apply_expanded(entry: &mut Entry, set: &HashSet<PathBuf>, cfg: &AppConfig) {
    if entry.is_dir {
        if set.contains(&entry.path) {
            entry.expanded = true;
        }
        if entry.expanded && !entry.loaded {
            load_children(entry, cfg);
        }
        if entry.expanded {
            for c in &mut entry.children {
                apply_expanded(c, set, cfg);
            }
        }
    }
}

fn collect_expanded(entry: &Entry, out: &mut Vec<PathBuf>) {
    if entry.is_dir && entry.expanded {
        if entry.path != entry.path.parent().unwrap_or(&entry.path) {
            out.push(entry.path.clone());
        }
        for c in &entry.children {
            collect_expanded(c, out);
        }
    }
}

fn collapse_recursive(entry: &mut Entry) {
    entry.expanded = false;
    for c in &mut entry.children {
        collapse_recursive(c);
    }
}

fn collect_loaded_dirs(entry: &Entry, out: &mut Vec<PathBuf>) {
    if entry.is_dir && entry.loaded {
        out.push(entry.path.clone());
        for c in &entry.children {
            collect_loaded_dirs(c, out);
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_puts_2_before_10() {
        let mut names = vec!["第10章.md", "第2章.md", "第1章.md"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["第1章.md", "第2章.md", "第10章.md"]);
        assert_eq!(natural_cmp("a.md", "b.md"), std::cmp::Ordering::Less);
        assert_eq!(natural_cmp("A.md", "a.md"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn counts_markdown_files_recursively() {
        let dir = std::env::temp_dir().join(format!("easymd-count-{}", std::process::id()));
        let sub = dir.join("deep").join("deeper");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        std::fs::write(sub.join("c.markdown"), "x").unwrap();
        assert_eq!(count_markdown_files(&dir, 1000), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tree_respects_filters_and_expansion() {
        let dir = std::env::temp_dir().join(format!("easymd-tree-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        std::fs::write(dir.join(".hidden.md"), "x").unwrap();
        std::fs::write(sub.join("b.md"), "x").unwrap();

        let mut cfg = AppConfig::default();
        let mut tree = Tree::new(dir.clone(), &cfg, &[]);
        let rows = tree.take_rows(&cfg);
        assert_eq!(rows.len(), 2, "默认不显示隐藏文件：sub 与 a.md");
        tree.put_rows(rows, &cfg);

        cfg.show_hidden = true;
        tree.invalidate();
        let rows = tree.take_rows(&cfg);
        assert_eq!(rows.len(), 3, "开启后多出 .hidden.md");
        tree.put_rows(rows, &cfg);

        tree.set_expanded(&sub, true, &cfg);
        let rows = tree.take_rows(&cfg);
        assert!(
            rows.iter().any(|r| r.path.ends_with("b.md")),
            "展开后应看到子目录内文件"
        );
        assert!(rows.iter().any(|r| r.depth == 1), "子目录内的层级为 1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn root_can_be_collapsed_and_restored() {
        let dir = std::env::temp_dir().join(format!("easymd-root-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        let cfg = AppConfig::default();
        let mut tree = Tree::new(dir.clone(), &cfg, &[]);
        assert_eq!(tree.take_rows(&cfg).len(), 2);
        // 折叠根目录：列表清空（此时界面上的空白区域即代表根目录）
        tree.set_expanded(&dir, false, &cfg);
        assert_eq!(tree.take_rows(&cfg).len(), 0);
        // 展开回来
        tree.set_expanded(&dir, true, &cfg);
        assert_eq!(tree.take_rows(&cfg).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_filters_by_name() {
        let dir = std::env::temp_dir().join(format!("easymd-filter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Rust 笔记.md"), "x").unwrap();
        std::fs::write(dir.join("Python 笔记.md"), "x").unwrap();
        let cfg = AppConfig::default();
        let mut tree = Tree::new(dir.clone(), &cfg, &[]);
        tree.set_query("rust".into());
        let rows = tree.take_rows(&cfg);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].name.contains("Rust"));
        assert!(rows[0].matched);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
