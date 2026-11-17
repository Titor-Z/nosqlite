//! 账号与 SCRAM-SHA-256 凭据(v0.7.0)。
//!
//! ## 存储位置:真实的 `admin.system.users` 集合
//!
//! 用户文档存在**普通集合** `admin.system.users` 里(不是隐藏表),这样
//! MongoDB 客户端/Compass 能像看别的集合一样看到它。代价是必须加保护,
//! 保护做在**边界**上(wire 层):
//! - 写路径拒绝 `admin.system.*`;
//! - 读时剥掉 `credentials`;
//! - `.collect` / `.export` 排除 `admin.system.*`。
//!
//! 嵌入式(CLI / C ABI)打开文件时不做任何认证 —— 能读文件字节就等于拥有
//! 全部数据,那层的保护靠 OS 权限与全盘加密,不靠密码。
//!
//! ## 凭据格式照抄 MongoDB
//!
//! `credentials["SCRAM-SHA-256"] = {iterationCount, salt, storedKey, serverKey}`
//! 全部 base64,算法为 RFC 5802/7677 标准定义:
//!
//! ```text
//! SaltedPassword = PBKDF2(password, salt, i, 32)
//! ClientKey      = HMAC(SaltedPassword, "Client Key")
//! StoredKey      = SHA256(ClientKey)
//! ServerKey      = HMAC(SaltedPassword, "Server Key")
//! ```

use serde_json::{Map, Value, json};

use crate::crypto::{base64, ct_eq, hmac_sha256, pbkdf2_sha256, random_bytes, sha256};
use crate::database::Database;
use crate::document::Document;

/// 用户所在的集合(物理名,`admin` 是保留命名空间)。
pub const USERS_COLLECTION: &str = "admin.system.users";

/// 版本集合(建库时写入,使 `admin` 库从创建那一刻就存在)。
pub const VERSION_COLLECTION: &str = "admin.system.version";

/// 认证机制名(只支持 256)。
pub const MECHANISM: &str = "SCRAM-SHA-256";

/// PBKDF2 迭代次数:MongoDB 对 SHA-256 的默认值。
///
/// 不取更高的 210000,是为了与 MongoDB 的凭据参数保持一致(便于对照与移植);
/// 迭代数存在凭据里,将来提高不会让老账号失效。
pub const ITERATIONS: u32 = 15000;

/// 盐长度(字节)。
const SALT_LEN: usize = 16;

/// 一个用户的 SCRAM-SHA-256 凭据。
#[derive(Debug, Clone)]
pub struct Credential {
    /// PBKDF2 迭代次数。
    pub iterations: u32,
    /// 盐(原始字节)。
    pub salt: Vec<u8>,
    /// `SHA256(HMAC(SaltedPassword, "Client Key"))`。
    pub stored_key: [u8; 32],
    /// `HMAC(SaltedPassword, "Server Key")`。
    pub server_key: [u8; 32],
}

impl Credential {
    /// 由明文密码生成凭据(随机盐 + 固定迭代数)。
    pub fn from_password(password: &str) -> Self {
        let salt = random_bytes(SALT_LEN);
        Self::from_password_with_salt(password, salt, ITERATIONS)
    }

    /// 由明文密码 + 指定盐/迭代数生成(测试用,可复现 RFC 向量)。
    pub fn from_password_with_salt(password: &str, salt: Vec<u8>, iterations: u32) -> Self {
        let salted = pbkdf2_sha256(password.as_bytes(), &salt, iterations, 32);
        let client_key = hmac_sha256(&salted, b"Client Key");
        let stored_key = sha256(&client_key);
        let server_key = hmac_sha256(&salted, b"Server Key");
        Credential {
            iterations,
            salt,
            stored_key,
            server_key,
        }
    }

    /// 计算服务端签名 `HMAC(ServerKey, AuthMessage)`,即 SCRAM 的 `v=` 值。
    pub fn server_signature(&self, auth_message: &[u8]) -> [u8; 32] {
        hmac_sha256(&self.server_key, auth_message)
    }

