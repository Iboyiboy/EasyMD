//! 字体系统：
//! - 后台线程枚举系统字体（fontdb），不阻塞启动；
//! - 按需从字体文件加载并安装到 egui，中文/emoji 回退自动补齐；
//! - 通过修改字体 `lineGap` 实现真正的行距控制（阅读排版关键项），失败时自动退回原字体。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// 字体家族信息（用于设置界面的选择列表）。
#[derive(Debug, Clone)]
pub struct FamilyInfo {
    pub name: String,
    pub mono: bool,
    /// 是否包含中日韩字形（依据名称与文件体积推断）
    pub cjk: bool,
    pub faces: usize,
    pub file: PathBuf,
    pub index: u32,
}

pub struct FontScan {
    pub families: Vec<FamilyInfo>,
}

/// 在后台线程扫描系统字体。返回接收端，扫描完成后可取回结果。
pub fn scan_async() -> Receiver<FontScan> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("easymd-font-scan".into())
        .spawn(move || {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            let mut families: Vec<FamilyInfo> = Vec::new();
            let mut seen: BTreeMap<String, usize> = BTreeMap::new();
            for face in db.faces() {
                let (file, index) = match &face.source {
                    fontdb::Source::File(p) => (p.clone(), face.index),
                    fontdb::Source::SharedFile(p, _) => (p.clone(), face.index),
                    fontdb::Source::Binary(_) => continue,
                };
                let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                for fam in face.families.iter() {
                    let name = fam.0.clone();
                    if name.is_empty() || name.starts_with('.') {
                        continue;
                    }
                    let cjk = looks_cjk(&name) || size > 6 * 1024 * 1024;
                    if let Some(i) = seen.get(&name) {
                        let e: &mut FamilyInfo = &mut families[*i];
                        e.faces += 1;
                        e.mono |= face.monospaced;
                        e.cjk |= cjk;
                    } else {
                        seen.insert(name.clone(), families.len());
                        families.push(FamilyInfo {
                            name,
                            mono: face.monospaced,
                            cjk,
                            faces: 1,
                            file: file.clone(),
                            index,
                        });
                    }
                }
            }
            families.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            let _ = tx.send(FontScan { families });
        })
        .ok();
    rx
}

fn looks_cjk(name: &str) -> bool {
    const KEYS: &[&str] = &[
        "yahei", "雅黑", "simsun", "宋体", "simhei", "黑体", "kaiti", "楷体", "fangsong", "仿宋",
        "noto sans cjk", "noto serif cjk", "noto sans sc", "noto serif sc", "source han",
        "sourcehan", "思源", "wqy", "wenquanyi", "文泉驿", "pingfang", "苹方", "heiti", "hiragino",
        "ms gothic", "meiryo", "yu gothic", "malgun", "nanum", "unifont", "ukai", "uming",
        "dengxian", "等线", "microsoft jhenghei", "微软正黑", "方正", "fzhei", "fangsongti",
    ];
    let l = name.to_lowercase();
    KEYS.iter().any(|k| l.contains(k))
}

/// 启动时立刻可用的默认字体（按平台常见路径直接探测，避免等待系统扫描导致的字体跳动）。
pub struct QuickFonts {
    pub prop: Option<FamilyInfo>,
    pub mono: Option<FamilyInfo>,
}

impl QuickFonts {
    pub fn families(&self) -> Vec<FamilyInfo> {
        let mut v = Vec::new();
        if let Some(p) = &self.prop {
            v.push(p.clone());
        }
        if let Some(m) = &self.mono {
            v.push(m.clone());
        }
        v
    }
}

