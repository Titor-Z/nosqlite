//! 查询模块(M3)。
//!
//! 仿 MongoDB 文档过滤语法,提供:
//!
//! - [`Filter`]:解析并匹配查询条件(操作符子集 `$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`,
//!   支持点号路径与顶层 `$and/$or/$nor/$not` 逻辑组合)。
//! - [`SortSpec`]:排序规格(`{"age": 1, "name": -1}`,1 升序 / -1 降序,多字段 + 点号路径)。
//! - [`FindOptions`](options::FindOptions):查询选项(排序、跳过、限制)。

mod compare;
mod filter;
mod options;

pub use compare::compare_values;
pub use filter::{Filter, FilterError};
pub use options::{FindOptions, SortError, SortSpec};
