@echo off
rem SPDX-License-Identifier: GPL-3.0-only
rem Copyright (C) 2026 mc <whoismc@163.com>
rem This file is part of EasyMD; see LICENSE and NOTICE for the full terms.
rem EasyMD 构建脚本（Windows）
rem
rem 说明：如果本机已安装 MSYS2 / MinGW-w64（PATH 中能找到 dlltool 与 as），
rem       则直接使用标准工具链；否则自动构建并使用 tools/dlltool-shim 里的 dlltool 替代实现。
setlocal enabledelayedexpansion
cd /d "%~dp0"

set PROFILE=%1
if "%PROFILE%"=="" set PROFILE=release

where dlltool >nul 2>nul && where as >nul 2>nul
if %errorlevel%==0 (
    echo [EasyMD] 检测到 MinGW-w64 dlltool，使用标准工具链构建...
    cargo build --profile %PROFILE%
    goto :done
)

echo [EasyMD] 未检测到 MinGW-w64 binutils，使用内置 dlltool 替代方案...
echo [EasyMD] 1/2 构建 dlltool 替代程序...
cargo build --release --manifest-path tools\dlltool-shim\Cargo.toml || goto :fail

set SHIM=%CD%\tools\dlltool-shim\target\release\dlltool.exe
if not exist "%SHIM%" goto :fail

echo [EasyMD] 2/2 构建 EasyMD...
set RUSTFLAGS=-C linker=rust-lld -C dlltool=%SHIM%
cargo build --profile %PROFILE% || goto :fail

:done
echo.
echo [EasyMD] 构建完成：target\%PROFILE%\easymd.exe
exit /b 0

:fail
echo.
echo [EasyMD] 构建失败。可尝试安装 MSYS2 的 mingw-w64-x86_64-binutils，或改用 MSVC 工具链。
exit /b 1
