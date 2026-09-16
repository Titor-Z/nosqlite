//! 并发写入排队压测示例(M9 压测基线)。
//!
//! 回答一个关键问题:redb 是**单写事务**模型,多线程并发写入时是真正并行、
//! 还是被串行化?这决定了 nosqlite 作为 HTTP 服务存储层时的写入吞吐上限。
//!
//! 做法:同样的总写入量,分别用单线程和多线程跑一遍,比较墙上时钟;
//! 若两者接近,说明写入被串行化(排队),并发只带来锁竞争开销而非吞吐提升。
//!
//! 用法(release 模式):
//!
//! ```bash
//! cargo run --release --example bench_concurrency            # 默认 200 条 / 8 线程
//! cargo run --release --example bench_concurrency 400 8      # 自定义总量 / 线程数
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use nosqlite::{Database, Document};

/// 主函数:跑单线程与多线程两组写入并比较。
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let total: usize = args
        .first()
        .map(|s| s.parse().expect("总量必须是正整数"))
        .unwrap_or(200);
    let threads: usize = args
        .get(1)
        .map(|s| s.parse().expect("线程数必须是正整数"))
        .unwrap_or(8);

    println!("nosqlite 并发写入排队压测(M9)");
    println!(
        "总量 {} 条,线程数 {};每条插入均为独立写事务",
        total, threads
    );
    println!();

    let single = run_single(total);
    println!(
        "单线程 : {:.3} s({:.2} ms/条,{:.1} 条/秒)",
        single.as_secs_f64(),
        single.as_secs_f64() * 1000.0 / total as f64,
        total as f64 / single.as_secs_f64()
    );

    let multi = run_multi(total, threads);
    println!(
        "{} 线程: {:.3} s({:.2} ms/条,{:.1} 条/秒)",
        threads,
        multi.as_secs_f64(),
        multi.as_secs_f64() * 1000.0 / total as f64,
        total as f64 / multi.as_secs_f64()
    );

    let speedup = single.as_secs_f64() / multi.as_secs_f64();
    println!();
    println!("加速比(单线程耗时 / 多线程耗时): {:.2}x", speedup);
    if speedup < 1.3 {
        println!("结论:写入被串行化(单写事务排队),并发无法提升写入吞吐");
    } else {
        println!("结论:写入存在并行收益,但仍受单写事务约束");
    }
}

/// 单线程写入 `total` 条。
fn run_single(total: usize) -> Duration {
    let path = temp_nodb("single");
    let db = Database::create(&path).expect("创建数据库应成功");

    let start = Instant::now();
    for i in 0..total {
        let json = format!(r#"{{"_id":"s{i}","value":{i}}}"#);
        let mut doc = Document::from_json(&json).expect("文档应合法");
        db.insert("items", &mut doc).expect("插入应成功");
    }
    let elapsed = start.elapsed();

    drop(db);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// 用 `threads` 个线程并发写入 `total` 条(均分到各线程)。
fn run_multi(total: usize, threads: usize) -> Duration {
    let path = temp_nodb("multi");
    let db = Arc::new(Database::create(&path).expect("创建数据库应成功"));
    let barrier = Arc::new(Barrier::new(threads));

    let per_thread = total / threads;
    let remainder = total % threads;

    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            // 余数摊到前 remainder 个线程,保证总数精确
            let count = per_thread + if t < remainder { 1 } else { 0 };
            thread::spawn(move || {
                barrier.wait();
                for i in 0..count {
                    let json = format!(r#"{{"_id":"m{t}-{i}","value":{i}}}"#);
                    let mut doc = Document::from_json(&json).expect("文档应合法");
                    db.insert("items", &mut doc).expect("并发插入不应失败");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("线程不应 panic");
    }
    let elapsed = start.elapsed();

    assert_eq!(
        db.count(
            "items",
            &nosqlite::query::Filter::from_value(&serde_json::Value::Null).unwrap()
        )
        .expect("计数应成功"),
        total,
        "并发写入不应丢数据"
    );

    drop(db);
    let _ = std::fs::remove_file(&path);
    elapsed
}

/// 临时文件序号(保证同一进程内多次调用不重名)。
static FILE_SEQ: AtomicU32 = AtomicU32::new(0);

/// 生成唯一的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let seq = FILE_SEQ.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nosqlite_bench_conc_{}_{}_{}.nodb",
        tag,
        std::process::id(),
        seq
    ))
}
