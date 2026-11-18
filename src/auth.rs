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

/// 内置角色(用 MongoDB 的词汇表;单库架构下语义如下,见 [`AuthContext`])。
pub const BUILTIN_ROLES: &[&str] = &[
    "read",
    "readWrite",
    "dbAdmin",
    "userAdmin",
    "dbOwner",
    "root",
];

/// 授权检查要判定的动作类别。
///
/// 不做更细的粒度:字段级 / 行级权限永远是应用的职责(MongoDB 也一样)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 读数据与元数据(find / aggregate / count / distinct / explain / getMore …)。
    Read,
    /// 写数据(insert / update / delete / findAndModify)。
    Write,
    /// 集合与索引管理(createIndexes / drop / dropDatabase …)。
    Admin,
    /// 账号管理(createUser / dropUser / usersInfo …)。
    UserAdmin,
}

impl Action {
    /// 用于错误信息的中文名。
    pub fn describe(&self) -> &'static str {
        match self {
            Action::Read => "读取",
            Action::Write => "写入",
            Action::Admin => "管理(索引/集合)",
            Action::UserAdmin => "管理账号",
        }
    }
}

/// 一个角色条目,形状与 MongoDB 的用户文档一致:`{role, db}`。
///
/// `db` 在我们的单库架构下只作记录,不影响授权范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub role: String,
    pub db: String,
}

impl Role {
    /// 从 `{"role": "...", "db": "..."}` 还原。
    pub fn from_json(v: &Value) -> Option<Self> {
        let m = v.as_object()?;
        Some(Role {
            role: m.get("role")?.as_str()?.to_string(),
            db: m
                .get("db")
                .and_then(|v| v.as_str())
                .unwrap_or("admin")
                .to_string(),
        })
    }

    /// 该角色授予哪些动作。
    pub fn grants(&self, action: Action) -> bool {
        match self.role.as_str() {
            "root" | "dbOwner" => true,
            "readWrite" => matches!(action, Action::Read | Action::Write),
            "read" => matches!(action, Action::Read),
            "dbAdmin" => matches!(action, Action::Admin),
            "userAdmin" => matches!(action, Action::UserAdmin),
            // 未知角色名不应走到这里(`parse_roles` 会先报错),此处兜底为不授权
            _ => false,
        }
    }
}

/// 解析 `roles` 数组;未知角色名报错(MongoDB 语义:角色必须先存在)。
pub fn parse_roles(v: &Value) -> Result<Vec<Role>, String> {
    let arr = v
        .as_array()
        .ok_or_else(|| "roles 必须是数组".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        // MongoDB 允许字符串简写:`roles: ["readWrite"]`(db 缺省为 admin)
        let (role, db) = match item {
            Value::String(s) => (s.clone(), "admin".to_string()),
            Value::Object(m) => (
                m.get("role")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "角色项缺少 role 字段".to_string())?
                    .to_string(),
                m.get("db")
                    .and_then(|v| v.as_str())
                    .unwrap_or("admin")
                    .to_string(),
            ),
            _ => return Err("roles 每一项必须是 {role, db} 对象或角色名字符串".to_string()),
        };
        if !BUILTIN_ROLES.contains(&role.as_str()) {
            return Err(format!("未知角色: {}(内置:{})", role, BUILTIN_ROLES.join(", ")));
        }
        out.push(Role { role, db });
    }
    Ok(out)
}

/// 校验角色名全是内置角色。
///
/// 放在 `UserStore` 里而不是只放在 wire 解析层 —— CLI 的 `.user add <name> <role>`
/// 直接构造 `Role`,不经过 `parse_roles`,遗漏会让未知角色被静默写进库。
fn validate_roles(roles: &[Role]) -> Result<(), String> {
    for r in roles {
        if !BUILTIN_ROLES.contains(&r.role.as_str()) {
            return Err(format!(
                "未知角色: {}(内置:{})",
                r.role,
                BUILTIN_ROLES.join(", ")
            ));
        }
    }
    Ok(())
}

/// 认证通过后的身份与角色(每条连接一份,随认证结果装入 `ConnState`)。
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// 用户名。
    pub user: String,
    /// 角色列表。
    pub roles: Vec<Role>,
}

impl AuthContext {
    /// 是否拥有某动作的权限(任一角色授予即可)。
    pub fn allows(&self, action: Action) -> bool {
        self.roles.iter().any(|r| r.grants(action))
    }

    /// 能否看指定用户的资料:自己永远可以,看别人要 `userAdmin`/`root`。
    pub fn can_see_user(&self, name: &str) -> bool {
        name == self.user || self.allows(Action::UserAdmin)
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

    /// 取认证上下文(用户名 + 角色);用户不存在返回 `None`。
    pub fn context(&self, name: &str) -> Option<AuthContext> {
        let doc = self.db.get_by_id(USERS_COLLECTION, &user_id(name)).ok()??;
        let roles = parse_roles(doc.get("roles")?).ok()?;
        Some(AuthContext {
            user: name.to_string(),
            roles,
        })
    }

    /// 创建用户,角色由调用方指定(`None` = 默认:首账号 `root`,其余 `readWrite`)。
    pub fn create(
        &self,
        name: &str,
        password: &str,
        roles: Option<Vec<Role>>,
    ) -> Result<(), String> {
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
        let roles = roles.unwrap_or_else(|| default_roles(!self.has_users()));
        validate_roles(&roles)?;
        let cred = Credential::from_password(password);
        self.write_user(name, &cred, &roles)
    }

    /// 改密码 / 改角色(至少给一项;`None` 表示不修改该项)。
    pub fn update(
        &self,
        name: &str,
        password: Option<&str>,
        roles: Option<Vec<Role>>,
    ) -> Result<(), String> {
        validate_name(name)?;
        if password.is_none() && roles.is_none() {
            return Err("updateUser 需要 pwd 或 roles 至少一项".to_string());
        }
        if self
            .db
            .get_by_id(USERS_COLLECTION, &user_id(name))
            .map(|d| d.is_none())
            .unwrap_or(true)
        {
            return Err(format!("用户不存在: {}", name));
        }
        let cred = match password {
            Some(pwd) => Credential::from_password(pwd),
            None => self.credential(name).expect("上面已确认用户存在"),
        };
        let roles = roles.unwrap_or_else(|| {
            self.context(name)
                .map(|c| c.roles)
                .unwrap_or_else(|| default_roles(false))
        });
        validate_roles(&roles)?;
        self.write_user(name, &cred, &roles)
    }

    /// 删除用户,返回是否真的删掉了。
    pub fn drop_user(&self, name: &str) -> Result<bool, String> {
        self.db
            .delete_by_id(USERS_COLLECTION, &user_id(name))
            .map_err(|e| e.to_string())
    }

    /// 写入(覆盖)用户文档。
    fn write_user(
        &self,
        name: &str,
        cred: &Credential,
        roles: &[Role],
    ) -> Result<(), String> {
        let roles = json!(roles
            .iter()
            .map(|r| json!({"role": r.role, "db": r.db}))
            .collect::<Vec<_>>());
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

/// 默认角色:库里还没有任何账号时第一个账号给 `root`,否则 `readWrite`。
fn default_roles(first_user: bool) -> Vec<Role> {
    vec![Role {
        role: (if first_user { "root" } else { "readWrite" }).to_string(),
        db: "admin".to_string(),
    }]
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
