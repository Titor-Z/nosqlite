//! 写路径对照基准(M9 压测基线的归因补测)。
//!
//! 目的:把「fsync 的物理成本」与「nosqlite/redb 自身开销」分离开,回答
//! 「写入慢到底是引擎的锅,还是每次 commit 都 fsync 的物理必然」。
//!
//! 四组对照(同一台机器、同一批文档、同一 key 布局):
//!
//! - **A. nosqlite 现状**:`Database::insert` 逐条插入(= 每条一个写事务,`Durability::Immediate`)
//! - **B. redb 批量单事务(Immediate)**:N 条插完只 commit 一次,即摊薄 fsync
//! - **C. redb 批量单事务(None + 末次落盘)**:批量期间不 fsync,末尾补一次 `Immediate` 提交
//! - **D. fsync 基准**:对同一文件反复「追加少量字节 + `sync_all()`」,得到本机单次 fsync 成本
//!
//! 判读方式:若 **A 的单条耗时 ≈ D 的单次 fsync 成本**,则瓶颈就是 fsync 的物理成本,
//! 而非 nosqlite/redb 的实现缺陷;B/C 相对 A 的倍数即「批量写」能拿到的收益。
//!
//! 用法(release 模式):
//!
//! ```bash
//! cargo run --release --example bench_write             # 默认 1000 条 / 重复 3 次
//! cargo run --release --example bench_write 2000 5      # 自定义条数 / 重复次数
//! ```

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nosqlite::{Database, Document};
use redb::{Database as RedbDatabase, Durability, TableDefinition};

/// 文档表定义(与 `src/engine/redb_engine.rs` 保持一致)。
const DOCS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("nosqlite_docs");
/// 集合名与 doc_id 的分隔符(与引擎保持一致)。
const KEY_SEPARATOR: u8 = 0x00;
/// 集合名(四组对照统一使用)。
const COLLECTION: &str = "items";

/// 一组测量的统计结果。
#[derive(Debug)]
struct Stats {
    /// 中位数耗时。
    median: Duration,
    /// 最小值。
    min: Duration,
    /// 最大值。
    max: Duration,
}

/// 主函数:跑四组对照并打印归因结论。
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n: usize = args
        .first()
        .map(|s| s.parse().expect("条数必须是正整数"))
        .unwrap_or(1000);
    let repeat: usize = args
        .get(1)
        .map(|s| s.parse().expect("重复次数必须是正整数"))
        .unwrap_or(3);

    println!("nosqlite 写路径对照基准(M9)");
    println!(
        "条数 {} 条,每组重复 {} 次取中位数;文档结构与值完全一致",
        n, repeat
    );
    println!();

    // 预生成文档文本,保证四组写入的是同一批数据
    let docs: Vec<(String, Vec<u8>)> = (0..n)
        .map(|i| {
            let id = format!("doc{i:07}");
            let json = doc_json(i);
            (id, json.into_bytes())
        })
        .collect();

    // 校验文档内嵌的 `_id` 与 A 组使用的 key 一致(否则四组的 key 对不上)
    for probe in [0, n.saturating_sub(1)] {
        let doc = Document::from_bytes(&docs[probe].1).expect("文档应合法");
        assert_eq!(
            doc.get_id().expect("应能取到主键").as_deref(),
            Some(docs[probe].0.as_str()),
            "文档内嵌 _id 应与 key 一致"
        );
    }

    let a = measure(repeat, || group_nosqlite_insert(&docs));
    print_group("A nosqlite 逐条插入(每条一个写事务/Immediate)", &a, n);

    let b = measure(repeat, || group_redb_batch_immediate(&docs));
    print_group("B redb 批量单事务(Immediate)", &b, n);

    let c = measure(repeat, || group_redb_batch_none(&docs));
    print_group("C redb 批量单事务(None + 末尾落盘)", &c, n);

    // F 组:nosqlite 的公开批量 API(内部会重新序列化)
    let f = measure(repeat, || group_nosqlite_insert_many(&docs));
    print_group("F nosqlite insert_many(已解析文档)", &f, n);

    // E 组:归因用 —— JSON 序列化开销(Document::to_bytes)
    let e = measure(repeat, || group_serialize_baseline(&docs));
    print_group_serialize(&e, n);

    // D 组:Rust `File::sync_all()`(macOS/iOS 上映射到 F_FULLFSYNC)
    let d = measure(repeat, || group_std_sync_all(n));
    print_group_sync(&d, n, "D Rust File::sync_all()");

    // G 组:POSIX `fsync()`(绕过 Rust std 的平台映射)
    #[cfg(unix)]
    let g = measure(repeat, || group_posix_fsync(n));
    #[cfg(not(unix))]
    let g = Stats {
        median: Duration::ZERO,
        min: Duration::ZERO,
        max: Duration::ZERO,
    };
    #[cfg(unix)]
    print_group_sync(&g, n, "G POSIX fsync()");
    #[cfg(not(unix))]
    println!("{:<44} 本平台不支持", "G POSIX fsync()");

    print_conclusion(n, &a, &b, &c, &d);
    print_insert_many_note(n, &b, &e, &f);
    print_platform_note(n, &d, &g);
}

