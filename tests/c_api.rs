//! C ABI 集成测试(阶段 1)。
//!
//! 不满足于「Rust 侧能调通」,而是**真的编译一个 C 程序**、链接本库动态库、
//! 跑一遍完整生命周期 —— 这样链接符号、头文件、内存约定一旦出问题就会暴露。

use std::path::PathBuf;
use std::process::Command;

/// 生成本次测试独占的临时路径。
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_cabi_{}_{}_{}.{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        ext
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 定位编译产物目录(cdylib 与 rlib 都在 target/<profile> 下)。
///
/// 测试二进制通常在 `target/<profile>/deps/`,但 CI 上可能落在
/// `target/<profile>/build/<crate>/<hash>/` 之类的子目录里 ——
/// 只取固定两级会算错(曾在 GitHub Actions 上一直失败)。
/// 这里改为向上找第一个名为 `debug` / `release` 的祖先目录。
fn target_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("应能取得测试二进制路径");
    for ancestor in exe.ancestors().skip(1) {
        match ancestor.file_name().and_then(|n| n.to_str()) {
            Some("debug") | Some("release") => return ancestor.to_path_buf(),
            _ => {}
        }
    }
    panic!("未能从 {} 反推 target/<profile> 目录", exe.display());
}

/// 编译并运行 `tests/c_smoke.c`,断言其退出码为 0。
#[test]
fn c_abi_smoke_test() {
    // 1. 确保动态库已构建(测试编译期已构建 lib,这里只做存在性校验)
    let target = target_dir();
    let lib = if cfg!(target_os = "macos") {
        target.join("libnodblite.dylib")
    } else if cfg!(target_os = "windows") {
        target.join("nodblite.dll")
    } else {
        target.join("libnodblite.so")
    };
    // cdylib 不参与 `cargo test` 的构建(crate-type 里 rlib 优先),
    // 因此在干净环境下动态库可能还不存在 —— 这里按需构建一次,
    // 而不是依赖「碰巧之前 build 过」。CI 首次跑就是栽在这里。
    if !lib.exists() {
        let status = Command::new(env!("CARGO"))
            .args(["build", "--lib"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .expect("应能执行 cargo build --lib");
        assert!(status.success(), "构建 cdylib 失败");
    }
    assert!(lib.exists(), "动态库应存在: {}", lib.display());

    // 2. 编译 C 程序
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/c_smoke.c");
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let bin = temp_path("smoke", if cfg!(windows) { "exe" } else { "bin" });

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = Command::new(&cc);
    cmd.arg(&src)
        .arg(format!("-I{}", include.display()))
        .arg(format!("-L{}", target.display()))
        .arg("-lnodblite")
        .arg("-o")
        .arg(&bin);
    if cfg!(target_os = "macos") {
        // 让链接器在运行时到构建目录找 dylib
        cmd.arg(format!("-Wl,-rpath,{}", target.display()));
    }
    let compile = cmd.output().expect("应能调用 C 编译器");
    assert!(
        compile.status.success(),
        "C 编译失败: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    // 3. 运行
    let db = temp_path("smoke_db", "nodb");
    let run = Command::new(&bin)
        .arg(&db)
        .env(
            if cfg!(target_os = "macos") {
                "DYLD_LIBRARY_PATH"
            } else {
                "LD_LIBRARY_PATH"
            },
            &target,
        )
        .output()
        .expect("应能运行 C 程序");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "C 冒烟测试失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("ALL PASSED"), "实际输出: {}", stdout);

    let _ = std::fs::remove_file(&bin);
    let _ = std::fs::remove_file(&db);
}

/// 结构体布局必须与 C 头文件一致(`repr(C)` + 字段顺序)。
#[test]
fn struct_layouts_match_header() {
    use nodblite::ffi::*;

    // Slice = { ptr, len }
    assert_eq!(
        std::mem::size_of::<NodbliteSlice>(),
        std::mem::size_of::<usize>() * 2
    );

    // Error = { i32, slice, slice, slice } => 4 + padding(4) + 16*3 = 56
    // 用对齐推算,不硬编码具体字节数,避免平台差异
    assert!(std::mem::size_of::<NodbliteError>() >= 4 + 3 * 16);

    // OpenOptions 的 struct_size 字段在偏移 0
    assert_eq!(std::mem::offset_of!(NodbliteOpenOptions, struct_size), 0);

    // FindOptions 的 struct_size 字段在偏移 0
    assert_eq!(std::mem::offset_of!(NodbliteFindOptions, struct_size), 0);

    // Result 的前两个字段是 code/count(便于 C 侧直接读)
    assert_eq!(std::mem::offset_of!(NodbliteResult, code), 0);
    assert!(std::mem::offset_of!(NodbliteResult, count) > 0);
}

/// 头文件里的错误码宏必须与 Rust 枚举的 `as_i32()` 一致。
///
/// 直接解析 `include/nodblite.h` 的 `#define`,这样头文件与实现漂移会立刻被抓住。
#[test]
fn header_macros_match_rust() {
    use nodblite::ErrorCode;

    let header = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/nodblite.h"),
    )
    .expect("应能读取头文件");

    // 从 `#define NODBLITE_<NAME> <num>` 中取出数值
    let find = |name: &str| -> Option<i32> {
        header
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("#define {name} ")))
            .and_then(|l| l.split_whitespace().last())
            .and_then(|n| n.parse::<i32>().ok())
    };

    assert_eq!(find("NODBLITE_OK"), Some(0), "成功码必须是 0");
    let pairs = [
        ("NODBLITE_DUPLICATE_KEY", ErrorCode::DuplicateKey),
        ("NODBLITE_INVALID_PATH", ErrorCode::InvalidPath),
        ("NODBLITE_INVALID_FORMAT", ErrorCode::InvalidFormat),
        ("NODBLITE_VERSION_MISMATCH", ErrorCode::VersionMismatch),
        ("NODBLITE_INVALID_COLLECTION", ErrorCode::InvalidCollection),
        ("NODBLITE_INVALID_FILTER", ErrorCode::InvalidFilter),
        ("NODBLITE_INVALID_UPDATE", ErrorCode::InvalidUpdate),
        ("NODBLITE_INVALID_SORT", ErrorCode::InvalidSort),
        ("NODBLITE_INVALID_PROJECTION", ErrorCode::InvalidProjection),
        ("NODBLITE_INVALID_DOCUMENT", ErrorCode::InvalidDocument),
        ("NODBLITE_IO_ERROR", ErrorCode::IoError),
        ("NODBLITE_INTERNAL", ErrorCode::Internal),
    ];
    for (macro_name, code) in pairs {
        assert_eq!(
            find(macro_name),
            Some(code.as_i32()),
            "头文件宏 {} 与 Rust 侧 {} 不一致",
            macro_name,
            code.as_str()
        );
    }
}