/// 直接按已知路径挑默认字体（同步、很快；找不到就交给后台扫描）。
pub fn quick_defaults() -> QuickFonts {
    // 候选表每一项是 (显示名, 文件路径, TTC 内的 face 索引)。
    // TTC 一个文件常含多个字面（NotoSansCJK 的 0=JP、2=SC…），索引写错会拿到别的语言版本：
    // 中文能显示，但用的是日文字形。所以索引必须跟字体一起写死。
    let prop_candidates: &[(&str, &str, u32)] = if cfg!(windows) {
        &[
            ("Microsoft YaHei", "C:/Windows/Fonts/msyh.ttc", 0),
            ("Microsoft YaHei", "C:/Windows/Fonts/msyh.ttf", 0),
            ("DengXian", "C:/Windows/Fonts/Deng.ttf", 0),
            ("SimHei", "C:/Windows/Fonts/simhei.ttf", 0),
            ("SimSun", "C:/Windows/Fonts/simsun.ttc", 0),
            ("Segoe UI", "C:/Windows/Fonts/segoeui.ttf", 0),
            ("Arial", "C:/Windows/Fonts/arial.ttf", 0),
        ]
    } else if cfg!(target_os = "macos") {
        &[
            ("PingFang SC", "/System/Library/Fonts/PingFang.ttc", 0),
            ("Hiragino Sans GB", "/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
            ("Helvetica", "/System/Library/Fonts/Helvetica.ttc", 0),
        ]
    } else {
        &[
            // NotoSansCJK-Regular.ttc 的字面顺序：0=JP 1=KR 2=SC 3=TC 4=HK
            (
                "Noto Sans CJK SC",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                2,
            ),
            (
                "Noto Sans CJK SC",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.otf",
                0,
            ),
            (
                "Source Han Sans SC",
                "/usr/share/fonts/opentype/source-han-sans/SourceHanSansSC-Regular.otf",
                0,
            ),
            ("WenQuanYi Micro Hei", "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
            ("Noto Sans SC", "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf", 0),
            (
                "Droid Sans Fallback",
                "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
                0,
            ),
            ("Noto Sans", "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", 0),
            ("DejaVu Sans", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
            (
                "Liberation Sans",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                0,
            ),
        ]
    };
    let mono_candidates: &[(&str, &str, u32)] = if cfg!(windows) {
        &[
            ("Cascadia Mono", "C:/Windows/Fonts/CascadiaMono.ttf", 0),
            ("Consolas", "C:/Windows/Fonts/consola.ttf", 0),
            ("Lucida Console", "C:/Windows/Fonts/lucon.ttf", 0),
            ("Courier New", "C:/Windows/Fonts/cour.ttf", 0),
        ]
    } else if cfg!(target_os = "macos") {
        &[
            ("Menlo", "/System/Library/Fonts/Menlo.ttc", 0),
            ("Monaco", "/System/Library/Fonts/Monaco.ttf", 0),
        ]
    } else {
        &[
            (
                "DejaVu Sans Mono",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                0,
            ),
            (
                "Noto Sans Mono",
                "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
                0,
            ),
            (
                "Liberation Mono",
                "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
                0,
            ),
        ]
    };

    let find = |cands: &[(&str, &str, u32)], mono: bool| -> Option<FamilyInfo> {
        for (name, path, index) in cands {
            let p = std::path::Path::new(path);
            if p.is_file() {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                return Some(FamilyInfo {
                    name: (*name).to_string(),
                    mono,
                    cjk: !mono && (looks_cjk(name) || size > 6 * 1024 * 1024),
                    faces: 1,
                    file: p.to_path_buf(),
                    index: *index,
                });
            }
        }
        None
    };

    QuickFonts {
        prop: find(prop_candidates, false),
        mono: find(mono_candidates, true),
    }
}

/// 安装结果，用于界面提示。
#[derive(Debug, Clone, Default)]
pub struct InstallReport {
    pub prop: Option<String>,
    pub mono: Option<String>,
    pub cjk_fallback: Option<String>,
    pub line_gap_applied: bool,
    /// 已装入的图标 / 表情回退字体（文件名）
    pub symbol_fallbacks: Vec<String>,
    pub warnings: Vec<String>,
}

/// 依据平台挑一个默认正文字体（保证中文可读）。
pub fn pick_default_prop(families: &[FamilyInfo]) -> Option<String> {
    let prefer: &[&str] = if cfg!(windows) {
        &[
            "Microsoft YaHei UI",
            "Microsoft YaHei",
            "微软雅黑",
            "Noto Sans CJK SC",
            "Source Han Sans SC",
            "思源黑体",
            "DengXian",
            "等线",
            "PingFang SC",
            "Segoe UI",
            "Arial",
        ]
    } else if cfg!(target_os = "macos") {
        &["PingFang SC", "Hiragino Sans GB", "Heiti SC", "Helvetica Neue", "Arial"]
    } else {
        &[
            "Noto Sans CJK SC",
            "Source Han Sans SC",
            "Noto Sans SC",
            "WenQuanYi Micro Hei",
            "文泉驿微米黑",
            "Noto Sans",
            "DejaVu Sans",
            "Liberation Sans",
            "Ubuntu",
        ]
    };
    pick(families, prefer, |f| !f.mono)
}

pub fn pick_default_mono(families: &[FamilyInfo]) -> Option<String> {
    let prefer: &[&str] = if cfg!(windows) {
        &[
            "Cascadia Mono",
            "Cascadia Code",
            "Consolas",
            "JetBrains Mono",
            "Source Code Pro",
            "Noto Sans Mono CJK SC",
            "DejaVu Sans Mono",
            "Courier New",
        ]
    } else if cfg!(target_os = "macos") {
        &["SF Mono", "Menlo", "JetBrains Mono", "Monaco", "Courier New"]
    } else {
        &[
            "JetBrains Mono",
            "Noto Sans Mono CJK SC",
            "Source Code Pro",
            "DejaVu Sans Mono",
            "Noto Sans Mono",
            "Liberation Mono",
            "Ubuntu Mono",
        ]
    };
    pick(families, prefer, |f| f.mono)
}

fn pick(
    families: &[FamilyInfo],
    prefer: &[&str],
    filter: impl Fn(&FamilyInfo) -> bool,
) -> Option<String> {
    for want in prefer {
        let w = want.to_lowercase();
        if let Some(f) = families
            .iter()
            .find(|f| f.name.to_lowercase() == w && filter(f))
        {
            return Some(f.name.clone());
        }
    }
    for want in prefer {
        let w = want.to_lowercase();
        if let Some(f) = families
            .iter()
            .find(|f| f.name.to_lowercase().contains(&w) && filter(f))
        {
            return Some(f.name.clone());
        }
    }
    families.iter().find(|f| filter(f)).map(|f| f.name.clone())
}

/// 找一个可靠的中日韩回退字体。
pub fn pick_cjk_fallback(families: &[FamilyInfo]) -> Option<String> {
    let prefer: &[&str] = &[
        "Microsoft YaHei",
        "微软雅黑",
        "Noto Sans CJK SC",
        "Source Han Sans SC",
        "思源黑体",
        "PingFang SC",
        "WenQuanYi Micro Hei",
        "SimHei",
        "SimSun",
        "Microsoft JhengHei",
    ];
    for want in prefer {
        let w = want.to_lowercase();
        if let Some(f) = families
            .iter()
            .find(|f| f.name.to_lowercase() == w && f.cjk)
        {
            return Some(f.name.clone());
        }
    }
    families
        .iter()
        .find(|f| f.cjk && !f.mono)
        .map(|f| f.name.clone())
}

/// 图标 / 表情回退字体候选（系统自带），按优先级排列。缺字形会渲染成方框，
/// 这几款覆盖了常见 emoji、勾叉、警告、箭头、几何符号等。
fn symbol_font_candidates() -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    {
        vec![
            // Segoe UI Emoji：emoji（COLR 字体，基字形为单色轮廓）
            std::path::PathBuf::from("C:/Windows/Fonts/seguiemj.ttf"),
            // Segoe UI Symbol：勾叉、几何、箭头、货币等符号
            std::path::PathBuf::from("C:/Windows/Fonts/seguisym.ttf"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![std::path::PathBuf::from(
            "/System/Library/Fonts/Apple Symbols.ttf",
        )]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // 各发行版路径略有差异，都列上（存在就用，不存在跳过）：
        // - Debian/Ubuntu：Symbola 在 fonts-symbola，DejaVu 在 fonts-dejavu-core
        // - Arch/Fedora：Noto Emoji 常在 opentype 目录
        vec![
            std::path::PathBuf::from("/usr/share/fonts/truetype/noto/NotoEmoji-Regular.ttf"),
            std::path::PathBuf::from("/usr/share/fonts/opentype/noto/NotoEmoji-Regular.ttf"),
            std::path::PathBuf::from("/usr/share/fonts/truetype/ancient-scripts/Symbola_hint.ttf"),
            std::path::PathBuf::from("/usr/share/fonts/truetype/symbola/Symbola_hint.ttf"),
            std::path::PathBuf::from("/usr/share/fonts/TTF/Symbola.ttf"),
            std::path::PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            std::path::PathBuf::from("/usr/share/fonts/dejavu/DejaVuSans.ttf"),
        ]
    }
}

/// 把选定字体安装到 egui。`line_height` 为目标行高（相对字号倍数，如 1.55）。
pub fn install(
    ctx: &egui::Context,
    scan: &FontScan,
    prop: Option<&str>,
    mono: Option<&str>,
    line_height: f32,
    base_size: f32,
) -> InstallReport {
    let mut report = InstallReport::default();
    let mut defs = FontDefinitions::default();

    let find = |name: &str| -> Option<&FamilyInfo> {
        scan.families.iter().find(|f| f.name == name)
    };

    let prop_info = prop.and_then(find).or_else(|| {
        report.warnings.push(format!(
            "未找到字体 {}，已使用默认字体",
            prop.unwrap_or("(未指定)")
        ));
        None
    });
    let mono_info = mono.and_then(find);

    // 行高（相对字号的 em 比例）：egui 的行高 = ascent - descent + lineGap，
    // 这里把「多出来的行距」作为 lineGap 补进字体。
    //
    // 注意换算：lineGap 是字体单位，而各字体 unitsPerEm 不同（雅黑/Consolas 是 2048，
    // 思源黑体是 1000）。这里统一按 2048 折算，让「行距」设置在任何字体上都得到
    // 同样的实际效果（历史行为 = 雅黑下的效果，保持不变）。
    const NATURAL_ROW_RATIO: f32 = 1.32;
    const REF_UPEM: f32 = 2048.0;
    let gap_em = ((line_height - NATURAL_ROW_RATIO).max(0.0)) * (1000.0 / REF_UPEM);
    let _ = base_size;

    // ---- 正文字体：既提供行距，也作为所有字体共用的度量基准 ----
    let mut prop_key = None;
    let mut reference: Option<FaceMetrics> = None;
    if let Some(info) = prop_info {
        match load_font(&info.file, info.index, gap_em, None) {
            Ok((data, applied, metrics)) => {
                let key = "easymd-prop".to_string();
                defs.font_data.insert(key.clone(), Arc::new(data));
                prop_key = Some(key);
                report.prop = Some(info.name.clone());
                report.line_gap_applied |= applied;
                reference = metrics;
            }
            Err(e) => report.warnings.push(format!("加载字体 {} 失败: {e}", info.name)),
        }
    }
    // 基准再抬到「汉字墨迹也放得下」的下限：正文若选了很紧凑的西文字体，
    // 汉字会被挤出行外（越界到上一行）。抬升同时作用于所有字体，因此不影响对齐。
    let unify = reference.map(unified_metrics);

    let mut mono_key = None;
    if let Some(info) = mono_info {
        match load_font(&info.file, info.index, gap_em, unify) {
            Ok((data, applied, _)) => {
                let key = "easymd-mono".to_string();
                defs.font_data.insert(key.clone(), Arc::new(data));
                mono_key = Some(key);
                report.mono = Some(info.name.clone());
                report.line_gap_applied |= applied;
            }
            Err(e) => report.warnings.push(format!("加载字体 {} 失败: {e}", info.name)),
        }
    }

    // 中文回退：必须同时挂到「正文」和「等宽」两个家族上，
    // 否则代码块 / 行内代码里的中文会显示成方块（Consolas 等西文等宽字体没有中文字形）。
    // 如果主字体本身就是中文回退字体，则直接复用同一份数据，不重复占内存。
    let mut cjk_key = None;
    if let Some(cjk_name) = pick_cjk_fallback(&scan.families) {
        if report.prop.as_deref() == Some(cjk_name.as_str()) {
            cjk_key = prop_key.clone();
        } else if report.mono.as_deref() == Some(cjk_name.as_str()) {
            cjk_key = mono_key.clone();
        } else if let Some(info) = find(&cjk_name) {
            if let Ok((data, _, _)) = load_font(&info.file, info.index, gap_em, unify) {
                let key = "easymd-cjk".to_string();
                defs.font_data.insert(key.clone(), Arc::new(data));
                cjk_key = Some(key);
                report.cjk_fallback = Some(cjk_name);
            }
        }
    }

    // 图标 / 表情回退：系统自带的表情与符号字体，装在中文回退之后、egui 内置字体之前。
    //
    // 注意：epaint 的光栅器只用"白色 + 覆盖度"画字形轮廓（见 epaint `text/font.rs` 里
    // 的 `OpaqueColor::WHITE` 与文本着色方式），彩色字形（COLR / CBDT 位图）用不上，
    // 所以这里解决的是"缺字形渲染成方框"的问题，图标会是单色。
    let mut symbol_keys: Vec<String> = Vec::new();
    for path in symbol_font_candidates() {
        if !path.is_file() {
            continue;
        }
        if [prop_info, mono_info].into_iter().flatten().any(|f| f.file == path) {
            continue; // 跟已装入的字体是同一个文件，不重复占内存
        }
        let key = format!("easymd-sym{}", symbol_keys.len());
        match load_font(&path, 0, gap_em, unify) {
            Ok((data, _, _)) => {
                defs.font_data.insert(key.clone(), Arc::new(data));
                symbol_keys.push(key);
                report.symbol_fallbacks.push(
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string()),
                );
            }
            Err(e) => report
                .warnings
                .push(format!("加载图标字体 {} 失败: {e}", path.display())),
        }
    }

    // 组装字体家族：主字体 -> 中文回退 -> 图标回退 -> egui 内置字体
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = defs.families.entry(family.clone()).or_default();
        let mut front: Vec<String> = Vec::new();
        match &family {
            FontFamily::Proportional => {
                if let Some(k) = &prop_key {
                    front.push(k.clone());
                }
            }
            FontFamily::Monospace => {
                if let Some(k) = &mono_key {
                    front.push(k.clone());
                } else if let Some(k) = &prop_key {
                    // 未选等宽字体时，代码块用等宽系统字体兜底
                    front.push(k.clone());
                }
            }
            _ => {}
        }
        if let Some(k) = &cjk_key {
            front.push(k.clone());
        }
        front.extend(symbol_keys.iter().cloned());
        // 去重：主字体本身就是中文回退字体时，同一个 key 会进来两次
        front.dedup();
        for (i, k) in front.into_iter().enumerate() {
            list.insert(i, k);
        }
    }

    ctx.set_fonts(defs);

    // 字体文件不支持改行距时，退化为 TextEdit 行距（保证编辑器仍可舒适阅读）
    if gap_em > 0.0 && !report.line_gap_applied {
        report.warnings.push(
            "当前字体不支持行距调整，行距设置仅在编辑器中生效（可换用思源黑体 / Noto Sans CJK）"
                .into(),
        );
    }
    report
}

/// 读取字体文件、调整纵向度量。返回 (FontData, 是否改过度量, 改后的度量)。
///
/// - `gap_em`：行间距（相对字号的 em 比例），作为行高的一部分。
/// - `unify`：非空时忽略 `gap_em`，直接把本 face 的三项度量统一成这组值
///   （按各自 unitsPerEm 换算比例），使混排字体共享同一基线。
///
/// 这里用「只读 + 写时复制」的文件映射，而不是把文件整个读进内存：
/// - 中文字体动辄 20MB，映射后由系统按需分页、可被回收，也不计入进程私有内存；
/// - `FontData::from_static` 让 epaint 直接借用这块内存（`Cow::Borrowed`），
///   不再像 `from_owned` 那样在内部再克隆一份（实测能省下整整一份字体大小的内存）；
/// - 度量修补只改几个字节，只有被改动的内存页会变成私有页。
fn load_font(
    path: &Path,
    index: u32,
    gap_em: f32,
    unify: Option<FaceMetrics>,
) -> Result<(FontData, bool, Option<FaceMetrics>), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut mapping = unsafe { memmap2::MmapOptions::new().map_copy(&file) }
        .map_err(|e| format!("映射字体文件失败: {e}"))?;
    let applied = {
        let bytes = &mut mapping[..];
        match read_face_metrics(bytes, index) {
            None => false,
            Some(own) => match unify {
                // 统一到基准度量：按 em 比例换算到本字体的单位
                Some(target) => {
                    let (a, d, g) = target.scaled_to(own.upem);
                    patch_face_metrics(bytes, index, a, d, g)
                }
                // 只把行间距按本字体单位补齐
                None => {
                    let gap = (gap_em.max(0.0) * own.upem as f32).round() as i16;
                    gap > 0
                        && patch_face_metrics(bytes, index, own.ascent, own.descent, gap)
                }
            },
        }
    };
    // 度量修补完成后立刻降级为只读视图：写时复制视图会被系统按字体大小计入提交内存，
    // 只读视图则完全不占（对 20MB 级别的中文字体就是 20MB 的差别）。
    let bytes = match mapping.make_read_only() {
        Ok(read_only) => leak_read_only(read_only),
        Err(_) => return Err("字体映射无法降级为只读".into()),
    };
    let metrics = read_face_metrics(bytes, index);
    let mut data = FontData::from_static(bytes);
    data.index = index;
    Ok((data, applied, metrics))
}

/// 把只读映射泄漏成 `'static` 切片。
///
/// egui 会在整个进程生命周期内按需光栅化字形，字体数据必须一直有效；
/// 映射由系统按需分页、可被回收、可与其他进程共享，因此不会占用进程私有内存。
/// 泄漏是刻意为之。
fn leak_read_only(mapping: memmap2::Mmap) -> &'static [u8] {
    let leaked: &'static memmap2::Mmap = Box::leak(Box::new(mapping));
    // 安全：映射不会被释放，且此后再无任何可变借用
    unsafe { core::slice::from_raw_parts(leaked.as_ptr(), leaked.len()) }
}

