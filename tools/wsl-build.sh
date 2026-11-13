#!/usr/bin/env bash
# 在 WSL 里构建 Linux 版 EasyMD。
#
# 用法（在 Windows 的 Git Bash / CMD 里）：
#   wsl -d Ubuntu-24.04 -- bash /mnt/<盘符>/<路径>/EasyMD/tools/wsl-build.sh
#   wsl -d Ubuntu-24.04 -- bash .../tools/wsl-build.sh --test     # 额外跑一遍测试
#
# 源码目录默认按脚本自身位置推导（脚本在 <EasyMD>/tools/ 下），
# 需要时可用环境变量覆盖：EASYMD_SRC=/mnt/d/notes/EasyMD bash tools/wsl-build.sh
#
# 首次使用前需要（在 WSL 里执行一次）：
#   sudo apt install -y build-essential pkg-config libxkbcommon-dev libwayland-dev \
#        libx11-dev libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev \
#        libegl1-mesa-dev libudev-dev libgtk-3-dev fonts-noto-cjk
#   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
#
# 为什么要复制到 Linux 侧再编译：/mnt/* 走 9p 协议，编译期大量小文件 I/O 会慢好几倍。
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "没找到 cargo，先装 rustup（见本脚本顶部注释）"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="${EASYMD_SRC:-$(cd "$SCRIPT_DIR/.." && pwd)}"
DST="${EASYMD_BUILD:-$HOME/easymd}"
RUN_TESTS=0
[ "${1:-}" = "--test" ] && RUN_TESTS=1

echo "[wsl-build] 源码: $SRC"
echo "[wsl-build] 构建目录: $DST"
rm -rf "$DST"
mkdir -p "$DST"
for item in Cargo.toml Cargo.lock build.rs src vendor examples; do
    cp -r "$SRC/$item" "$DST/"
done

cd "$DST"
echo "[wsl-build] 开始构建…"
cargo build --release

if [ "$RUN_TESTS" = "1" ]; then
    echo "[wsl-build] 运行测试…"
    cargo test --release 2>&1 | tail -5
fi

BIN="$DST/target/release/easymd"
echo "[wsl-build] 完成: $BIN"
file "$BIN"
ls -lh "$BIN"

# 把产物拷回 Windows 侧，方便直接拿到 Linux 机器上用（$SRC 在 /mnt/... 下时才可行）
if [ -d "$SRC" ] && [ -w "$SRC" ]; then
    mkdir -p "$SRC/dist"
    OUT="$SRC/dist/easymd-linux-x86_64"
    cp "$BIN" "$OUT"
    echo "[wsl-build] 已拷贝到: $OUT"
fi

echo "[wsl-build] 运行时需要的共享库（缺失的话装这些包）："
ldd "$BIN" | awk '/=>/ {print "  " $1}' | sort | head -20