/// 头文件里的每个 `nodblite_*` 函数,要么被 npm 的 `bindings.js` 声明,要么在允许清单里。
///
/// 为什么需要这条:**已经两次**出现「C 端加了函数、JS 端没跟」
/// (`aggregate`/`drop_collection`/`compact` 与 `update_array_filters`)。
/// 头文件宏与 Rust `ErrorCode` 早有对照测试,但**函数列表**没有,于是漂移无人发现。
#[test]
fn npm_bindings_cover_all_c_abi_functions() {
    use std::collections::BTreeSet;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header = std::fs::read_to_string(root.join("include/nodblite.h")).expect("应能读头文件");
    let bindings =
        std::fs::read_to_string(root.join("npm/src/bindings.js")).expect("应能读 npm 绑定");

    // 从头文件与 JS 里抽出所有 `nodblite_xxx` 标识符
    let names = |text: &str| -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while let Some(pos) = text[i..].find("nodblite_") {
            let start = i + pos;
            let mut end = start + "nodblite_".len();
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' {
                    end += 1;
                } else {
                    break;
                }
            }
            out.insert(text[start..end].to_string());
            i = end.max(start + 1);
        }
        out
    };

    let in_header = names(&header);
    let in_npm = names(&bindings);

    // 有意不在 npm 里暴露的函数(每一条都要写明理由,便于将来复核)
    const SKIP: &[(&str, &str)] = &[
        ("nodblite_malloc", "wasm 宿主用(在线性内存里分配字符串),Node 走 koffi 不需要"),
        ("nodblite_free", "同上,与 nodblite_malloc 配对"),
        ("nodblite_open_ex", "版本化 options;npm 用简单的 nodblite_open 已足够"),
        ("nodblite_exec", "兜底入口;npm 的每个方法都有专用函数"),
        ("nodblite_find_cursor", "分块游标;npm 用 findPage 做分页"),
        ("nodblite_bulk_begin", "流式批量会话;npm 用 insertMany 一次提交"),
        ("nodblite_bulk_add", "同上"),
        ("nodblite_bulk_commit", "同上"),
        ("nodblite_bulk_abort", "同上"),
    ];

    let missing: Vec<&String> = in_header
        .iter()
        .filter(|n| !in_npm.contains(*n))
        .filter(|n| !SKIP.iter().any(|(s, _)| s == &n.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "头文件声明了但 npm/src/bindings.js 没有绑定的函数: {:?}\n\
         如果是有意不暴露,请加入本测试的 SKIP 清单并写明理由。",
        missing
    );

    // 反向检查:SKIP 里不能有已经不存在(或已经绑定)的条目,否则清单会慢慢腐化
    for (name, why) in SKIP {
        assert!(
            in_header.contains(*name),
            "SKIP 清单里的 `{}` 已不在头文件中,请删除该条({})",
            name,
            why
        );
        assert!(
            !in_npm.contains(*name),
            "`{}` 其实已经绑定了,请从 SKIP 清单删除({})",
            name,
            why
        );
    }
}

