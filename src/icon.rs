// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 mc <whoismc@163.com>
// This file is part of EasyMD <https://github.com/Iboyiboy/EasyMD>;
// see LICENSE and NOTICE for the full terms.

// 程序图标：一份绘制代码同时供「窗口图标」（运行时，见 `main.rs`）和
// 「exe 文件图标」（构建时，见 `build.rs`）使用，保证两者永远一致。
//
// 这里刻意不依赖 egui：`build.rs` 用 `include!` 直接引入本文件，
// 因此只允许使用 `core`/`std`。

/// 4 倍超采样：先按放大尺寸逐点判定，再降采样平均，得到平滑的边缘。
const SS: usize = 4;

/// 按 `size × size` 生成 RGBA8 像素（行优先，未预乘）。
pub fn draw(size: usize) -> Vec<u8> {
    let big = size * SS;
    let mut out = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (x * SS + sx) as f32 / big as f32;
                    let fy = (y * SS + sy) as f32 / big as f32;
                    let [r, g, b, a] = sample(fx, fy);
                    let af = a as f32 / 255.0;
                    // 预乘后再平均，避免透明像素把边缘染黑
                    acc[0] += r as f32 * af;
                    acc[1] += g as f32 * af;
                    acc[2] += b as f32 * af;
                    acc[3] += a as f32;
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            let i = (y * size + x) * 4;
            if a > 0.5 {
                out[i] = (acc[0] / (acc[3] / 255.0)).clamp(0.0, 255.0) as u8;
                out[i + 1] = (acc[1] / (acc[3] / 255.0)).clamp(0.0, 255.0) as u8;
                out[i + 2] = (acc[2] / (acc[3] / 255.0)).clamp(0.0, 255.0) as u8;
            }
            out[i + 3] = a.clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 圆角底板 + 摊开的书本（左页 / 书脊 / 右页 + 页面横线）。
/// 坐标都是 0..1 的归一化值，因此任意尺寸下形状一致。
fn sample(fx: f32, fy: f32) -> [u8; 4] {
    const BADGE: [u8; 4] = [0xA8, 0x66, 0x3A, 255]; // 棕色底板
    const PAGE: [u8; 4] = [0xF7, 0xF3, 0xE8, 255]; // 米白书页
    const SPINE: [u8; 4] = [0x6B, 0x4F, 0x2E, 255]; // 书脊
    const LINE: [u8; 4] = [0xA8, 0x8A, 0x6A, 255]; // 页上横线

    if !in_round_rect(fx, fy, 0.02, 0.98, 0.02, 0.98, 0.20) {
        return [0, 0, 0, 0];
    }

    // 书脊（中缝）
    if (0.5 - fx).abs() <= 0.022 && (0.235..0.775).contains(&fy) {
        return SPINE;
    }

    // 左右两页
    let (left_page, right_page) = (
        in_round_rect(fx, fy, 0.155, 0.478, 0.235, 0.775, 0.035),
        in_round_rect(fx, fy, 0.522, 0.845, 0.235, 0.775, 0.035),
    );
    if left_page || right_page {
        // 页面上的文字行：左右内缩、不贴页边（贴边看起来像窗格而不是书页）
        let (x0, x1) = if left_page {
            (0.205, 0.43)
        } else {
            (0.57, 0.795)
        };
        if x0 <= fx && fx <= x1 {
            for y in [0.365f32, 0.455, 0.545] {
                if (fy - y).abs() <= 0.017 {
                    return LINE;
                }
            }
        }
        return PAGE;
    }

    BADGE
}

/// 点是否落在圆角矩形内（全部用归一化坐标）。
fn in_round_rect(fx: f32, fy: f32, x0: f32, x1: f32, y0: f32, y1: f32, r: f32) -> bool {
    if fx < x0 || fx > x1 || fy < y0 || fy > y1 {
        return false;
    }
    let r = r.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5);
    // 四角按圆弧判定
    let cx = if fx < x0 + r {
        x0 + r
    } else if fx > x1 - r {
        x1 - r
    } else {
        return true;
    };
    let cy = if fy < y0 + r {
        y0 + r
    } else if fy > y1 - r {
        y1 - r
    } else {
        return true;
    };
    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
    d <= r
}
