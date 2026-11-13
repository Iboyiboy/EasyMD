// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 通用工具函数：路径、时间、文本统计、颜色。

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 视为 Markdown 的扩展名。
pub const MD_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn", "mdx"];

/// 用户配置目录（不依赖 dirs crate，保持依赖精简）：
/// - Windows: %APPDATA%
/// - Linux/macOS: $XDG_CONFIG_HOME 或 ~/.config
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            if !appdata.is_empty() {
                return PathBuf::from(appdata);
            }
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let mut p = PathBuf::from(home);
            p.push("AppData");
            p.push("Roaming");
            return p;
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg);
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            let mut p = PathBuf::from(home);
            p.push(".config");
            return p;
        }
    }
    PathBuf::from(".")
}

pub fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_markdown(path: &Path) -> bool {
    MD_EXTS.contains(&ext_lower(path).as_str())
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// 隐藏文件：以 `.` 开头，或（Windows）带隐藏属性。
pub fn is_hidden(path: &Path) -> bool {
    let name = file_name(path);
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        if let Ok(meta) = std::fs::metadata(path) {
            let attr = meta.file_attributes();
            return attr & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0;
        }
    }
    false
}

/// 文件名合法性校验。返回 `Err` 时包含中文原因说明。
pub fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("名称不能为空".into());
    }
    if name == "." || name == ".." {
        return Err("名称不能是 . 或 ..".into());
    }
    if name.len() > 200 {
        return Err("名称过长（超过 200 字符）".into());
    }
    const BAD: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if let Some(c) = name.chars().find(|c| BAD.contains(c)) {
        return Err(format!("名称不能包含字符 {c}"));
    }
    if name.chars().any(|c| (c as u32) < 0x20) {
        return Err("名称不能包含控制字符".into());
    }
    // Windows 不允许结尾是空格或点
    if name.ends_with(' ') || name.ends_with('.') {
        return Err("名称不能以空格或点结尾".into());
    }
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem_upper = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if RESERVED.contains(&stem_upper.as_str()) {
        return Err(format!("{name} 是系统保留名称"));
    }
    Ok(())
}