/// wasm 宿主(`web/nodblite.js`)与 studio(`studio-ui/src/lib/nodblite.js`)同样要跟头文件。
///
/// 这两处比 npm 更容易漂:它们在 JS 里手写 `this.exports.nodblite_xxx`,
/// 没有类型系统帮忙,漏了就是运行时报 `undefined is not a function`。
#[test]
fn js_hosts_cover_all_c_abi_functions() {
    use std::collections::BTreeSet;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header = std::fs::read_to_string(root.join("include/nodblite.h")).expect("应能读头文件");

    let names = |text: &str| -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while let Some(pos) = text[i..].find("nodblite_") {
            let start = i + pos;
            let mut end = start + "nodblite_".len();
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' {
                    end += 1;
                } else {
                    break;
                }
            }
            out.insert(text[start..end].to_string());
            i = end.max(start + 1);
        }
        out
    };
    let in_header = names(&header);

    // wasm 宿主与 studio 只用到「数据 + 账号无关」的那部分:
    // 它们跑在浏览器里,本身就是从本地文件拿到的句柄,拿不到也不该有网络权限。
    const HOST_SKIP: &[(&str, &str)] = &[
        ("nodblite_malloc", "宿主通过 Node WASI 自己分配字符串,不用导出"),
        ("nodblite_free", "同上"),
        ("nodblite_open_ex", "未使用版本化 options"),
        ("nodblite_exec", "studio 用专用方法"),
        ("nodblite_bulk_begin", "studio 用 insertMany"),
        ("nodblite_bulk_add", "同上"),
        ("nodblite_bulk_commit", "同上"),
        ("nodblite_bulk_abort", "同上"),
        ("nodblite_last_error_code", "用 nodblite_last_error 取结构化错误"),
        ("nodblite_last_error", "由 readResult/lastError 内部封装,不算缺失"),
    ];

    // studio 只在应用内按页取数,不做流式遍历,因此不绑游标
    const STUDIO_SKIP: &[(&str, &str)] = &[
        ("nodblite_find_cursor", "studio 用 find 按页取"),
        ("nodblite_cursor_next", "同上"),
        ("nodblite_cursor_close", "同上"),
    ];

    for rel in ["web/nodblite.js", "studio-ui/src/lib/nodblite.js"] {
        let text = std::fs::read_to_string(root.join(rel)).expect("应能读 JS 宿主");
        let host = names(&text);
        let missing: Vec<&String> = in_header
            .iter()
            .filter(|n| !host.contains(*n))
            .filter(|n| !HOST_SKIP.iter().any(|(s, _)| s == &n.as_str()))
            .filter(|n| !(rel.starts_with("studio-ui") && STUDIO_SKIP.iter().any(|(s, _)| s == &n.as_str())))
            .collect();
        assert!(
            missing.is_empty(),
            "{} 缺少绑定的 C ABI 函数: {:?}\n\
             这些函数在浏览器里调用会直接报 undefined。若是有意不暴露,请加入 HOST_SKIP。",
            rel,
            missing
        );
    }
}

