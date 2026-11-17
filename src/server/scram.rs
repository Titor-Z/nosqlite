//! SCRAM-SHA-256 服务端状态机(v0.7.0)。
//!
//! 只支持 `SCRAM-SHA-256`、**不支持信道绑定**(`c=biws`,即 `n,,` 起始的 GS2 头)。
//! 这是 MongoDB 驱动与 mongosh 实际使用的形态。
//!
//! 协议序列(RFC 5802/7677):
//!
//! ```text
//! C: n,,n=user,r=<clientNonce>                      (saslStart)
//! S:  r=<clientNonce><serverNonce>,s=<salt>,i=<n>   (saslContinue)
//! C: c=biws,r=<combined>,p=<proof>                  (saslContinue)
//! S:  v=<serverSignature>                           (done)
//! ```
//!
//! 服务端**不存明文密码**,只用 `StoredKey` / `ServerKey` 校验与签名。

use crate::auth::Credential;
use crate::crypto::base64;
use crate::crypto::random_bytes;

/// SCRAM 交互中的错误(统一映射为 MongoDB 的 `AuthenticationFailed` / 18)。
#[derive(Debug, PartialEq)]
pub enum ScramError {
    /// 机制不支持(我们只做 SCRAM-SHA-256)。
    UnsupportedMechanism(String),
    /// 消息格式不合法。
    BadMessage(String),
    /// 用户不存在或密码错误(刻意不区分,避免用户名枚举)。
    AuthenticationFailed,
}

impl std::fmt::Display for ScramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScramError::UnsupportedMechanism(m) => {
                write!(f, "不支持的认证机制: {}(仅支持 SCRAM-SHA-256)", m)
            }
            ScramError::BadMessage(m) => write!(f, "SCRAM 消息非法: {}", m),
            ScramError::AuthenticationFailed => write!(f, "认证失败"),
        }
    }
}

/// 一步交互的输出。
#[derive(Debug)]
pub struct Step {
    /// 要回给客户端的 SASL payload(明文,上层转成 BSON binary)。
    pub payload: String,
    /// 是否已完成(客户端此时可发业务命令)。
    pub done: bool,
}

/// 服务端 SCRAM 会话(每条连接一个)。
pub struct ScramServer {
    /// 已认证的用户名(完成前为空)。
    user: String,
    /// 该用户的凭据。
    cred: Credential,
    /// 客户端的 client-first-bare(参与 AuthMessage)。
    client_first_bare: String,
    /// 服务端回过的 server-first 原文(参与 AuthMessage)。
    server_first: String,
    /// 完整 nonce(client + server 拼接)。
    combined_nonce: String,
    /// 是否已收到并校验过 client-final。
    verified: bool,
}

impl ScramServer {
    /// 处理 `saslStart`。
    ///
    /// `lookup` 负责按用户名取凭据(用户不存在返回 `None`)。
    pub fn start<F>(mechanism: &str, payload: &str, lookup: F) -> Result<(Self, Step), ScramError>
    where
        F: FnOnce(&str) -> Option<Credential>,
    {
        if mechanism != crate::auth::MECHANISM {
            return Err(ScramError::UnsupportedMechanism(mechanism.to_string()));
        }

        // GS2 头:必须是 `n,,`(客户端不支持信道绑定)
        let client_first_bare = payload
            .strip_prefix("n,,")
            .ok_or_else(|| ScramError::BadMessage("缺少 n,, 起始头(不支持信道绑定)".to_string()))?
            .to_string();

        let attrs = parse_attrs(&client_first_bare)?;
        let username = attrs
            .iter()
            .find(|(k, _)| *k == "n")
            .map(|(_, v)| unescape_name(v))
            .ok_or_else(|| ScramError::BadMessage("缺少用户名 n=".to_string()))?;
        let client_nonce = attrs
            .iter()
            .find(|(k, _)| *k == "r")
            .map(|(_, v)| v.clone())
            .ok_or_else(|| ScramError::BadMessage("缺少客户端 nonce r=".to_string()))?;
        if client_nonce.is_empty() {
            return Err(ScramError::BadMessage("客户端 nonce 为空".to_string()));
        }

        // 用户不存在与密码错误返回同一个错误,避免用户名枚举
        let cred = lookup(&username).ok_or(ScramError::AuthenticationFailed)?;

        // 服务端 nonce:24 字节随机数的 base64(不含逗号,可安全拼进消息)
        let server_nonce = base64::encode(&random_bytes(24));
        let combined_nonce = format!("{}{}", client_nonce, server_nonce);
        let server_first = format!(
            "r={},s={},i={}",
            combined_nonce,
            base64::encode(&cred.salt),
            cred.iterations
        );

        let server = ScramServer {
            user: username,
            cred,
            client_first_bare,
            server_first: server_first.clone(),
            combined_nonce,
            verified: false,
        };
        Ok((
            server,
            Step {
                payload: server_first,
                done: false,
            },
        ))
    }

