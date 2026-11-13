// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

//! 自定义 `easymd://` 资源加载器。
//!
//! 为什么需要它：Markdown 里的图片几乎都写成相对路径（`![](images/a.png)`），
//! 而 egui 内置的 `FileLoader` 只能处理绝对 `file://` 路径。这里注册一个专属协议，
//! 由本程序把 URI 还原成本地绝对路径后读取，同时保证不同文档的图片缓存互不冲突。

use std::path::PathBuf;
use std::sync::Arc;

use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};

pub const SCHEME: &str = "easymd://";

#[derive(Default)]
pub struct EasyMdLoader {
    /// 最近一次渲染文档所在目录，用于兜底解析绝对路径写法
    pub current_dir: Arc<std::sync::Mutex<Option<PathBuf>>>,
    cache: Arc<std::sync::Mutex<ahash_like::Map>>,
}

/// 用一个极简的内部 map 包装，避免额外依赖。
mod ahash_like {
    use std::collections::HashMap;
    use std::sync::Arc;

    pub type Map = HashMap<String, Entry>;
    pub type Entry = core::task::Poll<Result<Loaded, String>>;

    #[derive(Clone)]
    pub struct Loaded {
        pub bytes: Arc<[u8]>,
    }
}

impl EasyMdLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(ctx: &egui::Context) -> Arc<Self> {
        let loader = Arc::new(Self::new());
        ctx.add_bytes_loader(loader.clone());
        loader
    }

    pub fn set_current_dir(&self, dir: Option<PathBuf>) {
        if let Ok(mut d) = self.current_dir.lock() {
            *d = dir;
        }
    }

    fn resolve(&self, uri: &str) -> Option<PathBuf> {
        let rest = uri.strip_prefix(SCHEME)?;
        let cleaned = rest.split('#').next().unwrap_or(rest);
        let cleaned = crate::util::percent_decode(cleaned);
        let raw = PathBuf::from(&cleaned);
        if raw.exists() {
            return Some(raw);
        }
        // 兜底：URI 里带了文档目录前缀，但 Markdown 写的是绝对路径
        if let Ok(dir) = self.current_dir.lock() {
            if let Some(dir) = dir.as_ref() {
                let prefix = dir.to_string_lossy().replace('\\', "/");
                if let Some(tail) = cleaned.strip_prefix(&prefix) {
                    let tail = tail.trim_start_matches('/');
                    let p = PathBuf::from(tail);
                    if p.is_absolute() {
                        return Some(p);
                    }
                    let joined = dir.join(p);
                    if joined.exists() {
                        return Some(joined);
                    }
                }
            }
        }
        Some(raw)
    }
}

impl BytesLoader for EasyMdLoader {
    fn id(&self) -> &str {
        "easymd"
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !uri.starts_with(SCHEME) {
            return Err(LoadError::NotSupported);
        }
        let path = self.resolve(uri).ok_or_else(|| {
            LoadError::Loading(format!("无法解析资源地址 {uri}"))
        })?;

        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| LoadError::Loading("缓存锁失效".into()))?;
            if let Some(entry) = cache.get(uri) {
                return match entry {
                    core::task::Poll::Ready(Ok(v)) => Ok(BytesPoll::Ready {
                        size: None,
                        bytes: Bytes::Shared(v.bytes.clone()),
                        mime: None,
                    }),
                    core::task::Poll::Ready(Err(e)) => Err(LoadError::Loading(e.clone())),
                    core::task::Poll::Pending => Ok(BytesPoll::Pending { size: None }),
                };
            }
        }
        // 标记为加载中，然后放到后台线程读取，避免大图卡住渲染
        {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| LoadError::Loading("缓存锁失效".into()))?;
            cache.insert(uri.to_string(), core::task::Poll::Pending);
        }
        let cache = self.cache.clone();
        let uri_owned = uri.to_string();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("easymd-img".into())
            .spawn(move || {
                let result = std::fs::read(&path)
                    .map(|bytes| ahash_like::Loaded {
                        bytes: bytes.into(),
                    })
                    .map_err(|e| format!("读取 {} 失败: {e}", path.display()));
                if let Ok(mut cache) = cache.lock() {
                    cache.insert(uri_owned, core::task::Poll::Ready(result));
                }
                ctx.request_repaint();
            })
            .ok();
        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(uri);
        }
    }

    fn forget_all(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .map(|c| {
                c.values()
                    .map(|e| match e {
                        core::task::Poll::Ready(Ok(v)) => v.bytes.len(),
                        core::task::Poll::Ready(Err(e)) => e.len(),
                        core::task::Poll::Pending => 0,
                    })
                    .sum()
            })
            .unwrap_or(0)
    }

    fn has_pending(&self) -> bool {
        self.cache
            .lock()
            .map(|c| c.values().any(|e| e.is_pending()))
            .unwrap_or(false)
    }
}
