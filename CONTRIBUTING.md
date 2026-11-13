# 贡献指南

[English](CONTRIBUTING.en.md) | **简体中文**

感谢你愿意为 EasyMD 出力！下面是最短路径。

## 先聊再写

- Bug 修复、小改动：直接提 PR 或 issue 都行。
- 新功能、界面改动、依赖替换、重构：**先开 issue 说明动机和方案**，避免白做工。

## 开发环境

- Rust 1.82 或更高版本（<https://rustup.rs>）
- 构建与测试：

```bash
build.cmd            # Windows（CMD / PowerShell）
./build.sh           # Linux / macOS / Git Bash
./build.sh test      # 等价于 cargo test --release
```

Linux 还需要 GTK3 / OpenGL / 字体等依赖，见 [README 的「从源码构建」](README.md#从源码构建)。

## 提交前

1. `cargo test --release` 全绿。
2. 风格贴近邻近代码（仓库大体遵循 rustfmt 默认），不要顺手大改无关代码。
3. 不要提交 `target/`、`dist/`、`*.log` 等产物（已在 `.gitignore` 里）。
4. 若改动了 `vendor/egui_commonmark` 这个本地补丁副本，请同步更新它的 `PATCHES.md`。

## 提交信息风格

仓库用「前缀：中文描述」的形式，首行说清做了什么，正文再讲为什么：

```
Fixed: 编辑模式拖拽选择越过可视区域时不滚动，长文档无法一次选完

egui 的 TextEdit 只在光标被键盘/程序移动时才 scroll_to_rect……
```

常用前缀：`Added:` / `Fixed:` / `Changed:`。

## 签署贡献者许可协议（CLA）

**首次提交 PR 时必须签署 [贡献者许可协议](CLA.md)**（[中文译文](CLA.zh-CN.md)）。机器人会在 PR 里提示，按提示在 PR 下评论一句即可：

```
I have read the CLA Document and I hereby sign the CLA
```

简要说明：

- **你保留自己贡献的版权**，只是授予维护者永久、不可撤回的许可，允许其在**其他许可（含商业许可）**下分发你的贡献 —— 这样项目才能同时走 GPL 开源与商业双许可。
- 同时包含专利授权（你若发起专利诉讼则终止）。
- 签署记录（GitHub 用户名、PR 编号、时间）公开存放在本仓库的 `cla-signatures` 分支。
- 若你的**雇主对你在职期间的作品享有知识产权**，请先联系维护者，需要另行签署企业版 CLA（CCLA）。

## 行为

就事论事，对代码苛刻没问题，对人不行。
