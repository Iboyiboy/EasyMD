// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 当前打开的文档：内容、编码、脏标记、大纲、统计信息。

use std::path::{Path, PathBuf};

use crate::fs_ops::{self, Encoding, LoadedText};
use crate::util::{self, TextStats};

/// 大纲中的一条标题。
#[derive(Debug, Clone)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    /// 锚点 id：优先使用文档里显式写的 `{#id}`，否则由标题文本生成（GitHub 风格）
    pub slug: String,
    pub line: usize,
    /// 需要在渲染文本中插入 `{#slug}` 的字节位置（None 表示文档里已显式指定）
    pub inject_at: Option<usize>,
}

pub struct Document {
    pub path: PathBuf,
    pub text: String,
    /// 供渲染使用的内容：与 `text` 相同，但为没有锚点的标题补上了 `{#slug}`
    /// （Markdown 渲染器只会为显式写了 `{#id}` 的标题生成锚点，不补就无法做大纲跳转）
    pub render_text: String,
    /// 渲染文本里插入的锚点属性：(渲染文本中的字节偏移, 插入的字节数)。
    /// 因为插入会让后面的偏移整体后移，所以在两套坐标之间换算时必须用它。
    pub injections: Vec<(usize, usize)>,
    /// 上次保存/载入时的内容，用于判断是否有未保存修改
    pub saved_text: String,
    pub encoding: Encoding,
    pub crlf: bool,
    pub bytes_on_disk: u64,
    pub binary: bool,
    pub load_error: Option<String>,
    pub headings: Vec<Heading>,
    /// 文档内的链接地址（用于注册点击钩子）
    pub link_targets: Vec<String>,
    pub stats: TextStats,
    /// 需要滚动到的大纲锚点（在阅读面板中消费）
    pub pending_anchor: Option<String>,
    /// 需要定位到的行号（0 基，来自全文搜索结果）
    pub pending_line: Option<usize>,
    /// 编辑器待设置的光标位置（字符索引）
    pub pending_cursor: Option<usize>,
    /// 每帧请求重新解析大纲等
    dirty_outline: bool,
    last_mtime: Option<std::time::SystemTime>,
}

impl Document {
    pub fn load(path: &Path) -> Result<Self, String> {
        let loaded = fs_ops::read_text(path)?;
        Ok(Self::from_loaded(path, loaded))
    }

    pub fn from_loaded(path: &Path, loaded: LoadedText) -> Self {
        let headings = parse_headings(&loaded.text);
        let (render_text, injections) = build_render_text(&loaded.text, &headings);
        let link_targets = local_link_targets(&loaded.text);
        let stats = util::text_stats(&loaded.text);
        let last_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        Self {
            path: path.to_path_buf(),
            text: loaded.text.clone(),
            saved_text: loaded.text,
            encoding: loaded.encoding,
            crlf: loaded.crlf,
            bytes_on_disk: loaded.bytes,
            binary: loaded.binary,
            load_error: None,
            headings,
            render_text,
            injections,
            link_targets,
            stats,
            pending_anchor: None,
            pending_line: None,
            pending_cursor: None,
            dirty_outline: false,
            last_mtime,
        }
    }

    pub fn name(&self) -> String {
        util::file_name(&self.path)
    }

