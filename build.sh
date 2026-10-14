#!/usr/bin/env bash
# EasyMD 构建脚本（Linux / macOS / Windows 上的 Git Bash）
#
# Windows + GNU 工具链且未安装 MinGW-w64 时，会自动构建并使用
# tools/dlltool-shim 中的 dlltool 替代实现（依赖工具链自带 lld-link）。
set -euo pipefail
cd "$(dirname "$0")"

PROFILE="${1:-release}"
CARGO_ARGS=(build --profile "$PROFILE")

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    if command -v dlltool >/dev/null 2>&1 && command -v as >/dev/null 2>&1; then
      echo "[EasyMD] 检测到 MinGW-w64 binutils，使用标准工具链构建..."
      cargo "${CARGO_ARGS[@]}"
    else
      echo "[EasyMD] 未检测到 MinGW-w64 binutils，使用内置 dlltool 替代方案..."
      echo "[EasyMD] 1/2 构建 dlltool 替代程序..."
      cargo build --release --manifest-path tools/dlltool-shim/Cargo.toml
      SHIM="$(pwd)/tools/dlltool-shim/target/release/dlltool.exe"
      # 转成 Windows 路径形式，避免 rustc 在非 MSYS 路径下找不到
      if command -v cygpath >/dev/null 2>&1; then
        SHIM="$(cygpath -w "$SHIM")"
      fi
      echo "[EasyMD] 2/2 构建 EasyMD..."
      RUSTFLAGS="-C linker=rust-lld -C dlltool=$SHIM" cargo "${CARGO_ARGS[@]}"
    fi
    ;;
  *)
    cargo "${CARGO_ARGS[@]}"
    ;;
esac

echo
echo "[EasyMD] 构建完成：target/$PROFILE/easymd"