    /// 处理 `saslContinue`。
    pub fn continue_with(&mut self, payload: &str) -> Result<Step, ScramError> {
        // 已认证后再来的空 payload:是驱动在 skipEmptyExchange 关闭时的收尾,直接确认
        if self.verified {
            return Ok(Step {
                payload: String::new(),
                done: true,
            });
        }

        // 客户端最终消息:截掉 `,p=` 之后的部分作为 AuthMessage 的第三段
        let proof_at = payload
            .rfind(",p=")
            .ok_or_else(|| ScramError::BadMessage("缺少客户端证明 p=".to_string()))?;
        let client_final_without_proof = &payload[..proof_at];
        let proof_b64 = &payload[proof_at + 3..];
        let proof = base64::decode(proof_b64)
            .ok_or_else(|| ScramError::BadMessage("证明不是合法 base64".to_string()))?;

        let attrs = parse_attrs(client_final_without_proof)?;
        let nonce = attrs
            .iter()
            .find(|(k, _)| *k == "r")
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| ScramError::BadMessage("缺少 nonce r=".to_string()))?;
        if nonce != self.combined_nonce {
            return Err(ScramError::BadMessage("nonce 不匹配".to_string()));
        }

        // AuthMessage = client-first-bare , server-first , client-final-without-proof
        let auth_message = format!(
            "{},{},{}",
            self.client_first_bare, self.server_first, client_final_without_proof
        );

        let signature = self
            .cred
            .verify(auth_message.as_bytes(), &proof)
            .ok_or(ScramError::AuthenticationFailed)?;

        self.verified = true;
        Ok(Step {
            payload: format!("v={}", base64::encode(&signature)),
            done: true,
        })
    }

    /// 已认证的用户名(未完成时为空串)。
    pub fn user(&self) -> &str {
        if self.verified { &self.user } else { "" }
    }
}