    pub fn dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }

    /// 标记内容变更（编辑后调用，避免每次按键都重新解析大纲）。
    pub fn mark_edited(&mut self) {
        self.stats = util::text_stats(&self.text);
        self.dirty_outline = true;
    }

    /// 按需重建大纲（编辑时隔一小段时间才重建，避免每次按键都解析）。
    pub fn refresh_outline_if_needed(&mut self) {
        if self.dirty_outline {
            self.rebuild_derived();
        }
    }

    pub fn force_refresh_outline(&mut self) {
        self.rebuild_derived();
    }

    /// 重新解析大纲、链接与渲染文本。
    fn rebuild_derived(&mut self) {
        self.headings = parse_headings(&self.text);
        let (render_text, injections) = build_render_text(&self.text, &self.headings);
        self.render_text = render_text;
        self.injections = injections;
        self.link_targets = local_link_targets(&self.text);
        self.stats = util::text_stats(&self.text);
        self.dirty_outline = false;
    }

    pub fn save(&mut self) -> Result<(), String> {
        fs_ops::write_text(&self.path, &self.text, self.encoding, self.crlf)?;
        self.saved_text = self.text.clone();
        self.bytes_on_disk = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        self.last_mtime = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        Ok(())
    }

    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        let loaded = fs_ops::read_text(&self.path)?;
        let d = Self::from_loaded(&self.path, loaded);
        self.text = d.text;
        self.saved_text = d.saved_text;
        self.encoding = d.encoding;
        self.crlf = d.crlf;
        self.bytes_on_disk = d.bytes_on_disk;
        self.binary = d.binary;
        self.headings = d.headings;
        self.render_text = d.render_text;
        self.injections = d.injections;
        self.link_targets = d.link_targets;
        self.stats = d.stats;
        self.last_mtime = d.last_mtime;
        self.dirty_outline = false;
        Ok(())
    }

    /// 渲染文本坐标 → 原文坐标。
    pub fn render_to_text_offset(&self, render_offset: usize) -> usize {
        let mut off = render_offset;
        for (at, len) in &self.injections {
            if render_offset >= at + len {
                off -= len;
            }
        }
        clamp_to_char_boundary(&self.text, off)
    }

    /// 原文坐标 → 渲染文本坐标。
    pub fn text_to_render_offset(&self, text_offset: usize) -> usize {
        let mut off = text_offset;
        for (at, len) in &self.injections {
            // at 是渲染文本坐标，换算成对应的原文坐标来比较
            let text_at = at.saturating_sub(
                self.injections
                    .iter()
                    .filter(|(a, l)| a + l <= *at)
                    .map(|(_, l)| *l)
                    .sum::<usize>(),
            );
            if text_offset >= text_at {
                off += len;
            }
        }
        clamp_to_char_boundary(&self.render_text, off)
    }

    /// 相对仓库根目录的显示路径。
    pub fn rel_display(&self, root: Option<&Path>) -> String {
        if let Some(root) = root {
            if let Ok(rel) = self.path.strip_prefix(root) {
                return rel.to_string_lossy().to_string();
            }
        }
        self.path.to_string_lossy().to_string()
    }

    pub fn goto_line(&mut self, line0: usize) {
        let idx = char_index_of_line(&self.text, line0);
        self.pending_cursor = Some(idx);
        self.pending_line = Some(line0);
    }

    /// 光标所处的行列（供状态栏显示）。
    pub fn line_col_of_char_index(&self, idx: usize) -> (usize, usize) {
        let mut line = 1usize;
        let mut col = 1usize;
        for (i, c) in self.text.chars().enumerate() {
            if i >= idx {
                break;
            }
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}

/// 把行号转换成字符索引。
pub fn char_index_of_line(text: &str, line0: usize) -> usize {
    let mut line = 0usize;
    for (i, c) in text.chars().enumerate() {
        if line >= line0 {
            return i;
        }
        if c == '\n' {
            line += 1;
        }
    }
    text.chars().count()
}

/// 用与渲染相同的解析选项提取标题，并为没有显式 `{#id}` 的标题生成 GitHub 风格锚点。
pub fn parse_headings(text: &str) -> Vec<Heading> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let opts = parser_options();

    let mut out: Vec<Heading> = Vec::new();
    let mut current: Option<(u8, String, Option<String>, usize)> = None;
    let mut line = 0usize;
    let mut last_pos = 0usize;

    for (event, range) in Parser::new_ext(text, opts).into_offset_iter() {
        // 依据字节偏移估算行号。注意 pulldown-cmark 给出的区间并非总是单调递增，
        // 因此只在前向推进时累计换行，避免切片越界。
        let start = range.start.min(text.len());
        let _end = range.end.min(text.len()).max(start);
        if start >= last_pos {
            line += text[last_pos..start].matches('\n').count();
            last_pos = start;
        }

        match event {
            Event::Start(Tag::Heading { level, id, .. }) => {
                let lvl = match level {
                    pulldown_cmark::HeadingLevel::H1 => 1,
                    pulldown_cmark::HeadingLevel::H2 => 2,
                    pulldown_cmark::HeadingLevel::H3 => 3,
                    pulldown_cmark::HeadingLevel::H4 => 4,
                    pulldown_cmark::HeadingLevel::H5 => 5,
                    pulldown_cmark::HeadingLevel::H6 => 6,
                };
                // 锚点属性必须写在标题块的「首行行尾」：
                //   ATX   ## 标题        -> ## 标题 {#slug}
                //   ATX   ## 标题 ##     -> ## 标题 ## {#slug}
                // 结尾式 标题\n=====      -> 标题 {#slug}\n=====
                let mut at = start;
                let bytes = text.as_bytes();
                while at < text.len() && bytes[at] != b'\n' && bytes[at] != b'\r' {
                    at += 1;
                }
                while at > start && bytes[at - 1].is_ascii_whitespace() {
                    at -= 1;
                }
                current = Some((lvl, String::new(), id.map(|i| i.into_string()), at));
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, s, _, _)) = current.as_mut() {
                    s.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, s, _, _)) = current.as_mut() {
                    s.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((lvl, raw_text, explicit_id, inject_at)) = current.take() {
                    let body = raw_text.trim().to_string();
                    match explicit_id {
                        Some(id) => out.push(Heading {
                            level: lvl,
                            text: body,
                            slug: id,
                            line,
                            inject_at: None,
                        }),
                        None => {
                            let mut slug = slugify(&body);
                            if slug.is_empty() {
                                slug = format!("section-{}", line + 1);
                            }
                            // 同一文档内保证唯一
                            let mut n = 1;
                            let base = slug.clone();
                            while out.iter().any(|h| h.slug == slug) {
                                slug = format!("{base}-{n}");
                                n += 1;
                            }
                            out.push(Heading {
                                level: lvl,
                                text: body,
                                slug,
                                line,
                                inject_at: Some(inject_at),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

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

/// GitHub 风格的标题锚点：小写化、空格转连字符、去掉标点，保留中英文与数字。
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in text.trim().chars() {
        if c.is_whitespace() {
            if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        } else if c.is_alphanumeric() || c == '-' || c == '_' {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_dash = c == '-';
        }
    }
    let trimmed = out.trim_matches('-');
    trimmed.to_string()
}

/// 把字节偏移收拢到最近的字符边界（防御非法偏移导致切片 panic）。
pub fn clamp_to_char_boundary(text: &str, mut offset: usize) -> usize {
    if offset > text.len() {
        offset = text.len();
    }
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// 生成渲染用文本：为没有锚点的标题补 `{#slug}`（渲染时不可见，只影响锚点）。
///
/// 同时返回插入表 `(渲染文本中的偏移, 插入字节数)`：渲染文本与原文的字节偏移
/// 从插入点之后就不再相同，跨坐标系换算必须用它（否则中文会切在多字节字符中间）。
pub fn build_render_text(text: &str, headings: &[Heading]) -> (String, Vec<(usize, usize)>) {
    let mut inserts: Vec<(usize, String)> = headings
        .iter()
        .filter_map(|h| h.inject_at.map(|at| (at, format!(" {{#{}}}", h.slug))))
        .collect();
    if inserts.is_empty() {
        return (text.to_string(), Vec::new());
    }
    inserts.sort_by_key(|(at, _)| *at);
    let mut out = String::with_capacity(text.len() + inserts.len() * 16);
    let mut injections = Vec::with_capacity(inserts.len());
    let mut last = 0usize;
    for (at, ins) in inserts {
        let at = at.min(text.len());
        if at < last || !text.is_char_boundary(at) {
            continue;
        }
        out.push_str(&text[last..at]);
        injections.push((out.len(), ins.len()));
        out.push_str(&ins);
        last = at;
    }
    out.push_str(&text[last..]);
    (out, injections)
}

/// 找出文档中的链接与图片地址（用于注册点击钩子）。
pub fn local_link_targets(text: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag};
    let opts = parser_options();
    let mut out: Vec<String> = Vec::new();
    for event in Parser::new_ext(text, opts) {
        if let Event::Start(Tag::Link { dest_url, .. }) | Event::Start(Tag::Image { dest_url, .. }) =
            event
        {
            out.push(dest_url.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 文档内锚点（`#标题`）。
pub fn is_anchor(dest: &str) -> bool {
    dest.trim_start().starts_with('#')
}

/// 外部链接（http / https / mailto 等）。
pub fn is_external(dest: &str) -> bool {
    let lower = dest.trim().to_lowercase();
    const SCHEMES: &[&str] = &[
        "http://",
        "https://",
        "mailto:",
        "ftp://",
        "ftps://",
        "data:",
        "tel:",
        "sms:",
        "file://",
        "ssh://",
        "git://",
        "ws://",
        "wss://",
    ];
    SCHEMES.iter().any(|s| lower.starts_with(s)) || lower.contains("://")
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_headings_with_anchor_ids() {
        let text = "# 一级标题\n\n正文\n\n## 二级标题\n\n### 带 空格 的标题\n";
        let hs = parse_headings(text);
        assert_eq!(hs.len(), 3);
        assert_eq!(hs[0].level, 1);
        assert_eq!(hs[0].text, "一级标题");
        assert_eq!(hs[1].level, 2);
        assert_eq!(hs[2].text, "带 空格 的标题");
        assert_eq!(hs[0].slug, "一级标题", "无显式锚点时应按标题文本生成");
        assert_eq!(hs[2].slug, "带-空格-的标题", "空格应转成连字符");
        assert_eq!(hs[0].line, 0);
        assert_eq!(hs[1].line, 4, "二级标题在第 5 行（0 基 4）");
    }

    #[test]
    fn injects_anchor_attributes_for_render() {
        let text = "# 标题一\n\n正文\n\n## 已有锚点 {#custom}\n";
        let hs = parse_headings(text);
        assert_eq!(hs[0].slug, "标题一");
        assert!(hs[0].inject_at.is_some(), "应补锚点");
        assert_eq!(hs[1].slug, "custom", "显式锚点应被沿用");
        assert!(hs[1].inject_at.is_none(), "已有锚点不必再补");

        let (rendered, injections) = build_render_text(text, &hs);
        assert_eq!(injections.len(), 1);
        assert!(rendered.contains("# 标题一 {#标题一}"), "渲染文本应带上锚点：{rendered}");
        assert!(rendered.contains("## 已有锚点 {#custom}"));

        // 补过锚点的文本仍然没有可见的属性文字，且大纲解析结果一致
        let again = parse_headings(&rendered);
        assert_eq!(again.len(), 2);
        assert_eq!(again[0].text, "标题一");
        assert_eq!(again[0].slug, "标题一");
    }

    #[test]
    fn injects_anchor_for_setext_and_closed_atx() {
        // 结尾式标题：锚点必须写在首行行尾，写在 ==== 之后会让标题失效
        let setext = "标题一\n=====\n\n正文\n";
        let hs = parse_headings(setext);
        assert_eq!(hs.len(), 1, "结尾式标题应被识别");
        let (rendered, _) = build_render_text(setext, &hs);
        assert!(rendered.starts_with("标题一 {#标题一}\n====="), "渲染文本：{rendered}");
        let again = parse_headings(&rendered);
        assert_eq!(again.len(), 1, "补锚点后仍应是标题：{rendered:?}");
        assert_eq!(again[0].slug, "标题一");
        assert_eq!(again[0].text, "标题一");

        // 带收尾井号的 ATX 标题：锚点放在最后
        let atx = "## 标题二 ##\n";
        let hs2 = parse_headings(atx);
        let (r2, _) = build_render_text(atx, &hs2);
        assert_eq!(r2, "## 标题二 ## {#标题二}\n");
        let again2 = parse_headings(&r2);
        assert_eq!(again2[0].text, "标题二");
        assert_eq!(again2[0].slug, "标题二");
    }

    #[test]
    fn offset_translation_between_text_and_render() {
        // 中文标题 + 正文，注入锚点后偏移会整体后移
        let text = "# 中文标题

第一段正文内容。

第二段正文内容。
";
        let hs = parse_headings(text);
        let (render, injections) = build_render_text(text, &hs);
        assert_eq!(injections.len(), 1, "只有一个需要补锚点的标题");
        assert!(render.starts_with("# 中文标题 {#中文标题}"), "{render:?}");

        let d = Document::from_loaded(
            std::path::Path::new("test.md"),
            crate::fs_ops::LoadedText {
                text: text.to_string(),
                encoding: crate::fs_ops::Encoding::Utf8,
                crlf: false,
                bytes: text.len() as u64,
                binary: false,
            },
        );
        // 渲染文本里「第二段」的偏移，换算回原文后必须指向同一处文字
        let r_off = render.find("第二段").unwrap();
        let t_off = d.render_to_text_offset(r_off);
        assert_eq!(&text[t_off..t_off + "第二段".len()], "第二段");
        // 反向换算回去
        assert_eq!(d.text_to_render_offset(t_off), r_off);
        // 偏到多字节字符中间也不会 panic，且仍是合法边界
        let weird = d.render_to_text_offset(r_off + 1);
        assert!(text.is_char_boundary(weird));
        // 无需注入的文档：两套坐标完全一致
        let plain = "普通段落，没有标题。
";
        let hs2 = parse_headings(plain);
        let (r2, inj2) = build_render_text(plain, &hs2);
        assert!(inj2.is_empty());
        assert_eq!(r2, plain);
    }

    #[test]
    fn clamp_handles_mid_char_offsets() {
        let s = "中文abc";
        assert_eq!(clamp_to_char_boundary(s, 1), 0, "切在汉字中间应回退");
        assert_eq!(clamp_to_char_boundary(s, 3), 3);
        assert_eq!(clamp_to_char_boundary(s, 999), s.len());
    }

    #[test]
    fn slugify_is_stable_and_unique() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("中文 标题（测试）"), "中文-标题测试");
        assert_eq!(slugify("  --a b--  "), "a-b");
        let hs = parse_headings("# 重复\n\n# 重复\n");
        assert_eq!(hs[0].slug, "重复");
        assert_eq!(hs[1].slug, "重复-1", "重复标题应自动去重");
    }

    #[test]
    fn collects_local_and_external_links() {
        let text = "见 [本地](sub/a.md) 与 [外链](https://example.com) 以及 [锚点](#标题)";
        let targets = local_link_targets(text);
        assert!(targets.contains(&"sub/a.md".to_string()));
        assert!(targets.contains(&"https://example.com".to_string()));
        assert!(targets.contains(&"#标题".to_string()));
    }

    #[test]
    fn classifies_link_kinds() {
        assert!(is_anchor("#section"));
        assert!(is_external("https://a.com/x.md"));
        assert!(is_external("mailto:a@b.com"));
        assert!(!is_external("notes/a.md"));
        assert!(!is_external("sub/dir/b.md"));
    }

    #[test]
    fn maps_line_to_char_index() {
        let text = "第一行\n第二行\n第三行";
        assert_eq!(char_index_of_line(text, 0), 0);
        assert_eq!(char_index_of_line(text, 1), 4);
        assert_eq!(char_index_of_line(text, 2), 8);
        assert_eq!(char_index_of_line(text, 99), text.chars().count());
    }
}

/// 返回包含 `byte_offset` 的「块」的字节范围（用于分栏联动时高亮当前块）。
///
/// 覆盖常见的 Markdown 块：围栏代码块、标题行、以及被空行分隔的普通段落 / 列表组。
pub fn block_range_at(text: &str, byte_offset: usize) -> (usize, usize) {
    // 偏移可能落在多字节字符中间，先归到最近的字符边界
    let offset = clamp_to_char_boundary(text, byte_offset);
    let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);

    let lines: Vec<(usize, usize)> = {
        let mut v = Vec::new();
        let mut start = 0usize;
        for (i, _) in text.match_indices('\n') {
            v.push((start, i));
            start = i + 1;
        }
        v.push((start, text.len()));
        v
    };
    // 找到当前行在 lines 里的下标
    let idx = lines
        .iter()
        .position(|(s, e)| *s <= line_start && line_start <= *e)
        .unwrap_or_else(|| lines.len().saturating_sub(1));

    let is_fence = |l: &str| {
        let t = l.trim_start();
        t.starts_with("```") || t.starts_with("~~~")
    };
    let line_str = |i: usize| -> &str {
        lines
            .get(i)
            .map(|(s, e)| text[*s..*e].trim_end_matches('\r'))
            .unwrap_or("")
    };

    // 行首标记（`> `、`- `、`# `、`1. ` 等）不属于行内容：渲染锚点记的是内容起点，
    // 查表时也要用内容起点，否则会落到上一行去。
    fn content_start_of(line: &str) -> usize {
        let mut idx = 0;
        for (i, c) in line.char_indices() {
            let is_markup = c.is_whitespace()
                || matches!(c, '>' | '#' | '-' | '*' | '+' | '.' | '[' | ']' | 'x' | 'X')
                || c.is_ascii_digit();
            if !is_markup {
                idx = i;
                break;
            }
            idx = i + c.len_utf8();
        }
        idx
    }

    // 1) 围栏代码块：向前找未闭合的围栏
    let mut fence_open: Option<usize> = None;
    let mut inside_fence = false;
    for i in 0..=idx {
        let l = line_str(i);
        if is_fence(l) {
            if inside_fence {
                if i >= idx {
                    // 当前行就是闭合围栏
                    break;
                }
                inside_fence = false;
                fence_open = None;
            } else {
                inside_fence = true;
                fence_open = Some(i);
            }
        }
    }
    if inside_fence {
        if let Some(open) = fence_open {
            let mut end = lines.last().map(|(_, e)| *e).unwrap_or(text.len());
            for j in (open + 1)..lines.len() {
                if is_fence(line_str(j)) {
                    end = lines[j].1;
                    break;
                }
            }
            return (lines[open].0, end);
        }
    }

    // 2) 标题行：只高亮自己
    let cur = line_str(idx);
    if cur.trim_start().starts_with('#') {
        let start = lines[idx].0 + content_start_of(cur);
        return (start.min(lines[idx].1), lines[idx].1);
    }

    // 3) 普通段落 / 列表组：向上下扩展到相邻非空行
    let mut start = idx;
    while start > 0 {
        let prev = line_str(start - 1);
        if prev.trim().is_empty() || prev.trim_start().starts_with('#') || is_fence(prev) {
            break;
        }
        start -= 1;
    }
    let mut end = idx;
    while end + 1 < lines.len() {
        let next = line_str(end + 1);
        if next.trim().is_empty() || next.trim_start().starts_with('#') || is_fence(next) {
            break;
        }
        end += 1;
    }
    let block_start = lines[start].0 + content_start_of(line_str(start));
    (block_start.min(lines[start].1), lines[end].1)
}

#[cfg(test)]
mod block_tests {
    use super::*;

    #[test]
    fn block_range_handles_paragraph_heading_and_code() {
        let text = "# 标题\n\n第一段\n第二行\n\n- 列表一\n- 列表二\n\n\u{60}\u{60}\u{60}rust\nfn a() {}\n\u{60}\u{60}\u{60}\n";
        // 标题行：块起点跳过 `# ` 标记（渲染锚点记的是内容起点）
        let r = block_range_at(text, 2);
        assert_eq!(&text[r.0..r.1], "标题");
        // 段落应包含两行
        let r2 = block_range_at(text, text.find("第二行").unwrap() + 1);
        assert_eq!(&text[r2.0..r2.1], "第一段\n第二行");
        // 列表组：连续两行一起，起点跳过 `- ` 标记
        let r3 = block_range_at(text, text.find("列表一").unwrap() + 1);
        assert_eq!(&text[r3.0..r3.1], "列表一\n- 列表二");
        // 代码块：包含围栏
        let r4 = block_range_at(text, text.find("fn a").unwrap() + 1);
        assert!(text[r4.0..r4.1].starts_with("```rust"), "got {:?}", &text[r4.0..r4.1]);
        assert!(text[r4.0..r4.1].ends_with("```"));
    }
}

/// 文件被写入后的处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChange {
    /// 磁盘内容与内存一致（通常是本程序自己保存写下的），什么都不做
    Ignore,
    /// 外部修改且本地没有未保存内容：直接重新载入
    Reload,
    /// 外部修改但本地有未保存内容：保留本地编辑，只提示
    KeepLocal,
}

/// 判断一次文件写入事件该怎么处理。
///
/// 关键点：本程序自动保存/手动保存同样会触发文件监听事件，
/// 如果只按「有没有未保存修改」判断，就会把自己的保存误报成「文件被外部修改」。
/// 这里直接比对磁盘内容与内存内容——相同就说明是自己写的（或本来一致），忽略即可。
pub fn classify_external_change(disk: &str, memory: &str, dirty: bool) -> ExternalChange {
    if disk == memory {
        ExternalChange::Ignore
    } else if dirty {
        ExternalChange::KeepLocal
    } else {
        ExternalChange::Reload
    }
}

#[cfg(test)]
mod external_change_tests {
    use super::*;

    #[test]
    fn own_save_is_not_an_external_change() {
        // 自动保存后磁盘内容 == 内存内容，即便此时“有未保存修改”的判定为真，也不该弹窗
        assert_eq!(
            classify_external_change("同一份内容", "同一份内容", true),
            ExternalChange::Ignore
        );
        assert_eq!(
            classify_external_change("同一份内容", "同一份内容", false),
            ExternalChange::Ignore
        );
    }

    #[test]
    fn external_edit_without_local_changes_reloads() {
        assert_eq!(
            classify_external_change("磁盘上的新内容", "内存里的旧内容", false),
            ExternalChange::Reload
        );
    }

    #[test]
    fn external_edit_keeps_local_edits() {
        assert_eq!(
            classify_external_change("磁盘上的新内容", "我正在写的内容", true),
            ExternalChange::KeepLocal
        );
    }
}