    /// 校验客户端证明 `p=`。
    ///
    /// 做法:由 proof 反推 `ClientKey = proof XOR HMAC(StoredKey, AuthMessage)`,
    /// 再验证 `SHA256(ClientKey) == StoredKey`。成功返回服务端签名。
    ///
    /// 这正是 SCRAM 的定义(服务端**不存明文密码**,也不需要 salted password)。
    pub fn verify(&self, auth_message: &[u8], proof: &[u8]) -> Option<[u8; 32]> {
        if proof.len() != 32 {
            return None;
        }
        let client_signature = hmac_sha256(&self.stored_key, auth_message);
        let mut client_key = [0u8; 32];
        for i in 0..32 {
            client_key[i] = proof[i] ^ client_signature[i];
        }
        if !ct_eq(&sha256(&client_key), &self.stored_key) {
            return None;
        }
        Some(self.server_signature(auth_message))
    }

    /// 序列化为 MongoDB 的 `credentials` 子文档。
    pub fn to_json(&self) -> Value {
        json!({
            "iterationCount": self.iterations,
            "salt": base64::encode(&self.salt),
            "storedKey": base64::encode(&self.stored_key),
            "serverKey": base64::encode(&self.server_key),
        })
    }

    /// 从 MongoDB 的 `credentials` 子文档还原。
    pub fn from_json(v: &Value) -> Option<Self> {
        let m = v.as_object()?;
        let iterations = m.get("iterationCount")?.as_u64()? as u32;
        let salt = base64::decode(m.get("salt")?.as_str()?)?;
        let stored = base64::decode(m.get("storedKey")?.as_str()?)?;
        let server = base64::decode(m.get("serverKey")?.as_str()?)?;
        if stored.len() != 32 || server.len() != 32 {
            return None;
        }
        let mut stored_key = [0u8; 32];
        let mut server_key = [0u8; 32];
        stored_key.copy_from_slice(&stored);
        server_key.copy_from_slice(&server);
        Some(Credential {
            iterations,
            salt,
            stored_key,
            server_key,
        })
    }
}

/// 用户文档的 `_id`:MongoDB 用 `<db>.<user>`,我们只有 admin。
pub fn user_id(name: &str) -> String {
    format!("admin.{}", name)
}

/// 在 `admin.system.users` 上操作账号。
pub struct UserStore<'a> {
    db: &'a Database,
}

impl<'a> UserStore<'a> {
    /// 绑定数据库。
    pub fn new(db: &'a Database) -> Self {
        UserStore { db }
    }

