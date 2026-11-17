//! 密码学原语:只为 SCRAM-SHA-256 认证服务。
//!
//! **不自研加密**:这里实现的是**公开标准算法**(FIPS 180-4 / RFC 2104 / RFC 8018),
//! 每个都有 RFC 官方测试向量锁定([`tests`]),算错会被立刻抓到。
//! 真正不能自研的是「随机源」—— 那部分交给 `getrandom` crate。
//!
//! 不做的事(明确):
//! - 不做 AES / 不做加密(文件加密已决定不做,见 taolun #82);
//! - 不做 SHA-1 / MD5(SCRAM-SHA-1 与 MONGODB-CR 均已过时)。

pub mod base64;
pub mod sha256;

pub use sha256::{hmac_sha256, pbkdf2_sha256, sha256};

/// 生成 `len` 字节密码学安全随机数(用于 SCRAM 的 salt)。
///
/// 失败直接返回空 —— 调用方会因此算出可预测的 salt,但这种情况只可能是
/// 系统随机源不可用(内核异常),此时更该让上层报错。
pub(crate) fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    if getrandom::fill(&mut buf).is_err() {
        return Vec::new();
    }
    buf
}

/// 定长比较,抗时序侧信道(比较密码摘要时用)。
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bytes_vary() {
        let a = random_bytes(16);
        let b = random_bytes(16);
        assert_eq!(a.len(), 16);
        assert_ne!(a, b, "两次取随机数不应相同");
    }

    #[test]
    fn ct_eq_works() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
