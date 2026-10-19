//! 索引模块(M23b)。
//!
//! - [`key`]:order-preserving 编码 —— 字节序 == BSON 值序,支撑 B+树 range 扫描;
//! - [`def`]:索引定义与条目 key 构造(multikey / 稀疏 / 嵌套路径);
//! - [`planner`]:查询规划器 —— 把 filter 条件翻译成索引区间。

pub mod def;
pub mod key;
pub mod planner;