    /// 数据库里是否有任何用户(决定是否需要认证)。
    pub fn has_users(&self) -> bool {
        self.db
            .count(USERS_COLLECTION, &crate::query::Filter::match_all())
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    /// 取用户名 + 凭据(认证流程用)。
    pub fn credential(&self, name: &str) -> Option<Credential> {
        let doc = self.db.get_by_id(USERS_COLLECTION, &user_id(name)).ok()??;
        let creds = doc.get("credentials")?.as_object()?;
        Credential::from_json(creds.get(MECHANISM)?)
    }

    /// 列出全部用户文档(**已剥掉 `credentials`**,可安全外传)。
    pub fn list(&self) -> Vec<Map<String, Value>> {
        let filter = crate::query::Filter::match_all();
        let docs = self.db.find(USERS_COLLECTION, &filter).unwrap_or_default();
        docs.into_iter().map(strip_secrets).collect()
    }

    /// 创建用户。已存在时返回错误(用 [`Self::set_password`] 改密码)。
    pub fn create(&self, name: &str, password: &str) -> Result<(), String> {
        validate_name(name)?;
        if self.credential(name).is_some()
            || self
                .db
                .get_by_id(USERS_COLLECTION, &user_id(name))
                .map(|d| d.is_some())
                .unwrap_or(false)
        {
            return Err(format!("用户已存在: {}", name));
        }
        let cred = Credential::from_password(password);
        self.write_user(name, &cred)
    }

    /// 重设密码(用户不存在则报错)。
    pub fn set_password(&self, name: &str, password: &str) -> Result<(), String> {
        validate_name(name)?;
        if self
            .db
            .get_by_id(USERS_COLLECTION, &user_id(name))
            .map(|d| d.is_none())
            .unwrap_or(true)
        {
            return Err(format!("用户不存在: {}", name));
        }
        let cred = Credential::from_password(password);
        self.write_user(name, &cred)
    }

    /// 删除用户,返回是否真的删掉了。
    pub fn drop_user(&self, name: &str) -> Result<bool, String> {
        self.db
            .delete_by_id(USERS_COLLECTION, &user_id(name))
            .map_err(|e| e.to_string())
    }

    /// 写入(覆盖)用户文档。第一个用户自动获得 `root` 角色。
    fn write_user(&self, name: &str, cred: &Credential) -> Result<(), String> {
        let roles = if self.has_users() {
            json!([{"role": "readWrite", "db": "admin"}])
        } else {
            json!([{"role": "root", "db": "admin"}])
        };
        let fields = json!({
            "user": name,
            "db": "admin",
            "mechanisms": [MECHANISM],
            "credentials": {MECHANISM: cred.to_json()},
            "customData": {},
            "roles": roles,
        });

        let existing = self
            .db
            .get_by_id(USERS_COLLECTION, &user_id(name))
            .map_err(|e| e.to_string())?;

        match existing {
            // 已有:只覆写内容,主键不变
            Some(_) => {
                let update = crate::query::Update::from_value(&json!({"$set": fields}))
                    .map_err(|e| e.to_string())?;
                self.db
                    .update_by_id(USERS_COLLECTION, &user_id(name), &update)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            // 新建:显式指定 `_id`,与 MongoDB 的 `admin.<user>` 一致
            None => {
                let mut value = fields.as_object().cloned().unwrap_or_default();
                value.insert("_id".to_string(), Value::String(user_id(name)));
                let mut doc = Document::from_value(Value::Object(value)).map_err(|e| e.to_string())?;
                self.db
                    .insert(USERS_COLLECTION, &mut doc)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
    }
}

/// 剥掉密码摘要,只留可外传的字段。
pub fn strip_secrets(mut doc: Map<String, Value>) -> Map<String, Value> {
    doc.remove("credentials");
    doc
}

/// 用户名合法性(与 MongoDB 一致:非空、不含空格与 `$`、不以 `.` 结尾)。
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("用户名不能为空".to_string());
    }
    if name.len() > 64 {
        return Err("用户名过长(上限 64 字符)".to_string());
    }
    if name.contains(char::is_whitespace) || name.contains('$') || name.ends_with('.') {
        return Err(format!("用户名非法: {}", name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_roundtrip_and_verify() {
        let cred = Credential::from_password("pencil");
        let doc = cred.to_json();
        let back = Credential::from_json(&doc).expect("凭据应可还原");
        assert_eq!(back.iterations, cred.iterations);
        assert_eq!(back.salt, cred.salt);
        assert_eq!(back.stored_key, cred.stored_key);
        assert_eq!(back.server_key, cred.server_key);
    }

    #[test]
    fn wrong_password_fails_verification() {
        let right = Credential::from_password_with_salt("pencil", b"salt".to_vec(), 4096);
        let wrong = Credential::from_password_with_salt("eraser", b"salt".to_vec(), 4096);
        // 用 known salt,由正确密码猜出的 proof 必须与另一密码不同
        let auth_message = b"auth-message";
        // 构造一个「用正确密码算出的 proof」
        let client_key_sig = hmac_sha256(&right.stored_key, auth_message);
        let salted = pbkdf2_sha256(b"pencil", &right.salt, right.iterations, 32);
        let client_key = hmac_sha256(&salted, b"Client Key");
        let proof: Vec<u8> = client_key
            .iter()
            .zip(client_key_sig.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        assert!(right.verify(auth_message, &proof).is_some(), "正确密码应通过");
        assert!(wrong.verify(auth_message, &proof).is_none(), "错误密码必须失败");
    }

    #[test]
    fn name_validation() {
        assert!(validate_name("alice").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("a b").is_err());
        assert!(validate_name("a$b").is_err());
        assert!(validate_name("a.").is_err());
    }

    #[test]
    fn strip_secrets_removes_credentials() {
        let doc = json!({"user": "a", "credentials": {"SCRAM-SHA-256": {}}});
        let out = strip_secrets(doc.as_object().unwrap().clone());
        assert!(out.get("credentials").is_none());
        assert_eq!(out.get("user").unwrap(), "a");
    }
}
