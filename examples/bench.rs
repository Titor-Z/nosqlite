//! nosqlite 读路径压测(M9 压测基线)。
//!
//! 目的:用真实数据量回答两个问题 ——
//! 1. 全量扫描的**耗时**随数据量如何增长(#5、#7);
//! 2. `scan` 返回 `Vec<(String, Vec<u8>)>` 会把多少数据**同时拉进内存**(#6)。
//!
//! # 数据载入方式
//!
//! 本示例只压**读路径**,因此数据用**批量单事务**直接写入 redb(沿用 nosqlite 的
//! key 布局 `集合名(0x00)doc_id` 与元数据表 `nosqlite_meta`),生成的是标准 `.nodb` 文件,
//! 之后用 `Database::open` 打开并测量。
//! 这样做的原因:逐条插入的写路径瓶颈已在 `examples/bench_write.rs` 单独量化
//! (约 4–30 ms/条,取决于文件大小),若用它准备数据则无法把规模做到 10 万/50 万。
//!
//! # 用法(release 模式):
//!
//! ```bash
//! cargo run --release --example bench                      # 默认 1万 / 10万,重复 3 次
//! cargo run --release --example bench 1000 5000            # 自定义规模
//! cargo run --release --example bench 500000 --repeat=3    # 指定重复次数
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use nosqlite::query::{Filter, SortSpec, Update};
use nosqlite::{Database, Document};

/// 默认压测规模。
const DEFAULT_SIZES: [usize; 2] = [10_000, 100_000];
/// 默认重复次数。
const DEFAULT_REPEAT: usize = 3;

/// 集合名。
const COLLECTION: &str = "items";

/// 单轮测量结果。
#[derive(Debug, Default)]
struct Round {
    /// `insert_many` 批量载入耗时(同即写路径验收项)。
    load: Duration,
    /// 全量 find 耗时(旧 API,保留对照:内存 O(命中数))。
    find_all: Duration,
    /// 全量 count 耗时(流式,内存 O(1))。
    count_all: Duration,
    /// `find_page` 排序+深分页耗时(两遍法,内存 O(n × 排序键大小))。
    page: Duration,
    /// 按 `_id` 更新单条耗时(主键直接寻址)。
    update_by_id: Duration,
    /// 按 `_id` 删除单条耗时(主键直接寻址)。
    delete_by_id: Duration,
    /// find 前后的 RSS 增量(KB)。
    rss_delta_kb: i64,
    /// find_page 前后的 RSS 增量(KB)。
    rss_page_delta_kb: i64,
}

/// 主函数。
fn main() {
    let (sizes, repeat) = parse_args();

    println!("nosqlite 读路径压测(M9 压测基线)");
    println!(
        "规模 {:?},每档重复 {} 次取中位数;数据以批量单事务载入(见文件头说明)",
        sizes, repeat
    );
    println!();

    for size in sizes {
        run_one(size, repeat);
    }
}

/// 解析命令行参数:位置参数为规模,`--repeat=N` 指定重复次数。
fn parse_args() -> (Vec<usize>, usize) {
    let mut sizes = Vec::new();
    let mut repeat = DEFAULT_REPEAT;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--repeat=") {
            repeat = v.parse().expect("--repeat 必须是正整数");
        } else {
            sizes.push(arg.parse().expect("规模必须是正整数"));
        }
    }
    if sizes.is_empty() {
        sizes = DEFAULT_SIZES.to_vec();
    }
    (sizes, repeat)
}

/// 跑一档规模并打印报告。
fn run_one(n: usize, repeat: usize) {
    let docs: Vec<(String, Vec<u8>)> = (0..n)
        .map(|i| (format!("doc{i:07}"), doc_json(i).into_bytes()))
        .collect();
    let payload_bytes: usize = docs.iter().map(|(_, b)| b.len()).sum();

    let rounds: Vec<Round> = (0..repeat).map(|_| run_round(&docs)).collect();

    print_report(n, payload_bytes, &rounds);
}