/// F 组:nosqlite 的公开批量 API `Database::insert_many`。
///
/// 与 B 组的差异:本组走完整门面层 —— 文档需先解析成 `Document`,
/// 写入前每一条都要 `ensure_id()` + `to_bytes()` 重新序列化。
fn group_nosqlite_insert_many(docs: &[(String, Vec<u8>)]) -> Duration {
    let path = temp_nodb("f");
    let db = Database::create(&path).expect("创建数据库应成功");

    // 预解析放在计时区外,只测批量写入本身
    let mut parsed: Vec<Document> = docs
        .iter()
        .map(|(_, bytes)| Document::from_bytes(bytes).expect("文档应合法"))
        .collect();

    let start = Instant::now();
    db.insert_many(COLLECTION, &mut parsed)
        .expect("批量插入应成功");
    let elapsed = start.elapsed();

    drop(db);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// A 组:nosqlite 逐条插入(现状)。
///
/// 说明:A 组在计时区内比 B/C 多付一次**内存中的 JSON 序列化**(`insert` 内部会 `to_bytes`)。
/// 该成本约 1–2 µs/条,而 fsync 在毫秒量级,差异可忽略。
fn group_nosqlite_insert(docs: &[(String, Vec<u8>)]) -> Duration {
    let path = temp_nodb("a");
    let db = Database::create(&path).expect("创建数据库应成功");

    let start = Instant::now();
    for (_id, bytes) in docs {
        // 用引擎的原始 API 逐条插入:直接把 JSON 字节写进去,避免重复序列化开销
        let mut doc = Document::from_bytes(bytes).expect("文档应合法");
        db.insert(COLLECTION, &mut doc).expect("插入应成功");
    }
    let elapsed = start.elapsed();

    drop(db);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// B 组:redb 批量单事务 + `Durability::Immediate`(默认)。
fn group_redb_batch_immediate(docs: &[(String, Vec<u8>)]) -> Duration {
    let path = temp_nodb("b");
    let db = RedbDatabase::create(&path).expect("创建 redb 应成功");

    let start = Instant::now();
    let tx = db.begin_write().expect("开启写事务应成功");
    {
        let mut table = tx.open_table(DOCS_TABLE).expect("打开表应成功");
        for (id, bytes) in docs {
            let key = doc_key(COLLECTION, id);
            table.insert(&key[..], &bytes[..]).expect("插入应成功");
        }
    }
    // 只提交一次 → 只付出一次 fsync
    tx.commit().expect("提交应成功");
    let elapsed = start.elapsed();

    drop(db);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// C 组:redb 批量单事务 + `Durability::None`,末尾补一次 `Immediate` 提交保证落盘。
fn group_redb_batch_none(docs: &[(String, Vec<u8>)]) -> Duration {
    let path = temp_nodb("c");
    let db = RedbDatabase::create(&path).expect("创建 redb 应成功");

    let start = Instant::now();

    // 批量阶段:不 fsync
    let mut tx = db.begin_write().expect("开启写事务应成功");
    tx.set_durability(Durability::None)
        .expect("设置 durability 应成功");
    {
        let mut table = tx.open_table(DOCS_TABLE).expect("打开表应成功");
        for (id, bytes) in docs {
            let key = doc_key(COLLECTION, id);
            table.insert(&key[..], &bytes[..]).expect("插入应成功");
        }
    }
    tx.commit().expect("非持久提交应成功");

    // 收尾阶段:写一个标记 key 并立即提交,确保前面的数据一并落盘
    let tx = db.begin_write().expect("开启收尾事务应成功");
    {
        let mut table = tx.open_table(DOCS_TABLE).expect("打开表应成功");
        table
            .insert(&doc_key("_bench", "flush")[..], &b"1"[..])
            .expect("写入落盘标记应成功");
    }
    tx.commit().expect("持久提交应成功");

    let elapsed = start.elapsed();

    drop(db);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// D 组:Rust `File::sync_all()` 基准 —— 对同一文件反复「追加少量字节 + 同步」。
///
/// 模拟 redb「每条记录一次提交、每次提交一次 fsync」的物理成本。
///
/// ⚠️ 平台差异:在 macOS/iOS 上,Rust std 把 `sync_all()`/`sync_data()` 映射到
/// **`F_FULLFSYNC`**(强制磁盘写缓存落盘),远慢于普通 `fsync`;
/// 在 Linux 上映射到 `fdatasync`。因此本组数值**必须标注平台**才能引用。
fn group_std_sync_all(n: usize) -> Duration {
    let path = temp_fsync();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("打开基准文件应成功");

    let payload = [b'x'; 64];
    let start = Instant::now();
    for _ in 0..n {
        file.write_all(&payload).expect("追加写入应成功");
        // sync_all = Rust std 的完整同步(平台映射不同)
        file.sync_all().expect("sync_all 应成功");
    }
    let elapsed = start.elapsed();

    drop(file);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// G 组:POSIX `fsync()` 基准 —— 绕过 Rust std 的平台映射。
///
/// 与 D 组对比即可**直接测出平台的同步放大倍数**:
/// 若 G 明显快于 D,则说明本平台(如 macOS)的 `sync_all()` 走的是
/// `F_FULLFSYNC` 而非普通 `fsync`,我们测到的写入数字被平台放大。
#[cfg(unix)]
fn group_posix_fsync(n: usize) -> Duration {
    use std::os::unix::io::AsRawFd;

    // 直接声明 POSIX fsync,避免为基准引入 libc 依赖
    unsafe extern "C" {
        fn fsync(fd: std::os::raw::c_int) -> std::os::raw::c_int;
    }

    let path = temp_fsync();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("打开基准文件应成功");
    let fd = file.as_raw_fd();

    let payload = [b'x'; 64];
    let start = Instant::now();
    for _ in 0..n {
        file.write_all(&payload).expect("追加写入应成功");
        // 直接调用 POSIX fsync(不经过 Rust std 的 macOS F_FULLFSYNC 映射)
        let rc = unsafe { fsync(fd) };
        assert_eq!(rc, 0, "POSIX fsync 应成功");
    }
    let elapsed = start.elapsed();

    drop(file);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// E 组:JSON 序列化基准 —— 把已解析的文档重新序列化回字节。
///
/// 用于归因 `Database::insert_many` 相对裸 redb 批量的额外开销:
/// `insert_many` 在写入前对每条文档做一次 `to_bytes()`,而这在裸批量对照里是不存在的。
fn group_serialize_baseline(docs: &[(String, Vec<u8>)]) -> Duration {
    // 预解析(不计入计时),只测序列化
    let parsed: Vec<Document> = docs
        .iter()
        .map(|(_, bytes)| Document::from_bytes(bytes).expect("文档应合法"))
        .collect();

    let start = Instant::now();
    let mut total = 0usize;
    for doc in &parsed {
        total += doc.to_bytes().expect("序列化应成功").len();
    }
    let elapsed = start.elapsed();
    // 防止编译器优化掉整个循环
    assert!(total > 0);

    elapsed
}

/// 重复测量:跳过第一次(预热),其余取中位数与极差。
///
/// 中位数口径:奇数个取中间值,偶数个取中间两个的均值(避免偏大)。
/// 建议 `repeat >= 3`;`repeat = 1` 时只报单次结果,仅供参考。
fn measure<F: FnMut() -> Duration>(repeat: usize, mut f: F) -> Stats {
    // 预热一次:让页面缓存/文件系统进入稳定状态,结果不计入
    let _ = f();

    let mut samples: Vec<Duration> = (0..repeat).map(|_| f()).collect();
    samples.sort();
    Stats {
        median: median(&samples),
        min: samples[0],
        max: samples[samples.len() - 1],
    }
}

/// 取中位数(偶数个时取中间两个的均值)。
fn median(sorted: &[Duration]) -> Duration {
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    }
}

/// 打印一组写入耗时(条/秒)。
fn print_group(label: &str, s: &Stats, n: usize) {
    println!(
        "{:<44} 中位 {:>9}  [{} ~ {}]  {:>10.1} 条/秒",
        label,
        fmt(s.median),
        fmt(s.min),
        fmt(s.max),
        n as f64 / s.median.as_secs_f64()
    );
}

/// 打印同步基准(次/秒)。
fn print_group_sync(s: &Stats, n: usize, label: &str) {
    println!(
        "{:<44} 中位 {:>9}  [{} ~ {}]  {:>10.1} 次/秒",
        label,
        fmt(s.median),
        fmt(s.min),
        fmt(s.max),
        n as f64 / s.median.as_secs_f64()
    );
}

/// 打印序列化基准(条/秒)。
fn print_group_serialize(s: &Stats, n: usize) {
    println!(
        "{:<44} 中位 {:>9}  [{} ~ {}]  {:>10.1} 条/秒",
        "E JSON 序列化基准(Document::to_bytes)",
        fmt(s.median),
        fmt(s.min),
        fmt(s.max),
        n as f64 / s.median.as_secs_f64()
    );
}

/// 打印 `insert_many`(F)与「裸 redb 批量(B)+ 序列化(E)」的差距归因。
fn print_insert_many_note(n: usize, b: &Stats, e: &Stats, f: &Stats) {
    let per_b = b.median.as_secs_f64() / n as f64;
    let per_e = e.median.as_secs_f64() / n as f64;
    let per_f = f.median.as_secs_f64() / n as f64;

    // 理论上限:裸批量 + 一次序列化(单位:条/秒)
    let ceiling = n as f64 / (b.median + e.median).as_secs_f64();
    let actual = n as f64 / f.median.as_secs_f64();

    println!();
    println!("===== insert_many(F)差距归因 =====");
    println!(
        "单条耗时:B {:.2} µs + E(序列化) {:.2} µs = {:.2} µs → 理论上限 {:.0} 条/秒",
        per_b * 1e6,
        per_e * 1e6,
        (per_b + per_e) * 1e6,
        ceiling
    );
    println!(
        "F 实测单条 {:.2} µs → {:.0} 条/秒(达理论上限的 {:.0}%)",
        per_f * 1e6,
        actual,
        actual / ceiling * 100.0
    );
    println!(
        "序列化占理论下限的 {:.0}%;剩余未解释开销 {:.2} µs/条",
        per_e / (per_b + per_e) * 100.0,
        (per_f - per_b - per_e) * 1e6
    );
}

/// 打印平台同步放大说明(D `sync_all` vs G POSIX `fsync`)。
#[cfg(unix)]
fn print_platform_note(n: usize, d: &Stats, g: &Stats) {
    let per_d = d.median.as_secs_f64() / n as f64;
    let per_g = g.median.as_secs_f64() / n as f64;
    let ratio = per_d / per_g;

    println!();
    println!("===== 平台同步放大(D vs G)=====");
    println!(
        "sync_all {:.2} ms vs POSIX fsync {:.2} ms → 放大 {:.1}x",
        per_d * 1000.0,
        per_g * 1000.0,
        ratio
    );
    if ratio >= 3.0 {
        println!(
            "结论:本平台的 Rust sync_all 走 F_FULLFSYNC(非普通 fsync),写入数字被放大 {:.0}x —— **本机基准必须标注平台**",
            ratio
        );
    } else {
        println!("结论:两种同步路径代价接近,无明显平台放大(典型为 Linux fdatasync)");
    }
}

/// 打印归因结论。
fn print_conclusion(n: usize, a: &Stats, b: &Stats, c: &Stats, d: &Stats) {
    let per_a = a.median.as_secs_f64() / n as f64;
    let per_d = d.median.as_secs_f64() / n as f64;
    let ratio = per_a / per_d;

    println!();
    println!("===== 归因 =====");
    println!(
        "A 单条耗时 {:.2} ms vs D 单次 fsync {:.2} ms → A/D = {:.2}x",
        per_a * 1000.0,
        per_d * 1000.0,
        ratio
    );
    if (0.7..=1.4).contains(&ratio) {
        println!(
            "结论:A 的单条耗时与一次 fsync 同量级 → 写入瓶颈就是 fsync 的物理成本,不是引擎实现缺陷"
        );
    } else if ratio > 1.4 {
        println!(
            "结论:A 的单条耗时显著高于一次 fsync({:.2}x)→ 除 fsync 外还存在额外开销,需进一步定位",
            ratio
        );
    } else {
        println!(
            "结论:A 的单条耗时显著低于一次 fsync({:.2}x)→ fsync 不是主因,瓶颈在别处",
            ratio
        );
    }

    println!(
        "批量收益:B/A = {:.1}x,C/A = {:.1}x",
        a.median.as_secs_f64() / b.median.as_secs_f64(),
        a.median.as_secs_f64() / c.median.as_secs_f64()
    );
    println!(
        "B 吞吐 {:.0} 条/秒,C 吞吐 {:.0} 条/秒(A 为 {:.1} 条/秒)",
        n as f64 / b.median.as_secs_f64(),
        n as f64 / c.median.as_secs_f64(),
        n as f64 / a.median.as_secs_f64()
    );
}

/// 生成第 `i` 条文档的 JSON(与 `examples/bench.rs` 保持一致的文档结构)。
fn doc_json(i: usize) -> String {
    serde_json::json!({
        "_id": format!("doc{i:07}"),
        "title": format!("内容标题 {i}"),
        "author": format!("作者{}", i % 97),
        "age": (i % 120) as u64,
        "email": format!("user{}@example.com", i % 1000),
        "tags": ["alpha", "beta"],
        "addr": { "city": "北京", "zip": "100000" },
        "active": i % 2 == 0,
    })
    .to_string()
}

/// 构造文档表 key:`集合名(0x00)doc_id`(与引擎保持一致)。
fn doc_key(collection: &str, doc_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(collection.len() + 1 + doc_id.len());
    key.extend_from_slice(collection.as_bytes());
    key.push(KEY_SEPARATOR);
    key.extend_from_slice(doc_id.as_bytes());
    key
}

/// 生成唯一的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    temp_path(tag, "nodb")
}

/// 生成唯一的临时 fsync 基准文件路径。
fn temp_fsync() -> PathBuf {
    temp_path("fsync", "dat")
}

/// 生成唯一临时路径。
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir();
    let name = format!(
        "nosqlite_bench_write_{}_{}_{}.{}",
        tag,
        std::process::id(),
        seq,
        ext
    );
    clean(&dir.join(&name));
    dir.join(name)
}

/// 删除可能残留的同名文件(保证每次都从零开始)。
fn clean(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 格式化耗时。
fn fmt(d: Duration) -> String {
    if d.as_secs_f64() >= 1.0 {
        format!("{:.3} s", d.as_secs_f64())
    } else {
        format!("{:.2} ms", d.as_secs_f64() * 1000.0)
    }
}