/// 一个 face 的纵向度量（字体单位）。`upem` 用来在字体之间换算「em 比例」。
///
/// 为什么要动这些值：epaint 的 `text_layout.rs` 里
/// `glyph.pos.y（基线）= 本 face 的 ascent + …`，所以同一行里混排不同 face
/// （正文 / 等宽 / 中文回退）时，谁 ascent 大谁的基线就低，看起来就是
/// 「代码片段、数字比汉字高一截」。把各 face 的度量统一成同一组比例即可对齐。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetrics {
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
    pub upem: u16,
}

impl FaceMetrics {
    fn scaled_to(self, upem: u16) -> (i16, i16, i16) {
        let k = upem as f32 / self.upem.max(1) as f32;
        (
            (self.ascent as f32 * k).round() as i16,
            (self.descent as f32 * k).round() as i16,
            (self.line_gap as f32 * k).round() as i16,
        )
    }
}


/// 汉字墨迹大致在基线上方 0.88em、下方 0.12em；基准字体比这更紧时就抬到放得下汉字。
const CJK_INK_ASCENT_EM: f32 = 0.90;
const CJK_INK_DESCENT_EM: f32 = 0.14;

/// 把基准字体的度量抬到能容纳汉字墨迹的下限，作为所有字体共用的目标度量。
fn unified_metrics(base: FaceMetrics) -> FaceMetrics {
    let upem = base.upem.max(16) as f32;
    let ascent_em = (base.ascent as f32 / upem).max(CJK_INK_ASCENT_EM);
    let descent_em = (-(base.descent as f32) / upem).max(CJK_INK_DESCENT_EM);
    FaceMetrics {
        ascent: (ascent_em * upem).round() as i16,
        descent: -((descent_em * upem).round() as i16),
        line_gap: base.line_gap,
        upem: base.upem,
    }
}

