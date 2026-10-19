//! MongoDB wire 协议服务端(阶段 2)。
//!
//! 目标:让官方驱动(Go/Node/...)直连 `nodblite-server`,从而可替代 mongodb-server。
//!
//! 模块划分:
//! - [`bson`]:BSON ↔ JSON 编解码;`_id` 在协议层是 ObjectId,存储层是 hex 字符串;
//! - [`wire`]:OP_MSG 帧编解码与命令分发;
//! - [`serve`]:TCP 监听、连接处理。

pub mod bson;
pub mod serve;
pub mod wire;
