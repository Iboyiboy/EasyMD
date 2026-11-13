# EasyMD

[English](README.en.md) | **简体中文**

> 超轻量的本地 Markdown 阅读与管理器 —— 面向知识库、技术文档与长文阅读
> 纯 Rust 实现，单文件可执行，Windows / Linux 通用，**不依赖 Electron、WebView2、Node 或 .NET**

![阅读模式](docs/screenshots/read-light.png)

<table>
<tr>
<td width="50%"><img src="docs/screenshots/split-dark.png" alt="编辑模式：左源码 + 右实时预览（深色主题）"></td>
<td width="50%"><img src="docs/screenshots/edit-sepia.png" alt="编辑模式（羊皮纸主题，光标所在块在预览中高亮）"></td>
</tr>
</table>

## 特性

**阅读**
- 打开即阅读：正文栏宽居中，字号 / 行距 / 栏宽都按长文阅读调优，且可逐项调整
- 常用 Markdown 全覆盖：标题、段落、引用（含嵌套）、有序 / 无序 / 任务列表、表格、代码块语法高亮、图片、分隔线
- **宽表格与代码长行自动折行**，不需要横向滚动
- 标题锚点导航：大纲页签直接跳转；文档内的相对链接（`../`、`%20`、`#锚点`）可在应用内打开
- **每篇文档独立记忆阅读位置**，切走再回来、重启之后仍在原处
- 8 套内置主题（纸白 / 纯白 / 羊皮纸 / 护眼绿 / 石墨 / 午夜蓝 / 暖夜 / 极地），可自定义配色、背景图片与系统字体
- 禅模式（`F11`）隐藏全部界面元素，只留正文

**编辑**
- **左侧源码 + 右侧实时预览**：分隔条可拖动，两栏行级联动 —— 滚动同步、光标所在块在预览中精确高亮、点预览可把编辑器定位到对应行
- 工具栏一键插入标题、粗体、斜体、删除线、行内代码、代码块、引用、列表、链接、图片、表格、分隔线
- 自动保存；状态栏实时显示字数 / 行数 / 阅读时长 / 文件体积

**管理与查找**
- 目录树按需加载 + 虚拟列表渲染，几万条目依然流畅；新建 / 重命名 / 删除 / 拖拽移动（删除需二次确认）
- 文件名快速搜索、自然排序（`第2章` 在 `第10章` 前）、按修改时间排序、文件夹优先
- 仓库全文检索（后台线程，不卡界面）：结果显示文件名 + 行号 + 命中上下文，点击直达
- 外部改动自动重新载入；与未保存内容冲突时只提示、不打断

## 快速开始

1. 下载 `easymd.exe`（Windows）或 `easymd`（Linux），直接运行，无需安装。
2. 首次启动点「打开文档仓库」选择你的 Markdown 文件夹；之后会自动回到上次的仓库与阅读位置。

也可以用命令行打开指定目录或文件，或者把它们直接拖进窗口：

```bash
easymd                    # 打开上次的仓库
easymd ~/notes            # 指定仓库目录
easymd ~/notes/readme.md  # 直接打开某个文件（自动以所在目录为仓库）
```

仓库自带 `examples/` 示例文档，可以先用它熟悉各项功能。

## 快捷键

| 快捷键 | 作用 | | 快捷键 | 作用 |
| --- | --- | --- | --- | --- |
| `Ctrl+O` | 打开文档仓库 | | `Ctrl+B` | 显示 / 隐藏侧栏 |
| `Ctrl+Shift+O` | 打开单个文件 | | `Ctrl+E` | 编辑模式（左源码 + 右预览） |
| `Ctrl+S` | 保存当前文档 | | `Ctrl+R` | 切换到阅读模式 |
| `Ctrl+N` | 新建文件 | | `Ctrl+D` | 深浅色主题切换 |
| `Ctrl+Shift+N` | 新建文件夹 | | `Ctrl+=` / `Ctrl+-` | 放大 / 缩小字号 |
| `F2` | 重命名选中项 | | `Ctrl+0` | 重置字号 |
| `Delete` | 删除选中项（需确认） | | `Ctrl+W` | 关闭当前文档 |
| `F5` | 刷新目录树 | | `Alt+←` / `Alt+→` | 后退 / 前进 |
| `Ctrl+F` | 搜索文件名 | | `F11` | 禅模式 |
| `Ctrl+Shift+F` | 仓库全文检索 | | `Ctrl+,` | 打开设置 |
| `Ctrl+/` | 快捷键列表 | | `Ctrl+滚轮` | 缩放字号 |
| `↑` `↓` | 目录树上下移动 | | `←` `→` | 折叠 / 展开文件夹 |
| `Enter` | 打开选中文件 | | `Space` | 展开 / 折叠文件夹 |
| `Esc` | 关闭对话框 / 退出搜索 / 退出禅模式 | | | |

