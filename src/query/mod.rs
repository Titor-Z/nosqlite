//! 查询模块(M3/M4)。
//!
//! 仿 MongoDB 文档过滤语法,提供:
//!
//! - [`Filter`]:解析并匹配查询条件(操作符子集 `$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`,
//!   支持点号路径与顶层 `$and/$or/$nor/$not` 逻辑组合)。
//! - [`SortSpec`]:排序规格(`{"age": 1, "name": -1}`,1 升序 / -1 降序,多字段 + 点号路径)。
//! - [`FindOptions`](options::FindOptions):查询选项(排序、跳过、限制)。
//! - [`Update`]:更新操作(`$set` 字段点号路径赋值,嵌套自动创建/覆盖;路径段可为数组下标)。

mod compare;
mod filter;
mod options;
mod update;

pub use compare::{as_date, compare_values, parse_iso8601};
pub use filter::{Filter, FilterError};
pub use options::{FindOptions, Projection, ProjectionError, SortError, SortSpec};
pub use update::{Update, UpdateError};