/// 跑一轮完整测量。
fn run_round(docs: &[(String, Vec<u8>)]) -> Round {
    let n = docs.len();
    let path = temp_nodb(n);

    // ---- 批量载入(insert_many 单事务,写路径验收项) ----
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut docs_parsed: Vec<Document> = docs
        .iter()
        .map(|(_, b)| Document::from_bytes(b).expect("文档应合法"))
        .collect();
    let start = Instant::now();
    let ids = db
        .insert_many(COLLECTION, &mut docs_parsed)
        .expect("批量插入应成功");
    let load = start.elapsed();
    assert_eq!(ids.len(), n, "批量插入条数应正确");
    drop(docs_parsed);

    // ---- 用 nosqlite 的公开 API 打开并测量读路径 ----
    let null = Filter::from_value(&serde_json::Value::Null).unwrap();

    // 全量 find(旧 API 对照:内存 O(命中数) = O(n))
    let rss_before = current_rss_kb().unwrap_or(0);
    let start = Instant::now();
    let all = db.find(COLLECTION, &null).expect("全量查询应成功");
    let find_all = start.elapsed();
    let rss_after = current_rss_kb().unwrap_or(0);
    assert_eq!(all.len(), n, "全量查询条数应正确");
    drop(all);

    // ---- 全量 count(流式,内存 O(1)) ----
    let start = Instant::now();
    let counted = db.count(COLLECTION, &null).expect("count 应成功");
    let count_all = start.elapsed();
    assert_eq!(counted, n);

    // ---- find_page:排序 + 深分页(两遍法,内存 O(n × 排序键大小)) ----
    let sort = SortSpec::from_value(&serde_json::json!({"age": -1})).expect("排序应合法");
    let rss_page_before = current_rss_kb().unwrap_or(0);
    let start = Instant::now();
    let page = db
        .find_page(COLLECTION, &null, Some(&sort), n / 2, Some(20))
        .expect("分页查询应成功");
    let page_time = start.elapsed();
    let rss_page_after = current_rss_kb().unwrap_or(0);
    assert_eq!(page.len(), 20, "分页应返回 20 条");

    // ---- 主键直接寻址:update_by_id / delete_by_id ----
    let start = Instant::now();
    let changed = db
        .update_by_id(
            COLLECTION,
            "doc0000000",
            &Update::from_value(&serde_json::json!({"$set": {"flag": true}})).expect("更新应合法"),
        )
        .expect("更新应成功");
    let update_by_id = start.elapsed();
    assert!(changed, "按主键更新应生效");

    let start = Instant::now();
    let removed = db
        .delete_by_id(COLLECTION, "doc0000000")
        .expect("删除应成功");
    let delete_by_id = start.elapsed();
    assert!(removed, "按主键删除应生效");

    db.close().expect("关闭应成功");
    drop(db);
    let _ = std::fs::remove_file(&path);

    Round {
        load,
        find_all,
        count_all,
        page: page_time,
        update_by_id,
        delete_by_id,
        rss_delta_kb: rss_after as i64 - rss_before as i64,
        rss_page_delta_kb: rss_page_after as i64 - rss_page_before as i64,
    }
}

