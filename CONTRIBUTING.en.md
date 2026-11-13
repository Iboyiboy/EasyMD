# Contributing

**English** | [简体中文](CONTRIBUTING.md)

Thanks for your interest in EasyMD! Here is the short path.

## Talk first, then code

- Bug fixes and small changes: a pull request or an issue is fine.
- New features, UI changes, dependency swaps, refactors: **open an issue first** describing the
  motivation and the approach, so nobody wastes effort.

## Development environment

- Rust 1.82 or newer (<https://rustup.rs>)
- Build and test:

```bash
build.cmd            # Windows (CMD / PowerShell)
./build.sh           # Linux / macOS / Git Bash
./build.sh test      # same as cargo test --release
```

Linux additionally needs GTK3 / OpenGL / fonts — see
[Building from source](README.en.md#building-from-source) in the README.

## Before you submit

1. `cargo test --release` passes.
2. Match the style of the surrounding code (the project roughly follows default rustfmt) and do
   not reformat unrelated code.
3. Do not commit build output such as `target/`, `dist/` or `*.log` (already in `.gitignore`).
4. If you touch `vendor/egui_commonmark` (a locally patched copy), update its `PATCHES.md` too.

## Commit message style

The repository uses a `Prefix: description` form, with the first line saying *what* and the body
saying *why*:

```
Fixed: 编辑模式拖拽选择越过可视区域时不滚动，长文档无法一次选完

egui 的 TextEdit 只在光标被键盘/程序移动时才 scroll_to_rect……
```

Common prefixes: `Added:` / `Fixed:` / `Changed:`.

## Contributor License Agreement (CLA)

**Your first pull request requires signing the
[Contributor License Agreement](CLA.md).** The bot will ask on the pull request; just post the
following comment to sign:

```
I have read the CLA Document and I hereby sign the CLA
```

In short:

- **You keep the copyright in your contribution.** You grant the maintainer a perpetual,
  irrevocable license that also allows distributing your contribution under **other licenses,
  including commercial terms** — that is what makes a GPL + commercial dual-licensing model
  possible.
- It includes a patent grant (which terminates if you initiate patent litigation).
- The signature record (GitHub username, pull request number, timestamp) is public and stored in
  the `cla-signatures` branch of this repository.
- If **your employer owns the intellectual property** you create at work, contact the maintainer
  first: a separate Corporate CLA (CCLA) is required.

## Conduct

Be matter-of-fact: being harsh on code is fine, being harsh on people is not.