/// 在目录下生成一个不冲突的名字，如 `未命名.md`、`未命名 2.md`。
pub fn unique_child_name(dir: &Path, base: &str, ext: Option<&str>) -> String {
    let make = |i: usize| -> String {
        let stem = if i == 0 {
            base.to_string()
        } else {
            format!("{base} {i}")
        };
        match ext {
            Some(e) => format!("{stem}.{e}"),
            None => stem,
        }
    };
    for i in 0..10_000 {
        let candidate = make(i);
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    make(now_secs() as usize)
}

/// 把 Markdown 链接里的相对地址解析为真实路径（支持 `%20`、`<...>`、`#锚点`）。
pub fn resolve_relative(base_dir: &Path, link: &str) -> PathBuf {
    let mut s = link.trim().to_string();

    // 去掉锚点与查询
    if let Some(pos) = s.find('#') {
        s.truncate(pos);
    }
    if let Some(pos) = s.find('?') {
        s.truncate(pos);
    }
    // `file://` 前缀
    if let Some(rest) = s.strip_prefix("file://") {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        #[cfg(windows)]
        {
            s = rest.replace('/', "\\");
        }
        #[cfg(not(windows))]
        {
            s = format!("/{rest}");
        }
    }
    let s = percent_decode(&s);
    let p = Path::new(&s);
    if p.is_absolute() {
        return normalize_path(p);
    }
    normalize_path(&base_dir.join(p))
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 纯词法路径归一化（处理 `.` 与 `..`，不访问文件系统）。
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// 判断 `child` 是否在 `ancestor` 之内（用于禁止把目录拖进自身）。
pub fn is_within(child: &Path, ancestor: &Path) -> bool {
    let c = normalize_path(child);
    let a = normalize_path(ancestor);
    c.starts_with(&a)
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    normalize_path(a) == normalize_path(b)
}

/// 供 Markdown 渲染使用的文档目录 URI 前缀（保证相对图片路径可解析，且不同文档缓存互不干扰）。
pub fn doc_uri_prefix(doc_dir: &Path) -> String {
    let mut s = doc_dir.to_string_lossy().replace('\\', "/");
    while s.starts_with('/') {
        s.remove(0);
    }
    while s.ends_with('/') {
        s.pop();
    }
    format!("easymd://{s}/")
}

// ---------------------------------------------------------------- 时间

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn secs_of(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// 天数转公历日期（Howard Hinnant 算法）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn split_ymdhms(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

pub fn format_datetime(t: SystemTime) -> String {
    let (y, mo, d, h, mi, _) = split_ymdhms(secs_of(t));
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

pub fn format_date(t: SystemTime) -> String {
    let (y, mo, d, _, _, _) = split_ymdhms(secs_of(t));
    format!("{y:04}-{mo:02}-{d:02}")
}

pub fn format_relative(t: SystemTime) -> String {
    let now = now_secs() as i64;
    let then = secs_of(t);
    let diff = now - then;
    if diff < 0 {
        return format_datetime(t);
    }
    match diff {
        0..=59 => "刚刚".into(),
        60..=3599 => format!("{} 分钟前", diff / 60),
        3600..=86_399 => format!("{} 小时前", diff / 3600),
        86_400..=172_799 => "昨天".into(),
        172_800..=2_591_999 => format!("{} 天前", diff / 86_400),
        _ => format_date(t),
    }
}

// ---------------------------------------------------------------- 文本统计

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF        // 日文假名
        | 0x3400..=0x4DBF      // 扩展 A
        | 0x4E00..=0x9FFF      // 基本汉字
        | 0xF900..=0xFAFF      // 兼容汉字
        | 0x20000..=0x2FA1F    // 扩展 B~F
        | 0xAC00..=0xD7AF      // 韩文
    )
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TextStats {
    pub chars: usize,
    pub words: usize,
    pub lines: usize,
    pub cjk: usize,
}

pub fn text_stats(text: &str) -> TextStats {
    let mut st = TextStats {
        chars: text.chars().count(),
        lines: if text.is_empty() {
            1
        } else {
            text.lines().count().max(1)
        },
        ..Default::default()
    };
    let mut in_latin_word = false;
    for c in text.chars() {
        if is_cjk(c) {
            st.cjk += 1;
            st.words += 1;
            in_latin_word = false;
        } else if c.is_alphanumeric() || c == '\'' || c == '-' {
            if !in_latin_word {
                st.words += 1;
                in_latin_word = true;
            }
        } else {
            in_latin_word = false;
        }
    }
    st
}

/// 预估阅读时间（分钟）：中文 400 字/分钟，西文 220 词/分钟。
pub fn reading_minutes(stats: &TextStats) -> f32 {
    let latin_words = stats.words.saturating_sub(stats.cjk);
    (stats.cjk as f32 / 400.0) + (latin_words as f32 / 220.0)
}

pub fn reading_time_text(stats: &TextStats) -> String {
    let m = reading_minutes(stats);
    if m < 1.0 {
        "不足 1 分钟".to_string()
    } else if m < 60.0 {
        format!("约 {} 分钟", m.round() as i64)
    } else {
        format!("约 {:.1} 小时", m / 60.0)
    }
}

/// 按字符数截断并加省略号。
pub fn elide(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}

/// 中间省略的路径显示，便于在窄栏里看尾部文件名。
pub fn elide_middle(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars || max_chars < 8 {
        return s.to_string();
    }
    let keep = max_chars - 1; // 省略号占 1 个字符
    let head = keep / 3;
    let tail = keep - head;
    let start: String = s.chars().take(head).collect();
    let end: String = s
        .chars()
        .skip(count - tail)
        .collect::<String>();
    format!("{start}…{end}")
}

pub fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.2} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}

/// 粗略判断二进制内容（含 NUL 字节或大量不可打印字符）。
pub fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    let bad = sample
        .iter()
        .filter(|b| **b < 0x09 || (**b > 0x0D && **b < 0x20))
        .count();
    bad * 100 / sample.len().max(1) > 5
}

// ---------------------------------------------------------------- 颜色

pub fn hex_to_color(s: &str) -> Option<egui::Color32> {
    let s = s.trim().trim_start_matches('#');
    let parse = |r: &str| u8::from_str_radix(r, 16).ok();
    match s.len() {
        3 => {
            let mut it = s.chars();
            let r = it.next()?;
            let g = it.next()?;
            let b = it.next()?;
            let d = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
            Some(egui::Color32::from_rgb(d(r)?, d(g)?, d(b)?))
        }
        6 => Some(egui::Color32::from_rgb(
            parse(&s[0..2])?,
            parse(&s[2..4])?,
            parse(&s[4..6])?,
        )),
        8 => Some(egui::Color32::from_rgba_unmultiplied(
            parse(&s[0..2])?,
            parse(&s[2..4])?,
            parse(&s[4..6])?,
            parse(&s[6..8])?,
        )),
        _ => None,
    }
}

