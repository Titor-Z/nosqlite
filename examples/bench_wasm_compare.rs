//! 写路径对照:`insert_many` 的「同一份 JSON 数组」在原生与 wasm 下的耗时。
//!
//! 与 `web/bench-write.mjs` 配对使用 —— 两者读同一个 JSON 数组文件、调同一个
//! `insert_many`,差别只在执行环境。用于判定 wasm 写性能损失的真实幅度。
//!
//! 用法:`cargo run --release --example bench_wasm_compare -- <json 数组文件> <库路径> [重复次数]`

use std::time::Instant;

use nodblite::{Database, Document};
use serde_json::Value;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_path = args.get(1).expect("用法:<json 数组文件> <库路径> [重复次数]");
    let db_path = args.get(2).expect("用法:<json 数组文件> <库路径> [重复次数]");
    let repeat: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);

    let text = std::fs::read_to_string(json_path).expect("读取 JSON 文件失败");
    let items: Vec<Value> = serde_json::from_str(&text).expect("JSON 数组解析失败");

    let mut times = Vec::with_capacity(repeat);
    for _ in 0..repeat {
        let _ = std::fs::remove_file(db_path);
        let db = Database::create(std::path::Path::new(db_path)).expect("创建库失败");
        let mut docs: Vec<Document> = items
            .iter()
            .map(|v| Document::from_value(v.clone()).expect("文档非法"))
            .collect();

        let t = Instant::now();
        db.insert_many("c", &mut docs).expect("批量插入失败");
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    println!(
        "native: 条数={} 中位={:.1}ms 最小={:.1}ms 最大={:.1}ms 吞吐={:.0} 条/秒",
        items.len(),
        median,
        times[0],
        times[times.len() - 1],
        items.len() as f64 / (median / 1000.0)
    );
}
