//! ObjectId 生成器(M2)。
//!
//! 与 MongoDB 对齐的 12 字节 ObjectId:
//!
//! | 字节 | 含义 |
//! |---|---|
//! | 0..4 | 4 字节时间戳(Unix 秒,大端序) |
//! | 4..9 | 5 字节随机数(进程内伪随机) |
//! | 9..12 | 3 字节计数器(进程内自增) |
//!
//! 12 字节 hex 编码为 24 字符小写字符串,作为文档主键 `_id`。
//! 时间戳保证按插入时间大致有序,随机 + 计数器保证同秒内唯一。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 进程内自增计数器(占 3 字节,见 [`ObjectId::generate`])。
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 进程内伪随机数种子(占 5 字节,见 [`ObjectId::generate`])。
static RANDOM: AtomicU64 = AtomicU64::new(0);

/// 进程标识:取 PID 参与随机种子,避免不同进程生成相同 ObjectId。
///
/// wasm 等无进程概念的目标(std::process::id 会 panic)退化为 0 ——
/// 此时由纳秒时间戳 + 进程内计数器保证唯一。
fn process_id() -> u64 {
    #[cfg(not(target_family = "wasm"))]
    {
        std::process::id() as u64
    }
    #[cfg(target_family = "wasm")]
    {
        0
    }
}

/// ObjectId 生成器。
pub struct ObjectId;

impl ObjectId {
    /// 生成一个新的 24 字符 hex ObjectId 字符串。
    ///
    /// 布局:4 字节时间戳 + 5 字节伪随机 + 3 字节计数器。
    /// 伪随机种子由进程启动时间与 PID 混合得到,保证不同进程间不同。
    pub fn generate() -> String {
        // 1. 4 字节时间戳:Unix 秒,大端序
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应晚于 Unix 纪元")
            .as_secs() as u32;
        let ts_bytes = ts.to_be_bytes();

        // 2. 5 字节伪随机:首次调用时按(启动纳秒 ^ PID)播种
        let mut seed = RANDOM.load(Ordering::Relaxed);
        if seed == 0 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("系统时间应晚于 Unix 纪元")
                .as_nanos() as u64;
            seed = now ^ process_id();
            RANDOM.store(seed, Ordering::Relaxed);
        }
        // 线性同余更新,取中间 5 字节
        let next = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        RANDOM.store(next, Ordering::Relaxed);
        let rand_bytes = (next >> 8) & 0xFF_FFFF_FFFF; // 取 5 字节(40 位)
        let rand_bytes = rand_bytes.to_be_bytes()[3..8].to_vec(); // 后 5 字节

        // 3. 3 字节计数器:进程内自增,取低 24 位
        let counter = (COUNTER.fetch_add(1, Ordering::Relaxed) & 0xFF_FFFF) as u32;
        let counter_bytes = counter.to_be_bytes()[1..4].to_vec(); // 后 3 字节

        // 拼接 12 字节并 hex 编码
        let mut bytes = Vec::with_capacity(12);
        bytes.extend_from_slice(&ts_bytes);
        bytes.extend_from_slice(&rand_bytes);
        bytes.extend_from_slice(&counter_bytes);
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成的 ID 长度必须为 24 字符且全为小写 hex。
    #[test]
    fn id_is_24_lowercase_hex() {
        let id = ObjectId::generate();
        assert_eq!(id.len(), 24, "ObjectId 应为 24 字符");
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "ObjectId 应为小写 hex: {}",
            id
        );
    }

    /// 连续生成的 ID 必须唯一。
    #[test]
    fn id_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = ObjectId::generate();
            assert!(seen.insert(id.clone()), "ID 不应重复: {}", id);
        }
    }

    /// ID 应按时间大致递增(时间戳字段在前 8 位)。
    #[test]
    fn id_increases_over_time() {
        let a = ObjectId::generate();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = ObjectId::generate();
        assert!(a < b, "时间推进后 ID 应更大: {} vs {}", a, b);
    }
}