/// sfnt 中当前 face 的 table 目录起始偏移（TTC 需要先跳到对应 face）。
fn sfnt_base(d: &[u8], face_index: u32) -> Option<usize> {
    if d.len() < 12 {
        return None;
    }
    let base = match read_tag(d, 0)? {
        tag if tag == *b"ttcf" => {
            let num_fonts = read_u32(d, 8)? as usize;
            let idx = face_index as usize;
            if num_fonts == 0 || num_fonts > 64 || idx >= num_fonts {
                return None;
            }
            read_u32(d, 12 + idx * 4)? as usize
        }
        _ => 0,
    };
    (base + 12 <= d.len()).then_some(base)
}

/// 遍历当前 face 的所有 table 记录，回调 (tag, 数据偏移, 长度)。
fn for_each_table(d: &[u8], face_index: u32, mut f: impl FnMut([u8; 4], usize, usize)) {
    let Some(base) = sfnt_base(d, face_index) else {
        return;
    };
    let Some(num_tables) = read_u16(d, base + 4) else {
        return;
    };
    let num_tables = num_tables as usize;
    if num_tables == 0 || num_tables > 512 {
        return;
    }
    for i in 0..num_tables {
        let rec = base + 12 + i * 16;
        if rec + 16 > d.len() {
            return;
        }
        let (Some(tag), Some(off), Some(len)) =
            (read_tag(d, rec), read_u32(d, rec + 8), read_u32(d, rec + 12))
        else {
            return;
        };
        let (off, len) = (off as usize, len as usize);
        if off + len <= d.len() {
            f(tag, off, len);
        }
    }
}

