//! 索引性能基准(v0.6.2 补测)。
//!
//! 回答三个实际问题:
//! 1. 索引查询相对全表扫到底快多少(不同选择率下);
//! 2. 建索引的代价(回填时间);
//! 3. 索引对写入的拖累(每多一个索引,写慢多少)。
//!
//! 用法(release 模式):
//! ```bash
//! cargo run --release --example bench_index -- 100000
//! ```

use std::path::PathBuf;
use std::time::Instant;

use nodblite::query::{Filter, FindOptions};
use nodblite::{Database, Document};
use serde_json::json;

const COLLECTION: &str = "items";

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("nodblite_bi_{}_{}.nodb", tag, std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// 批量造数据。
fn seed(db: &Database, n: usize) {
    const BATCH: usize = 10_000;
    let mut done = 0;
    while done < n {
        let take = BATCH.min(n - done);
        let mut docs: Vec<Document> = (0..take)
            .map(|i| {
                let idx = done + i;
                Document::from_json(
                    &json!({
                        "n": idx,
                        // 高选择率字段(唯一)
                        "uid": format!("u{:08}", idx),
                        // 低选择率字段(只有 10 个值)
                        "kind": idx % 10,
                        "payload": "x".repeat(120)
                    })
                    .to_string(),
                )
                .unwrap()
            })
            .collect();
        db.insert_many(COLLECTION, &mut docs).unwrap();
        done += take;
    }
}

/// 计时一次查询并返回耗时。
fn timed<F: FnOnce() -> usize>(f: F) -> (std::time::Duration, usize) {
    let t = Instant::now();
    let n = f();
    (t.elapsed(), n)
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    println!("=== nodblite 索引基准(文档数 {}) ===\n", n);

    let path = temp_nodb("bench");
    let db = Database::create(&path).unwrap();

    // ---------- 1. 载入数据 ----------
    let t = Instant::now();
    seed(&db, n);
    let seed_ms = ms(t.elapsed());
    println!("[载入] insert_many {} 条: {} ms", n, seed_ms as u64);

    // ---------- 2. 建索引代价 ----------
    let t = Instant::now();
    db.create_index(COLLECTION, "kind_1", "kind", false, false)
        .unwrap();
    let build_low = ms(t.elapsed());

    let t = Instant::now();
    db.create_index(COLLECTION, "uid_1", "uid", true, false)
        .unwrap();
    let build_high = ms(t.elapsed());

    let t = Instant::now();
    db.create_index(COLLECTION, "n_1", "n", false, false)
        .unwrap();
    let build_n = ms(t.elapsed());

    let t = Instant::now();
    db.create_index_composite(COLLECTION, "kind_n", &["kind", "n"], false, false)
        .unwrap();
    let build_comp = ms(t.elapsed());

    println!(
        "[建索引] kind(10 值): {} ms | uid(唯一): {} ms | n(范围): {} ms | 复合(kind,n): {} ms",
        build_low as u64, build_high as u64, build_n as u64, build_comp as u64
    );

    // ---------- 3. 查询对比:全表扫 vs 索引 ----------
    println!("\n--- 查询耗时对比(ms) ---");
    println!(
        "{:<38} {:>10} {:>10} {:>8}",
        "场景", "全表扫", "索引", "加速比"
    );

    let cases: Vec<(&str, serde_json::Value, usize)> = vec![
        (
            "等值/高选择率 uid == u00050000",
            json!({"uid": "u00050000"}),
            1,
        ),
        ("等值/低选择率 kind == 3", json!({"kind": 3}), n / 10),
        (
            "范围 n ∈ [1000, 2000)",
            json!({"n": {"$gte": 1000, "$lt": 2000}}),
            1000,
        ),
        (
            "复合 kind==3 且 n>=90000",
            json!({"kind": 3, "n": {"$gte": 90000}}),
            n / 100,
        ),
    ];

    for (label, cond, expect) in cases {
        let filter = Filter::from_value(&cond).unwrap();
        let opts = FindOptions::default();

        // 全表扫:强制走 find_with_options
        let (d_full, n_full) = timed(|| {
            db.find_with_options(COLLECTION, &filter, &opts)
                .unwrap()
                .len()
        });

        // 索引路径
        let (d_idx, n_idx) = timed(|| {
            db.find_planned(COLLECTION, &filter, &cond, &opts)
                .unwrap()
                .len()
        });

        // 结果必须一致,否则基准无意义
        assert_eq!(n_full, n_idx, "{} 两条路径结果不一致", label);
        assert_eq!(
            n_full, expect,
            "{} 命中数期望 {} 实际 {}",
            label, expect, n_full
        );

        // 打印计划,解释为什么某些场景没加速
        if std::env::var("EXPLAIN").is_ok() {
            let plan = db.explain(COLLECTION, &cond, None).unwrap();
            eprintln!(
                "  计划 {}: {}",
                label,
                serde_json::to_string(&plan).unwrap_or_default()
            );
        }
        let speedup = ms(d_full) / ms(d_idx).max(0.001);
        println!(
            "{:<38} {:>10.2} {:>10.2} {:>7.1}x",
            label,
            ms(d_full),
            ms(d_idx),
            speedup
        );
    }

    // ---------- 4. 写入被索引拖累的程度 ----------
    println!("\n--- 索引对写入的影响(每批 5000 条 insert_many) ---");
    let base_path = temp_nodb("base");
    let base = Database::create(&base_path).unwrap();

    let batch_docs = |from: usize| -> Vec<Document> {
        (0..5000)
            .map(|i| {
                Document::from_json(
                    &json!({"n": from + i, "kind": (from + i) % 10,
                            "uid": format!("u{:08}", from + i)})
                    .to_string(),
                )
                .unwrap()
            })
            .collect()
    };

    let t = Instant::now();
    let mut d = batch_docs(0);
    base.insert_many(COLLECTION, &mut d).unwrap();
    let no_idx = ms(t.elapsed());

    // 带 3 个索引的库
    let idx_path = temp_nodb("withidx");
    let with_idx = Database::create(&idx_path).unwrap();
    with_idx
        .create_index(COLLECTION, "kind_1", "kind", false, false)
        .unwrap();
    with_idx
        .create_index(COLLECTION, "n_1", "n", false, false)
        .unwrap();
    with_idx
        .create_index(COLLECTION, "uid_1", "uid", false, false)
        .unwrap();

    let t = Instant::now();
    let mut d = batch_docs(0);
    with_idx.insert_many(COLLECTION, &mut d).unwrap();
    let three_idx = ms(t.elapsed());

    println!(
        "无索引: {:.1} ms | 3 个索引: {:.1} ms | 开销 {:.2}x",
        no_idx,
        three_idx,
        three_idx / no_idx.max(0.001)
    );

    // ---------- 5. 索引扫描的内存 ----------
    println!("\n(内存请配合 /usr/bin/time -l 观察)");

    drop(db);
    drop(base);
    drop(with_idx);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&base_path);
    let _ = std::fs::remove_file(&idx_path);
    println!("\n完成。");
}
