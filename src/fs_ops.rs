// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 文件系统操作：编码感知的读写、带合法性校验的新建/重命名/移动/删除。

use std::path::{Path, PathBuf};

use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Gbk,
    Latin1,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Gbk => "GBK/GB18030",
            Self::Latin1 => "Latin1",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedText {
    pub text: String,
    pub encoding: Encoding,
    pub crlf: bool,
    pub bytes: u64,
    pub binary: bool,
}

/// 读取文本文件，自动识别编码与换行风格。
pub fn read_text(path: &Path) -> Result<LoadedText, String> {
    let raw = std::fs::read(path).map_err(|e| format!("读取失败: {e}"))?;
    let bytes = raw.len() as u64;
    let binary = util::looks_binary(&raw);
    let (text, encoding) = decode(&raw);
    let crlf = text.contains("\r\n");
    // 统一用 \n 内部处理，保存时再还原
    let text = if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    };
    Ok(LoadedText {
        text,
        encoding,
        crlf,
        bytes,
        binary,
    })
}

pub fn decode(raw: &[u8]) -> (String, Encoding) {
    if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _, _) = encoding_rs::UTF_8.decode(&raw[3..]);
        return (s.into_owned(), Encoding::Utf8Bom);
    }
    if raw.starts_with(&[0xFF, 0xFE]) {
        let (s, _, _) = encoding_rs::UTF_16LE.decode(&raw[2..]);
        return (s.into_owned(), Encoding::Utf16Le);
    }
    if raw.starts_with(&[0xFE, 0xFF]) {
        let (s, _, _) = encoding_rs::UTF_16BE.decode(&raw[2..]);
        return (s.into_owned(), Encoding::Utf16Be);
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        return (s.to_string(), Encoding::Utf8);
    }
    // 非 UTF-8：优先按 GBK/GB18030 解码（中文用户的常见情况）
    let (s, _, had_errors) = encoding_rs::GBK.decode(raw);
    if !had_errors {
        return (s.into_owned(), Encoding::Gbk);
    }
    let (s, _, _) = encoding_rs::WINDOWS_1252.decode(raw);
    (s.into_owned(), Encoding::Latin1)
}

/// 按原编码与换行风格写回文件（先写临时文件再替换，避免中途失败损坏原文件）。
pub fn write_text(
    path: &Path,
    text: &str,
    encoding: Encoding,
    crlf: bool,
) -> Result<(), String> {
    let text = if crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    let mut out: Vec<u8> = Vec::with_capacity(text.len() + 3);
    match encoding {
        Encoding::Utf8 => out.extend_from_slice(text.as_bytes()),
        Encoding::Utf8Bom => {
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            out.extend_from_slice(text.as_bytes());
        }
        Encoding::Utf16Le => {
            out.extend_from_slice(&[0xFF, 0xFE]);
            for u in text.encode_utf16() {
                out.extend_from_slice(&u.to_le_bytes());
            }
        }
        Encoding::Utf16Be => {
            out.extend_from_slice(&[0xFE, 0xFF]);
            for u in text.encode_utf16() {
                out.extend_from_slice(&u.to_be_bytes());
            }
        }
        Encoding::Gbk => {
            let (bytes, _, had_errors) = encoding_rs::GBK.encode(&text);
            if had_errors {
                // 有字符无法用 GBK 表示，转存为 UTF-8 更安全
                out.extend_from_slice(text.as_bytes());
            } else {
                out.extend_from_slice(&bytes);
            }
        }
        Encoding::Latin1 => {
            let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(&text);
            out.extend_from_slice(&bytes);
        }
    }
    let tmp = path.with_extension("easymd-tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("写入失败: {e}"))?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(format!("保存失败: {e}"))
        }
    }
}

pub fn create_dir(parent: &Path, name: &str) -> Result<PathBuf, String> {
    util::validate_name(name)?;
    let target = parent.join(name.trim());
    if target.exists() {
        return Err(format!("{} 已存在", name.trim()));
    }
    std::fs::create_dir(&target).map_err(|e| format!("创建文件夹失败: {e}"))?;
    Ok(target)
}

pub fn create_file(parent: &Path, name: &str, initial: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    util::validate_name(name)?;
    let name = if Path::new(name).extension().is_none() {
        format!("{name}.md")
    } else {
        name.to_string()
    };
    let target = parent.join(&name);
    if target.exists() {
        return Err(format!("{name} 已存在"));
    }
    std::fs::write(&target, initial).map_err(|e| format!("创建文件失败: {e}"))?;
    Ok(target)
}

/// 重命名（仅改名，不移动位置）。
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    util::validate_name(new_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定上级目录".to_string())?;
    let new_name = new_name.trim();
    let target = parent.join(new_name);
    if util::same_file(&target, path) {
        return Ok(target);
    }
    if target.exists() {
        return Err(format!("{new_name} 已存在"));
    }
    std::fs::rename(path, &target).map_err(|e| format!("重命名失败: {e}"))?;
    Ok(target)
}

/// 删除（文件或整个目录）。调用方必须先取得用户确认。
pub fn delete(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| format!("删除文件夹失败: {e}"))
    } else {
        std::fs::remove_file(path).map_err(|e| format!("删除文件失败: {e}"))
    }
}

/// 移动到目标文件夹（带完整合法性校验）。
pub fn move_into(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    if !src.exists() {
        return Err("源文件不存在".into());
    }
    if !dest_dir.is_dir() {
        return Err("目标不是文件夹".into());
    }
    if src.is_dir() && util::is_within(dest_dir, src) {
        return Err("不能把文件夹移动到它自己或它的子文件夹中".into());
    }
    let parent = src.parent().unwrap_or(Path::new(""));
    if util::same_file(parent, dest_dir) {
        return Err("已经在当前位置".into());
    }
    let name = util::file_name(src);
    let target = dest_dir.join(&name);
    if target.exists() {
        return Err(format!("目标位置已存在 “{name}”"));
    }
    match std::fs::rename(src, &target) {
        Ok(()) => Ok(target),
        Err(_) => {
            // 跨分区等情况下退化为“复制 + 删除”
            copy_recursive(src, &target)?;
            delete(src)?;
            Ok(target)
        }
    }
}

pub fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| format!("创建目录失败: {e}"))?;
        for entry in std::fs::read_dir(src).map_err(|e| format!("读取目录失败: {e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(p) = dest.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::copy(src, dest)
            .map(|_| ())
            .map_err(|e| format!("复制失败: {e}"))
    }
}

/// 在系统文件管理器中定位/打开。
pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if path.is_dir() {
            let _ = std::process::Command::new("explorer")
                .arg(path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        } else {
            // explorer /select,"路径" 需要整体作为一个参数传入
            let arg = format!("/select,{}", path.display());
            let _ = std::process::Command::new("explorer")
                .raw_arg(&arg)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

/// 用系统默认程序打开（例如 Markdown 里的外链）。
pub fn open_external(target: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `start` 把「第一个带引号的参数」当窗口标题，所以标题位必须占住。
        // 目标自带引号交给 cmd 解析：Rust 只在参数含空格时才自动加引号，
        // 于是 `...?a=1&b=2` 这类地址会被 cmd 当成命令分隔符切开。raw_arg 原样拼接。
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/C")
            .raw_arg(format!("start \"\" \"{target}\""))
            .creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(target).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