/// 读取 face 的纵向度量（hhea 的 ascender/descender/lineGap + head 的 unitsPerEm）。
///
/// skrifa 取的是 `hhea`（未设置 USE_TYPO_METRICS 时），实测雅黑 2167/2048、
/// Consolas 1521/2048 都与渲染结果吻合，所以这里以 hhea 为准。
pub fn read_face_metrics(d: &[u8], face_index: u32) -> Option<FaceMetrics> {
    let mut ascent = None;
    let mut descent = None;
    let mut line_gap = None;
    let mut upem = None;
    for_each_table(d, face_index, |tag, off, len| {
        if tag == *b"hhea" && len >= 10 {
            // version(4) ascender(2) descender(2) lineGap(2)
            ascent = read_i16(d, off + 4);
            descent = read_i16(d, off + 6);
            line_gap = read_i16(d, off + 8);
        } else if tag == *b"head" && len >= 20 {
            upem = read_u16(d, off + 18);
        }
    });
    Some(FaceMetrics {
        ascent: ascent?,
        descent: descent?,
        line_gap: line_gap?,
        upem: upem?.max(16),
    })
}

/// 写入一套纵向度量：`hhea` 的 ascender/descender/lineGap 与 `OS/2` 的
/// sTypoAscender/sTypoDescender/sTypoLineGap（OS/2 里这三个值分别在偏移 68/70/72）。
///
/// 两处都写是因为不同引擎可能选其中一表：skrifa 在设了 USE_TYPO_METRICS 的字体上
/// 会优先用 OS/2，写全了才不会随字体不同而行为不一致。
fn patch_face_metrics(
    out: &mut [u8],
    face_index: u32,
    ascent: i16,
    descent: i16,
    line_gap: i16,
) -> bool {
    // 先在只读遍历里定位两张表，再写入：遍历闭包已借用 `out`，闭包里不能再可变借用
    let mut hhea = None;
    let mut os2 = None;
    for_each_table(out, face_index, |tag, off, len| {
        if tag == *b"hhea" && len >= 10 {
            hhea = Some(off);
        } else if tag == *b"OS/2" && len >= 74 {
            os2 = Some(off);
        }
    });

    let write = |out: &mut [u8], off: usize, values: [i16; 3]| {
        for (i, v) in values.iter().enumerate() {
            let at = off + i * 2;
            out[at..at + 2].copy_from_slice(&v.to_be_bytes());
        }
    };
    let mut touched = false;
    if let Some(off) = hhea {
        // version(4) ascender(2) descender(2) lineGap(2)
        write(out, off + 4, [ascent, descent, line_gap]);
        touched = true;
    }
    if let Some(off) = os2 {
        // 68 / 70 / 72 = sTypoAscender / sTypoDescender / sTypoLineGap
        write(out, off + 68, [ascent, descent, line_gap]);
        touched = true;
    }
    touched
}