## Markdown 支持范围

渲染基于 [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark)（CommonMark 规范实现）+ [egui_commonmark](https://github.com/lampsitter/egui_commonmark)，已启用 GFM 表格、任务列表、删除线、脚注、定义列表、标题属性（`{#id}` 锚点）、嵌套引用块、相对路径图片与仓库内文档互链。

暂不支持：内联 HTML 渲染（按纯文本显示）、远程图片自动下载（离线优先）、LaTeX 公式渲染。

## 配置与数据

- **Windows**：`%APPDATA%\EasyMD\config.json`
- **Linux**：`$XDG_CONFIG_HOME/EasyMD/config.json`（默认 `~/.config/EasyMD/config.json`）
- **便携模式**：在可执行文件同级放一个 `easymd.json`，配置就写到那里（适合放 U 盘随身携带）
- 日志：同目录下的 `easymd.log`。删除配置文件即可恢复默认状态。

配置内保存：上次打开的仓库、最近打开的仓库与文件、视图模式、主题与自定义配色、字体 / 字号 / 行距 / 栏宽、排序与显示偏好、每篇文档的阅读位置、窗口尺寸与位置。

## 从源码构建

需要 Rust 1.82 或更高版本（<https://rustup.rs>）。

```bash
build.cmd                # Windows（CMD / PowerShell）
./build.sh               # Linux / macOS / Git Bash
cargo build --release    # 手动构建；产物在 target/release/easymd[.exe]
```

脚本会顺带处理 Windows GNU 工具链缺少 MinGW `dlltool` 的情况（`tools/dlltool-shim` 里的小替身）。

Linux 依赖（窗口 / OpenGL / 文件对话框，GTK3 用于打开对话框）：

```bash
sudo apt install build-essential pkg-config \
                 libxkbcommon-dev libgl1-mesa-dev libegl1-mesa-dev \
                 libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev \
                 libudev-dev libgtk-3-dev

# 中文与图标字体，否则中文和符号会显示成方框
sudo apt install fonts-noto-cjk fonts-noto-core fonts-symbola fonts-dejavu-core
```

想在 Windows 上直接拿到 Linux 可执行文件，用 WSL 最省事 —— 脚本会把源码复制到 Linux 侧再编译，避免 `/mnt` 的 9p 性能损失：

```bash
# 在 Windows 的 Git Bash 里执行；路径换成你的 EasyMD 目录在 WSL 下的挂载路径
# （例如 C:\... 对应 /mnt/c/...，F:\... 对应 /mnt/f/...）
MSYS_NO_PATHCONV=1 wsl -d Ubuntu-24.04 -- bash \
  /mnt/c/path/to/EasyMD/tools/wsl-build.sh --test
```

等价的容器方式：

```bash
docker run --rm -v "$PWD":/w -w /w rust:1-bookworm bash -c "
  apt-get update && apt-get install -y --no-install-recommends \
    libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev \
    libxrandr-dev libudev-dev libgtk-3-dev libgl1-mesa-dev pkg-config &&
  cargo build --release"
```

Windows 上构建出的 `easymd.exe` 是原生程序（`x86_64-pc-windows-gnu`），不能直接在 Linux 上运行 —— 请在目标平台重新编译。平台相关代码都在 `cfg` 分支里（字体回退、外部链接、配置目录、文件管理器）。

## 测试

```bash
./build.sh test     # 或：cargo test --release
```

覆盖全文检索、目录树（过滤 / 展开 / 自然排序 / 文件计数）、文档解析（锚点注入、链接判定、行号换算）、文件名校验、相对路径与 `%20` 解码、文本统计、块范围识别、两栏联动坐标映射等。

## 性能

Windows 11、Release 构建、1200×860 窗口，打开一篇 1100 字中文文档的实测值：

| 指标 | 实测值 |
| --- | --- |
| 可执行文件 | **9.0 MB**（单文件，无附带的 DLL / 运行时依赖） |
| 启动到窗口出现 | 约 0.7 秒 |
| **私有工作集**（任务管理器「内存」列） | **约 39 MB**（空白 egui 窗口基线就有 29 MB） |
| 空闲 CPU | 0%（无输入时不重绘） |

对照参考：同功能的 Electron / WebView 方案通常是 250–500 MB 内存、80–150 MB 安装体积。

## 设计取舍（要点）

- **用 egui（立即模式 GUI）而不是 Web 技术栈**：没有 JS 桥接与 DOM，冷启动快、内存可控、单文件分发且天然跨平台；代价是排版精细度不如浏览器，因此对行距 / 栏宽 / 字号做了额外调整。
- **复用 `egui_commonmark` + `pulldown-cmark`** 而不是自己写解析器：稳定性优先，避开嵌套列表、表格、脚注等边界情况。同时关闭其可选的 `syntect` 高亮，改用 `egui_extras` 内置的轻量高亮器，代码块配色还能跟随主题。
- **标题锚点**：在渲染用的文本副本上给普通标题补 GitHub 风格锚点，原文与编辑内容完全不受影响。
- **相对路径图片**：注册独立的 `easymd://` 资源协议，既支持相对路径，也保证各文档的图片缓存互不冲突。
- **字体**：系统字体枚举 + 内存映射加载（不复制、几乎不占私有内存），并把各字体的纵向度量统一，让中文、正文与等宽在同一基线上。
- **`vendor/egui_commonmark`**：带最小补丁的本地副本（表格列宽按内容分配、渲染锚点、文字块点击上报）；补丁说明与升级上游时的做法见该目录的 `PATCHES.md`。

## 已知限制

- 数学公式（`$...$`）按普通文本显示；内联 HTML 不解析
- 一次只打开一篇文档（通过浏览历史与最近文件切换）
- 删除为永久删除（有二次确认），未接入系统回收站
- 未做文件编码转换 UI：自动识别 UTF-8 / UTF-8 BOM / UTF-16 / GBK，保存时按原编码写回

## 目录结构

```
src/
  main.rs        程序入口、命令行参数、窗口图标
  app.rs         应用状态与界面编排、快捷键、操作分发
  explorer.rs    左侧面板（目录树 / 大纲 / 全文检索）
  reader.rs      阅读面板（渲染、链接、滚动记忆、背景）
  editor.rs      编辑面板（格式工具栏、Tab 缩进、光标定位）
  settings.rs    设置面板
  dialogs.rs     对话框（新建 / 删除确认 / 消息 / 关于 / 快捷键）
  ui_ext.rs      通用控件（提示条、分段控件、分隔条、高亮文本等）
  theme.rs       主题与配色、背景图片绘制
  fonts.rs       系统字体枚举、加载与行距调整
  loader.rs      自定义 easymd:// 资源加载器
  tree.rs        目录树模型（懒加载 / 排序 / 过滤 / 扁平化）
  doc.rs         文档模型（大纲、统计、编码、滚动位置）
  fs_ops.rs      文件操作（含校验）与编码读写
  search.rs      全文检索（后台线程）
  watch.rs       文件变更监听（防抖）
  config.rs      配置持久化
  util.rs        路径 / 时间 / 文本统计等工具
examples/        示例文档仓库
vendor/          egui_commonmark 本地补丁副本（见其 PATCHES.md）
tools/dlltool-shim/  Windows GNU 工具链缺 MinGW 时的 dlltool 替身
docs/screenshots/    界面截图
```

## 许可

**GNU 通用公共许可证 v3.0（GPL-3.0-only）** · Copyright (C) 2026 mc <whoismc@163.com>

你可以自由使用、修改和分发本程序；但**基于它修改后的作品必须以同一许可开源**，并保留版权声明与作者署名。

- 完整条款：[LICENSE](LICENSE)
- 附加署名要求（GPLv3 第 7(b) 条）与第三方组件许可：[NOTICE](NOTICE)

> 说明：本仓库先前的快照（以及 GitHub Release v0.1.0 中的二进制）按 MIT 发布——已发出的授权对已获得副本不可撤回；GPL-3.0 适用于本仓库当前及以后的版本。