/// 打印一档规模的报告(各耗时取中位数)。
fn print_report(n: usize, payload_bytes: usize, rounds: &[Round]) {
    let load = median_dur(rounds.iter().map(|r| r.load));
    let find_all = median_dur(rounds.iter().map(|r| r.find_all));
    let count_all = median_dur(rounds.iter().map(|r| r.count_all));
    let page = median_dur(rounds.iter().map(|r| r.page));
    let update_by_id = median_dur(rounds.iter().map(|r| r.update_by_id));
    let delete_by_id = median_dur(rounds.iter().map(|r| r.delete_by_id));

    // RSS 是进程级指标:首轮最干净(分配器尚未复用前面的峰值),后续轮次会失真,
    // 故同时给出首轮观测值与各轮最大值,只作量级参考。
    let rss_first = rounds.first().map(|r| r.rss_delta_kb).unwrap_or(0);
    let rss_max = rounds.iter().map(|r| r.rss_delta_kb).max().unwrap_or(0);
    let rss_page_first = rounds.first().map(|r| r.rss_page_delta_kb).unwrap_or(0);
    let rss_page_max = rounds
        .iter()
        .map(|r| r.rss_page_delta_kb)
        .max()
        .unwrap_or(0);

    println!("===== 规模 {} 条(中位数,{} 轮)=====", n, rounds.len());
    println!(
        "  scan 物化            : {:.2} MiB(全部文档 JSON 字节合计)",
        mib(payload_bytes as u64)
    );
    println!(
        "  insert_many          : {}  [{:.0} 条/秒]  ← 写路径验收项",
        fmt(load),
        n as f64 / load.as_secs_f64()
    );
    println!(
        "  find 全量(旧 API)  : {}  [{:.2} 万条/秒, {:.2} µs/条]",
        fmt(find_all),
        per_sec(n, find_all),
        find_all.as_secs_f64() * 1e6 / n as f64
    );
    println!(
        "    └ RSS 增量        : 首轮 {:.1} MiB / 最大 {:.1} MiB  (≈ 物化的 {:.1}x / {:.1}x)",
        mib(rss_first.max(0) as u64 * 1024),
        mib(rss_max.max(0) as u64 * 1024),
        ratio(rss_first, payload_bytes),
        ratio(rss_max, payload_bytes)
    );
    println!("  count 全量(流式)   : {}  [内存 O(1)]", fmt(count_all));
    println!("  find_page 排序+分页 : {}  ← 分页验收项", fmt(page));
    println!(
        "    └ RSS 增量        : 首轮 {:.1} MiB / 最大 {:.1} MiB  (两遍法)",
        mib(rss_page_first.max(0) as u64 * 1024),
        mib(rss_page_max.max(0) as u64 * 1024)
    );
    println!(
        "  update_by_id        : {}  ← 主键寻址验收项",
        fmt(update_by_id)
    );
    println!(
        "  delete_by_id        : {}  ← 主键寻址验收项",
        fmt(delete_by_id)
    );
    println!();
}

/// RSS 增量相对文档总字节的倍数。
fn ratio(rss_kb: i64, payload_bytes: usize) -> f64 {
    if payload_bytes == 0 {
        return 0.0;
    }
    (rss_kb.max(0) as f64 * 1024.0) / payload_bytes as f64
}

/// 生成第 `i` 条文档的 JSON(与 `examples/bench_write.rs` 一致)。
fn doc_json(i: usize) -> String {
    serde_json::json!({
        "_id": format!("doc{i:07}"),
        "title": format!("内容标题 {i}"),
        "author": format!("作者{}", i % 97),
        "age": (i % 120) as u64,
        "email": format!("user{}@example.com", i % 1000),
        "tags": ["alpha", "beta"],
        "addr": { "city": "北京", "zip": "100000" },
        "active": i.is_multiple_of(2),
    })
    .to_string()
}

/// 取一组耗时的中位数(偶数个时取中间两个的均值)。
fn median_dur(it: impl Iterator<Item = Duration>) -> Duration {
    let mut v: Vec<Duration> = it.collect();
    v.sort();
    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2
    }
}

/// 生成唯一的临时 `.nodb` 路径。
fn temp_nodb(n: usize) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "nosqlite_bench_{}_{}_{}.nodb",
        std::process::id(),
        n,
        seq
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 字节转 MiB。
fn mib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

/// 吞吐(万条/秒)。
fn per_sec(n: usize, d: Duration) -> f64 {
    let secs = d.as_secs_f64();
    if secs <= 0.0 {
        return f64::INFINITY;
    }
    (n as f64 / secs) / 10_000.0
}

/// 格式化耗时。
fn fmt(d: Duration) -> String {
    if d.as_secs_f64() >= 1.0 {
        format!("{:.3} s", d.as_secs_f64())
    } else {
        format!("{:.2} ms", d.as_secs_f64() * 1000.0)
    }
}

/// 读取进程常驻内存(RSS,单位 KB);平台不支持时返回 `None`。
#[cfg(target_os = "linux")]
fn current_rss_kb() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = content.split_whitespace().nth(1)?.parse().ok()?;
    // 页大小按 4 KiB 估算
    Some(pages * 4)
}

/// 读取进程常驻内存(RSS,单位 KB);平台不支持时返回 `None`。
#[cfg(target_os = "macos")]
fn current_rss_kb() -> Option<u64> {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// 读取进程常驻内存(RSS,单位 KB);平台不支持时返回 `None`。
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn current_rss_kb() -> Option<u64> {
    None
}