fn read_tag(d: &[u8], off: usize) -> Option<[u8; 4]> {
    if off + 4 > d.len() {
        return None;
    }
    Some([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn read_u16(d: &[u8], off: usize) -> Option<u16> {
    if off + 2 > d.len() {
        return None;
    }
    Some(u16::from_be_bytes([d[off], d[off + 1]]))
}

fn read_i16(d: &[u8], off: usize) -> Option<i16> {
    read_u16(d, off).map(|v| v as i16)
}

fn read_u32(d: &[u8], off: usize) -> Option<u32> {
    if off + 4 > d.len() {
        return None;
    }
    Some(u32::from_be_bytes([
        d[off],
        d[off + 1],
        d[off + 2],
        d[off + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 度量修补：hhea 与 OS/2 都要写对位置。
    /// （OS/2 的 68/70/72 是 sTypoAscender/Descender/LineGap，
    ///  早先的实现把 lineGap 写到 68 上，等于改坏了 sTypoAscender。）
    #[test]
    fn writes_metrics_in_place() {
        let path = "C:/Windows/Fonts/consola.ttf";
        if !std::path::Path::new(path).is_file() {
            return; // 非 Windows 环境跳过
        }
        let original = std::fs::read(path).unwrap();
        let mut data = original.clone();
        let before = read_face_metrics(&data, 0).expect("应能读出度量");
        let target = FaceMetrics {
            ascent: 2167,
            descent: -483,
            line_gap: 230,
            upem: before.upem,
        };
        let (a, d, g) = target.scaled_to(before.upem);
        assert!(patch_face_metrics(&mut data, 0, a, d, g), "应当成功写入");
        assert_eq!(data.len(), original.len(), "只改内容不改长度");
        assert_ne!(data, original, "应当有字节变化");
        let after = read_face_metrics(&data, 0).expect("改后仍应可读");
        assert_eq!((after.ascent, after.descent, after.line_gap), (a, d, g));
        assert_eq!(read_os2_typo(&data, 0), Some((a, d, g)), "OS/2 的 sTypo 三项应与 hhea 一致");
        // 无法识别的数据不应 panic
        let mut junk = vec![0u8; 32];
        assert!(!patch_face_metrics(&mut junk, 0, 1, 2, 3));
    }

    /// 同一行里混排「正文 / 等宽 / 中文」时，基线必须一致。
    ///
    /// epaint 按每个 face 自己的 hhea ascender 定位基线，所以只要各字体度量没统一，
    /// 渲染出来就是「代码片段、数字比汉字高一截」。这条测试盯的就是这个。
    #[test]
    fn mixed_families_share_baseline() {
        let ctx = egui::Context::default();
        // 字体表要等第一帧之后才可用
        let warm = |ctx: &egui::Context| {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 600.0),
            ));
            ctx.run_ui(input, |_ui| {}).textures_delta.clear();
        };
        warm(&ctx);

        let quick = quick_defaults();
        let scan = FontScan {
            families: quick.families(),
        };
        let cfg = crate::config::AppConfig::default();
        let report = install(
            &ctx,
            &scan,
            quick.prop.as_ref().map(|f| f.name.as_str()),
            quick.mono.as_ref().map(|f| f.name.as_str()),
            cfg.line_height,
            cfg.font_size,
        );
        if report.prop.is_none() {
            return; // 找不到系统字体（非 Windows 环境）就跳过
        }
        warm(&ctx); // 让新字体真正生效

        let size = cfg.font_size;
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        for (text, family) in [
            ("汉字", egui::FontFamily::Proportional),
            ("#ab12", egui::FontFamily::Proportional),
            ("code", egui::FontFamily::Monospace),
            ("汉字", egui::FontFamily::Monospace), // 代码块里的中文
        ] {
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::new(size, family),
                    ..Default::default()
                },
            );
        }
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        let mut baselines: Vec<(char, f32)> = Vec::new();
        for placed in &galley.rows {
            for g in &placed.row.glyphs {
                if g.chr != ' ' {
                    baselines.push((g.chr, g.pos.y));
                }
            }
        }
        assert!(baselines.len() >= 8, "应当排出一行混合文字");
        let lo = baselines.iter().map(|b| b.1).fold(f32::MAX, f32::min);
        let hi = baselines.iter().map(|b| b.1).fold(f32::MIN, f32::max);
        assert!(
            hi - lo <= 1.0,
            "同一行里各种字体的基线应当一致（差 {:.1}px）：{baselines:?}",
            hi - lo
        );
        // 行高也应一致：混排不会把行撑高
        for placed in &galley.rows {
            assert!(
                placed.row.size.y <= size * cfg.line_height + 2.0,
                "混排行高不应明显超过设定行高：{:.2}",
                placed.row.size.y
            );
        }
    }

    /// 走完整渲染管线：把一行混排行内代码/数字/符号的 markdown 真正画出来，
    /// 再从绘制指令里读每条字形基线的**屏幕坐标**，要求它们落在同一条线上。
    /// 这正是用户报告的场景（`code`、#、数字与汉字同行）。
    #[test]
    fn rendered_markdown_lies_on_one_baseline() {
        let ctx = egui::Context::default();
        let run = |ctx: &egui::Context, cache: &mut egui_commonmark::CommonMarkCache| {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 600.0),
            ));
            let mut full = ctx.run_ui(input, |ui| {
                egui_commonmark::CommonMarkViewer::new().show(
                    ui,
                    cache,
                    "中文正文 `code` 与 #1 混排。",
                );
            });
            full.textures_delta.clear();
            full
        };

        // 先跑一帧让默认字体生效，再装应用字体，再跑一帧
        let mut cache = egui_commonmark::CommonMarkCache::default();
        run(&ctx, &mut cache);
        let quick = quick_defaults();
        let scan = FontScan {
            families: quick.families(),
        };
        let cfg = crate::config::AppConfig::default();
        let report = install(
            &ctx,
            &scan,
            quick.prop.as_ref().map(|f| f.name.as_str()),
            quick.mono.as_ref().map(|f| f.name.as_str()),
            cfg.line_height,
            cfg.font_size,
        );
        if report.prop.is_none() {
            return; // 找不到系统字体（非 Windows 环境）就跳过
        }
        run(&ctx, &mut cache);
        let full = run(&ctx, &mut cache);

        let mut baselines: Vec<(char, f32)> = Vec::new();
        for clipped in &full.shapes {
            collect_baselines(&clipped.shape, &mut baselines);
        }
        assert!(baselines.len() >= 8, "应当画出正文里的字：{baselines:?}");
        let lo = baselines.iter().map(|b| b.1).fold(f32::MAX, f32::min);
        let hi = baselines.iter().map(|b| b.1).fold(f32::MIN, f32::max);
        assert!(
            hi - lo <= 1.0,
            "同一行的各种字体应落在同一条基线上（差 {:.1}px）：{baselines:?}",
            hi - lo
        );
    }

    /// 从绘制出的形状里收集 (字符, 基线屏幕纵坐标)
    fn collect_baselines(shape: &egui::Shape, out: &mut Vec<(char, f32)>) {
        match shape {
            egui::Shape::Text(t) => {
                for placed in &t.galley.rows {
                    for g in &placed.row.glyphs {
                        if !g.chr.is_whitespace() {
                            out.push((g.chr, t.pos.y + placed.pos.y + g.pos.y));
                        }
                    }
                }
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    collect_baselines(s, out);
                }
            }
            _ => {}
        }
    }

    /// 读 OS/2 的 sTypoAscender / sTypoDescender / sTypoLineGap（偏移 68/70/72）
    fn read_os2_typo(d: &[u8], face_index: u32) -> Option<(i16, i16, i16)> {
        let mut out = None;
        for_each_table(d, face_index, |tag, off, len| {
            if tag == *b"OS/2" && len >= 74 {
                let a = read_i16(d, off + 68);
                let b = read_i16(d, off + 70);
                let c = read_i16(d, off + 72);
                out = match (a, b, c) {
                    (Some(a), Some(b), Some(c)) => Some((a, b, c)),
                    _ => None,
                };
            }
        });
        out
    }
}