pub fn color_to_hex(c: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

pub fn match_positions_ignore_case(haystack: &str, needle: &str, limit: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let h: Vec<char> = haystack.to_lowercase().chars().collect();
    let n: Vec<char> = needle.to_lowercase().chars().collect();
    if n.len() > h.len() {
        return out;
    }
    let mut i = 0;
    while i + n.len() <= h.len() {
        if h[i..i + n.len()] == n[..] {
            out.push((i, i + n.len()));
            if out.len() >= limit {
                break;
            }
            i += n.len();
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_file_names() {
        assert!(validate_name("笔记.md").is_ok());
        assert!(validate_name("a b.txt").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        assert!(validate_name("a/b.md").is_err());
        assert!(validate_name("a\\b.md").is_err());
        assert!(validate_name("a:b.md").is_err());
        assert!(validate_name("con.md").is_err(), "系统保留名应被拒绝");
        assert!(validate_name("末尾空格 .md").is_ok());
        // 首尾空白会被忽略（而不是报错），因此这里应当是合法的
        assert!(validate_name("结尾空格.md ").is_ok());
        assert_eq!(validate_name("结尾空格.md "), Ok(()));
        assert!(validate_name("结尾点.").is_err());
    }

    #[test]
    fn resolves_relative_links() {
        let base = Path::new("/repo/docs");
        assert_eq!(resolve_relative(base, "a.md"), PathBuf::from("/repo/docs/a.md"));
        assert_eq!(resolve_relative(base, "../x.md"), PathBuf::from("/repo/x.md"));
        assert_eq!(
            resolve_relative(base, "sub/b.md"),
            PathBuf::from("/repo/docs/sub/b.md")
        );
        assert_eq!(resolve_relative(base, "a.md#章节"), PathBuf::from("/repo/docs/a.md"));
        assert_eq!(
            resolve_relative(base, "带 空格.md"),
            PathBuf::from("/repo/docs/带 空格.md")
        );
        assert_eq!(
            resolve_relative(base, "%E7%AC%94%E8%AE%B0.md"),
            PathBuf::from("/repo/docs/笔记.md")
        );
    }

    #[test]
    fn builds_doc_uri_prefix() {
        let prefix = doc_uri_prefix(Path::new("F:/notes/子目录"));
        assert_eq!(prefix, "easymd://F:/notes/子目录/");
        assert_eq!(
            doc_uri_prefix(Path::new("/home/u/notes")),
            "easymd://home/u/notes/"
        );
    }

    #[test]
    fn counts_words_and_reading_time() {
        // 中文字按字计，西文按词计
        let st = text_stats("这是中文标题\n\n和 English words here.");
        assert_eq!(st.cjk, 7);
        assert_eq!(st.words, 7 + 3);
        assert!(reading_minutes(&st) < 1.0);
    }

    #[test]
    fn elides_text() {
        assert_eq!(elide("abcdef", 4), "abc…");
        assert_eq!(elide("abc", 4), "abc");
        // 过短的字符串不做中间省略
        assert_eq!(elide_middle("abcdefghij", 6), "abcdefghij");
        assert_eq!(elide_middle("abcdefghijklmnop", 10).chars().count(), 10);
    }

    #[test]
    fn parses_and_formats_colors() {
        assert_eq!(
            hex_to_color("#FF8800"),
            Some(egui::Color32::from_rgb(255, 136, 0))
        );
        assert_eq!(hex_to_color("f80"), Some(egui::Color32::from_rgb(255, 136, 0)));
        assert_eq!(hex_to_color("#GGGGGG"), None);
        assert_eq!(color_to_hex(egui::Color32::from_rgb(255, 136, 0)), "#FF8800");
    }

    #[test]
    fn detects_binary_content() {
        assert!(looks_binary(&[0x00, 0x01, 0x02]));
        assert!(!looks_binary("普通文本内容".as_bytes()));
    }

    #[test]
    fn formats_time() {
        assert_eq!(format_relative(SystemTime::now()), "刚刚");
        assert_eq!(format_date(UNIX_EPOCH + Duration::from_secs(0)), "1970-01-01");
    }

    #[test]
    fn computes_text_statistics() {
        let st = text_stats("第一行\n第二行\n");
        assert_eq!(st.lines, 2);
        assert_eq!(st.chars, 8, "6 个汉字 + 2 个换行");
        assert_eq!(st.cjk, 6);
    }
}
