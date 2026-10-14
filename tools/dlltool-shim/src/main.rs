//! dlltool 替代实现（仅 Windows + GNU 工具链、且未安装 MinGW-w64 binutils 时需要）。
//!
//! 背景：
//!   x86_64-pc-windows-gnu 目标在遇到 `#[link(kind = "raw-dylib")]`（例如 windows-link /
//!   parking_lot_core / libloading 使用）时，rustc 会调用 `dlltool` 生成导入库。
//!   rustup 自带的 rust-mingw 只提供 dlltool + ld + gcc，没有 `as.exe`，
//!   因此 dlltool 无法工作，报 “CreateProcess” 错误。
//!
//! 方案：
//!   rustc 支持 `-C dlltool=<路径>` 指定替代工具。本程序把 dlltool 的命令行参数
//!   转译为工具链自带的 `lld-link /lib /def:... /out:...`，直接生成导入库，
//!   不需要汇编器。由于生成的是 LLVM 格式导入库，链接器需使用 `rust-lld`
//!   （见仓库根目录的 build.cmd / build.sh）。
//!
//! 用法（由 rustc 自动调用，无需手工执行）：
//!   dlltool -d <input.def> -D <dllname.dll> -l <output.lib> -m i386:x86-64 -f --64 ...

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut def: Option<String> = None;
    let mut out: Option<String> = None;
    let mut dll_name: Option<String> = None;
    let mut machine = "x64".to_string();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" | "--input-def" => {
                i += 1;
                def = args.get(i).cloned();
            }
            "-l" | "--output-lib" => {
                i += 1;
                out = args.get(i).cloned();
            }
            "-D" | "--dllname" => {
                i += 1;
                dll_name = args.get(i).cloned();
            }
            "-m" | "--machine" => {
                i += 1;
                machine = match args.get(i).map(|s| s.as_str()) {
                    Some("i386:x86-64") => "x64",
                    Some("i386") => "x86",
                    Some("arm64") => "arm64",
                    Some("arm") => "arm",
                    _ => "x64",
                }
                .to_string();
            }
            // 这些参数只与汇编器有关，转译时忽略
            "-S" | "--as" | "-f" | "--as-flags" => i += 1,
            _ => {}
        }
        i += 1;
    }

    let (Some(def), Some(out)) = (def, out) else {
        eprintln!("dlltool-shim: 缺少 -d / -l 参数");
        std::process::exit(1);
    };

    let lld = match find_lld_link() {
        Some(p) => p,
        None => {
            eprintln!("dlltool-shim: 找不到 lld-link.exe（可设置 EASYMD_LLDLINK 指定路径）");
            std::process::exit(1);
        }
    };

    // rustc 生成的 .def 只有 EXPORTS 段，DLL 名通过 -D 传给 dlltool。
    // lld-link 需要 LIBRARY 行，这里补一个临时 .def。
    let def_path = PathBuf::from(&def);
    let temp_def = match &dll_name {
        Some(dll) => {
            let body = std::fs::read_to_string(&def_path).unwrap_or_default();
            if body.contains("LIBRARY") {
                None
            } else {
                let p = def_path.with_extension("easymd-def");
                let content = format!("LIBRARY {dll}\n{body}");
                if std::fs::write(&p, content).is_ok() {
                    Some(p)
                } else {
                    None
                }
            }
        }
        None => None,
    };
    let use_def = temp_def.clone().unwrap_or(def_path);

    let result = Command::new(&lld)
        .arg("/lib")
        .arg(format!("/def:{}", use_def.display()))
        .arg(format!("/out:{out}"))
        .arg(format!("/machine:{machine}"))
        .arg("/nologo")
        .output();

    if let Some(p) = temp_def {
        let _ = std::fs::remove_file(p);
    }

    match result {
        Ok(o) => {
            if !o.status.success() {
                eprintln!(
                    "dlltool-shim: lld-link 执行失败：{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("dlltool-shim: 无法运行 lld-link：{e}");
            std::process::exit(1);
        }
    }

    if !Path::new(&out).exists() {
        eprintln!("dlltool-shim: 未生成导入库 {out}");
        std::process::exit(1);
    }
}

/// 依次在：环境变量 / 自身同级目录 / PATH / rustup 工具链目录中查找 lld-link.exe。
fn find_lld_link() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("EASYMD_LLDLINK") {
        if Path::new(&p).is_file() {
            return Some(PathBuf::from(p));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("lld-link.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let p = dir.join("lld-link.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".rustup")))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".rustup")))?;
    let toolchains = rustup_home.join("toolchains");
    for entry in std::fs::read_dir(&toolchains).ok()?.flatten() {
        let rustlib = entry.path().join("lib").join("rustlib");
        let Ok(hosts) = std::fs::read_dir(&rustlib) else {
            continue;
        };
        for host in hosts.flatten() {
            let p = host.path().join("bin").join("gcc-ld").join("lld-link.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}