/// 把 `k=v,k=v` 拆成键值对(值里可能含 `=`?SCRAM 属性值不会,故按第一个 `=` 切)。
fn parse_attrs(text: &str) -> Result<Vec<(String, String)>, ScramError> {
    let mut out = Vec::new();
    for part in text.split(',') {
        if part.is_empty() {
            continue;
        }
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| ScramError::BadMessage(format!("属性缺少 `=` : {}", part)))?;
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

/// SCRAM 用户名转义:`=2C` → `,`,`=3D` → `=`。
fn unescape_name(name: &str) -> String {
    name.replace("=2C", ",").replace("=3D", "=")
}

/// 把明文 payload 包成 BSON binary 的 JSON 表示(`$binary` + base64)。
///
/// SCRAM 的 payload 在 wire 上是 `BinData(0, ...)`;驱动就是按这个形态发的。
pub fn payload_to_json(payload: &str) -> serde_json::Value {
    serde_json::json!({
        "$binary": {
            "base64": base64::encode(payload.as_bytes()),
            "subType": "00"
        }
    })
}

/// 从 BSON binary 的 JSON 表示取出明文 payload。
///
/// 兼容两种输入:标准 `$binary` 包装,以及驱动偶发的裸字符串(便于手测)。
pub fn payload_from_json(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    let b64 = v.get("$binary")?.get("base64")?.as_str()?;
    let raw = base64::decode(b64)?;
    String::from_utf8(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7677 §3 官方测试向量(SCRAM-SHA-256)。
    ///
    /// 这条测试同时锁定 PBKDF2 / HMAC / SHA-256 / 证明校验四件事,
    /// 任何一环算错都会立刻失败 —— 这是「我们的 SCRAM 与标准一致」的唯一硬证据。
    #[test]
    fn rfc7677_vector() {
        let salt = base64::decode("W22ZaJ0SNY7soEsUEjb6gQ==").unwrap();
        let cred = Credential::from_password_with_salt("pencil", salt, 4096);
        let client_first_bare = "n=user,r=rOprNGfwEbeRWgbNEkqO";
        let server_first =
            "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
        let client_final_no_proof =
            "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
        let auth_message = format!("{},{},{}", client_first_bare, server_first, client_final_no_proof);
        let proof = base64::decode("dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=").unwrap();

        let sig = cred
            .verify(auth_message.as_bytes(), &proof)
            .expect("RFC 向量必须通过校验");
        assert_eq!(
            base64::encode(&sig),
            "6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=",
            "服务端签名必须与 RFC 一致"
        );
    }

    /// 端到端:用我们自己算出的 proof 走完整交互。
    #[test]
    fn full_handshake_succeeds() {
        let cred = Credential::from_password("pencil");
        let (mut server, step1) = ScramServer::start(
            crate::auth::MECHANISM,
            "n,,n=alice,r=clientnonce123",
            |name| if name == "alice" { Some(cred.clone()) } else { None },
        )
        .unwrap();
        assert!(!step1.done);

        // 客户端据 server-first 算 proof(测试里手写一遍客户端逻辑)
        let server_first = &step1.payload;
        let salt = base64::decode(
            server_first
                .split(',')
                .find_map(|p| p.strip_prefix("s="))
                .unwrap(),
        )
        .unwrap();
        let combined = server_first
            .split(',')
            .find_map(|p| p.strip_prefix("r="))
            .unwrap()
            .to_string();
        let client_final_no_proof = format!("c=biws,r={}", combined);
        let auth_message = format!(
            "n=alice,r=clientnonce123,{},{}",
            server_first, client_final_no_proof
        );
        let salted = crate::crypto::pbkdf2_sha256(
            b"pencil",
            &salt,
            crate::auth::ITERATIONS,
            32,
        );
        let client_key = crate::crypto::hmac_sha256(&salted, b"Client Key");
        let stored = crate::crypto::sha256(&client_key);
        let sig = crate::crypto::hmac_sha256(&stored, auth_message.as_bytes());
        let proof: Vec<u8> = client_key
            .iter()
            .zip(sig.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        let client_final = format!("{},p={}", client_final_no_proof, base64::encode(&proof));

        let step2 = server.continue_with(&client_final).unwrap();
        assert!(step2.done);
        assert!(step2.payload.starts_with("v="));
        assert_eq!(server.user(), "alice");
    }

    #[test]
    fn unknown_user_is_indistinguishable() {
        let err = ScramServer::start(crate::auth::MECHANISM, "n,,n=ghost,r=abc", |_| None)
            .err()
            .unwrap();
        assert_eq!(err, ScramError::AuthenticationFailed);
    }

    #[test]
    fn rejects_other_mechanism() {
        let err = ScramServer::start("SCRAM-SHA-1", "n,,n=a,r=b", |_| None)
            .err()
            .unwrap();
        assert!(matches!(err, ScramError::UnsupportedMechanism(_)));
    }

    #[test]
    fn rejects_channel_binding_header() {
        let err = ScramServer::start(crate::auth::MECHANISM, "y,,n=a,r=b", |_| None)
            .err()
            .unwrap();
        assert!(matches!(err, ScramError::BadMessage(_)));
    }

    #[test]
    fn rejects_wrong_proof() {
        let cred = Credential::from_password("pencil");
        let (mut server, step1) = ScramServer::start(
            crate::auth::MECHANISM,
            "n,,n=alice,r=abc",
            move |_| Some(cred),
        )
        .unwrap();
        let combined = step1
            .payload
            .split(',')
            .find_map(|p| p.strip_prefix("r="))
            .unwrap()
            .to_string();
        let bogus = format!("c=biws,r={},p={}", combined, base64::encode(&[0u8; 32]));
        assert!(matches!(
            server.continue_with(&bogus),
            Err(ScramError::AuthenticationFailed)
        ));
    }

    #[test]
    fn payload_json_roundtrip() {
        let j = payload_to_json("n,,n=alice,r=abc");
        assert_eq!(payload_from_json(&j).unwrap(), "n,,n=alice,r=abc");
    }
}