/// koffi 绑定的**参数个数**必须与头文件一致。
///
/// 名字对得上、参数少一个,是最难查的一类 bug:koffi 不校验 arity,
/// 缺的参数会读到未初始化的寄存器。真实事故:`nodblite_find` 少声明了第 8 个参数
/// (`limit_is_null`),于是一部分调用把 limit 当成 0 → 偶尔「查不到任何数据」,
/// 而且只在 C ABI 路径复现(Rust 侧循环 300 轮全对)。
#[test]
fn npm_binding_arity_matches_header() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header = std::fs::read_to_string(root.join("include/nodblite.h")).expect("应能读头文件");
    let bindings =
        std::fs::read_to_string(root.join("npm/src/bindings.js")).expect("应能读 npm 绑定");

    /// 取 `name(` 之后配对括号内的参数,返回参数个数(无参数为 0)。
    fn arity_of(text: &str, name: &str) -> Option<usize> {
        let at = text.find(&format!("{}(", name))?;
        let rest = &text[at + name.len() + 1..];
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        let params = rest[..end?].trim();
        if params.is_empty() {
            return Some(0);
        }
        // 按顶层逗号切(参数里没有嵌套括号,头文件里都是简单类型)
        Some(params.split(',').count())
    }

    // 绑定里 koffi 的签名是跨行字符串拼接,先把 "' +\n ... '" 归一成一段
    let mut flat = String::with_capacity(bindings.len());
    let mut prev_quote = false;
    for ch in bindings.chars() {
        if prev_quote && ch == '+' {
            prev_quote = false;
            continue;
        }
        if ch == '\'' {
            prev_quote = true;
            continue;
        }
        if prev_quote && ch.is_whitespace() {
            continue;
        }
        flat.push(ch);
        prev_quote = false;
    }

    let mut checked = 0;
    for name in [
        "nodblite_open",
        "nodblite_insert",
        "nodblite_insert_many",
        "nodblite_get",
        "nodblite_update",
        "nodblite_update_array_filters",
        "nodblite_upsert",
        "nodblite_delete",
        "nodblite_count",
        "nodblite_find",
        "nodblite_find_page",
        "nodblite_create_index",
        "nodblite_create_index_composite",
        "nodblite_list_indexes",
        "nodblite_drop_index",
        "nodblite_explain",
        "nodblite_drop_collection",
        "nodblite_compact",
        "nodblite_aggregate",
        "nodblite_list_collections",
    ] {
        let in_header = arity_of(&header, name)
            .unwrap_or_else(|| panic!("头文件里找不到 {}", name));
        let in_js = arity_of(&flat, name).unwrap_or_else(|| panic!("bindings.js 里找不到 {}", name));
        assert_eq!(
            in_js, in_header,
            "koffi 绑定 `{}` 的参数个数是 {},头文件是 {} —— 少传的参数会读到未初始化的寄存器",
            name, in_js, in_header
        );
        checked += 1;
    }
    assert!(checked >= 20, "应检查至少 20 个函数,实际 {}", checked);
}