#[cfg(test)]
mod symbol_coverage_tests {
    use super::*;
    use crate::config::AppConfig;

    fn install_and_warm() -> egui::Context {
        let ctx = egui::Context::default();
        let warm = |ctx: &egui::Context| {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 600.0),
            ));
            ctx.run_ui(input, |_ui| {}).textures_delta.clear();
        };
        warm(&ctx);
        let quick = quick_defaults();
        let scan = FontScan {
            families: quick.families(),
        };
        let cfg = AppConfig::default();
        let report = install(
            &ctx,
            &scan,
            quick.prop.as_ref().map(|f| f.name.as_str()),
            quick.mono.as_ref().map(|f| f.name.as_str()),
            cfg.line_height,
            cfg.font_size,
        );
        if report.prop.is_none() {
            return ctx; // 没有系统字体（非 Windows 环境）就跳过检查
        }
        warm(&ctx);
        ctx
    }

    /// 渲染结果是否与"确定缺失字符"的替换字形一模一样（即方框）
    fn is_tofu(
        ctx: &egui::Context,
        c: char,
        font: &egui::FontId,
        tofu: (f32, f32, f32),
    ) -> bool {
        let m = glyph_metrics(ctx, c, font);
        (m.0 - tofu.0).abs() < 0.1 && (m.1 - tofu.1).abs() < 0.1
    }

    /// 单个字符渲染后的 (advance, 墨迹宽, 墨迹高)
    fn glyph_metrics(ctx: &egui::Context, c: char, font: &egui::FontId) -> (f32, f32, f32) {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        job.append(
            &c.to_string(),
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                ..Default::default()
            },
        );
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        for placed in &galley.rows {
            for g in &placed.row.glyphs {
                return (g.advance_width, g.uv_rect.size.x, g.uv_rect.size.y);
            }
        }
        (0.0, 0.0, 0.0)
    }

    /// 本机是否装了已知的中日韩字体（没装就跳过相关检查）
    fn has_any_cjk_font() -> bool {
        [
            "C:/Windows/Fonts/msyh.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/System/Library/Fonts/PingFang.ttc",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).is_file())
    }

    /// 默认字体必须真的能画出汉字（否则中文全是方框）。
    #[test]
    fn chinese_renders_with_real_glyph() {
        if !has_any_cjk_font() {
            return; // 本机没有任何中日韩字体
        }
        let ctx = install_and_warm();
        let cfg = AppConfig::default();
        let font = egui::FontId::proportional(cfg.font_size);
        let tofu = glyph_metrics(&ctx, '\u{E000}', &font);
        let m = glyph_metrics(&ctx, '中', &font);
        let is_tofu = (m.0 - tofu.0).abs() < 0.1 && (m.1 - tofu.1).abs() < 0.1;
        assert!(
            !is_tofu,
            "汉字渲染成了方框（宽={:.1} 墨迹={:.1}x{:.1}）——默认字体没选到含中文的字面",
            m.0, m.1, m.2
        );
    }

    /// TTC 里的 face 索引必须对着正确的字面：NotoSansCJK 的 0 是日文、2 才是简体中文，
    /// 索引写错时中文能显示、但用的是日文字形（阅读体验上很微妙，很难事后发现）。
    #[test]
    fn ttc_face_index_points_at_the_right_language() {
        let quick = quick_defaults();
        if let Some(prop) = &quick.prop {
            if prop.file.to_string_lossy().contains("NotoSansCJK") {
                assert!(
                    prop.name.contains("SC"),
                    "选择了简体中文字面，名字却是 {:?}",
                    prop.name
                );
                assert_eq!(prop.index, 2, "NotoSansCJK-Regular.ttc 里 2 才是 SC 字面");
            }
        }
    }

    /// 常见图标/符号必须有真字形（缺字形会渲染成方框）。
    ///
    /// 判定用"确定缺失的私用区字符"当基准：渲染结果与它完全一致就说明是替换字形。
    /// （不能用 `Fonts::has_glyph`：它拿"替换字符所在 face"做比较，
    ///  而替换字符 □ 本身就在图标字体里，会得出假阴性。）
    #[test]
    fn common_icons_render_a_real_glyph() {
        let ctx = install_and_warm();
        let cfg = AppConfig::default();
        let font = egui::FontId::proportional(cfg.font_size);
        let tofu = glyph_metrics(&ctx, '\u{E000}', &font);
        assert!(tofu.1 > 0.0, "私用区字符应当渲染成替换字形，作为基准");

        // 文档里常见的状态图标与符号（Unicode 9 及更早）。Linux 上的单色符号字体
        // （Symbola）只覆盖到 Unicode 9，这些都必须能画出来。
        let icons = ['✅', '❌', '⚠', '🔴', '📌', '✨', '🚀', '💡', '☑', '😀'];
        let mut missing = Vec::new();
        for c in icons {
            if is_tofu(&ctx, c, &font, tofu) {
                missing.push(c);
            }
        }
        assert!(
            missing.is_empty(),
            "{missing:?} 渲染成了方框——图标回退字体没生效（检查 fonts-symbola / Segoe UI Symbol 是否装上）"
        );

        // Unicode 10+ 的 emoji（如 🟢 🟣 🧑）：Windows 的 Segoe UI Emoji 覆盖，
        // Linux 上没有可用的单色新版 emoji 字体（Noto Color Emoji 是位图，epaint 不支持），
        // 因此只在 Windows 上要求。
        if cfg!(windows) {
            let mut missing = Vec::new();
            for c in ['🟢', '🟣', '🟡', '🧑'] {
                if is_tofu(&ctx, c, &font, tofu) {
                    missing.push(c);
                }
            }
            assert!(missing.is_empty(), "新版 emoji 缺字形: {missing:?}");
        }
    }
}
