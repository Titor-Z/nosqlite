/*
 * nosqlite.h —— nosqlite 的 C ABI 头文件(阶段 1)
 *
 * 设计立场:骨架照搬 SQLite(不透明句柄 + 错误码 + 游标),但修掉它因
 * 2004 年 ABI 冻结而做不到的三件事:
 *   1. arena 结果   —— 整个结果一次释放,不用逐字段 free;
 *   2. 结构化错误   —— 错误码 + 消息 + 操作名,可程序判断;
 *   3. 版本化 options —— struct_size 演进,不需要 _v2 新函数。
 *
 * 内存约定:
 *   - nosqlite_open  返回的句柄用 nosqlite_close 释放;
 *   - nosqlite_find_cursor 返回的游标用 nosqlite_cursor_close 释放;
 *   - 其余函数返回 NosqliteResult*,一律用 nosqlite_result_free 释放(只需一次);
 *   - 所有切片(ptr+len)在对应结果/游标释放前有效。
 *
 * 线程:同一句柄可跨线程使用,但不要并发 close。错误信息是线程局部的。
 */

#ifndef NOSQLITE_H
#define NOSQLITE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* 成功码(所有错误码均非 0) */
#define NOSQLITE_OK 0

/* 稳定错误码(与 Rust 侧 ErrorCode::as_i32() 一一对应) */
#define NOSQLITE_DUPLICATE_KEY      1
#define NOSQLITE_INVALID_PATH       2
#define NOSQLITE_INVALID_FORMAT     3
#define NOSQLITE_VERSION_MISMATCH   4
#define NOSQLITE_INVALID_COLLECTION 5
#define NOSQLITE_INVALID_FILTER     6
#define NOSQLITE_INVALID_UPDATE     7
#define NOSQLITE_INVALID_SORT       8
#define NOSQLITE_INVALID_PROJECTION 9
#define NOSQLITE_INVALID_DOCUMENT   10
#define NOSQLITE_IO_ERROR           11
#define NOSQLITE_INTERNAL           99

/* 字符串切片(UTF-8,不保证 NUL 结尾,用 len 定界) */
typedef struct {
    const char *ptr;
    size_t      len;
} NosqliteSlice;

/* 结构化错误(线程局部,由 nosqlite_last_error 返回,无需释放) */
typedef struct {
    int32_t       code;       /* 稳定错误码 */
    NosqliteSlice code_str;   /* "DUPLICATE_KEY" 等 */
    NosqliteSlice message;    /* 中文消息 */
    NosqliteSlice op;         /* 出错的操作名 */
} NosqliteError;

/* 查询结果(arena:只调一次 nosqlite_result_free) */
typedef struct {
    int32_t       code;       /* 0 = 成功 */
    uint64_t      count;      /* docs 条数 */
    uint64_t      affected;   /* update/delete/count/upsert 的计数 */
    NosqliteSlice *docs;      /* 文档 JSON 数组 */
    NosqliteSlice value;      /* 单值结果(insert 的 _id 等),无则 len=0 */
} NosqliteResult;

/* 打开选项(struct_size 版本化) */
typedef struct {
    uint32_t struct_size;       /* 必须填 sizeof(NosqliteOpenOptions) */
    uint32_t flags;             /* 保留,当前必须为 0 */
    uint8_t  create_if_missing; /* 文件不存在时是否创建 */
    uint8_t  _pad[7];           /* 置 0 */
} NosqliteOpenOptions;

/* 查询选项(struct_size 版本化) */
typedef struct {
    uint32_t struct_size;
    uint64_t skip;
    uint64_t limit;
    uint8_t  limit_is_null;   /* 1 = 不限制 */
    uint8_t  _pad[7];
} NosqliteFindOptions;

/* 不透明句柄 */
typedef struct NosqliteDb     NosqliteDb;
typedef struct NosqliteCursor NosqliteCursor;

/* ---------- 生命周期 ---------- */

/* 打开(或按 create_if_missing 创建)数据库。失败返回 NULL,错误见 nosqlite_last_error。 */
NosqliteDb *nosqlite_open(const char *path, uint8_t create_if_missing);

/*
 * 用选项结构打开(可扩展入口)。
 * opts 为 NULL 时等价于 nosqlite_open(path, 1)。
 * opts->struct_size 必须 >= sizeof(NosqliteOpenOptions),flags 必须为 0。
 */
NosqliteDb *nosqlite_open_ex(const char *path, const NosqliteOpenOptions *opts);

/* 关闭句柄。 */
void nosqlite_close(NosqliteDb *db);

/* ---------- 写入 ---------- */

/* 插入一条文档(JSON 对象字符串)。value = _id 的 JSON(带引号)。 */
NosqliteResult *nosqlite_insert(NosqliteDb *db, const char *collection, const char *doc_json);

/* 批量插入(JSON 数组字符串)。value = _id 数组的 JSON。 */
NosqliteResult *nosqlite_insert_many(NosqliteDb *db, const char *collection, const char *docs_json);

/* 按 _id 读单条。value = 文档 JSON;不存在则 value.len == 0。 */
NosqliteResult *nosqlite_get(NosqliteDb *db, const char *collection, const char *id);

/* 更新匹配文档。affected = 实际修改条数。 */
NosqliteResult *nosqlite_update(NosqliteDb *db, const char *collection,
                                const char *filter_json, const char *update_json);

/* 更新或插入。value = {"inserted":"<id>"} 或 {"updated":N}。 */
NosqliteResult *nosqlite_upsert(NosqliteDb *db, const char *collection,
                                const char *filter_json, const char *update_json);

/* 删除匹配文档。affected = 删除条数。 */
NosqliteResult *nosqlite_delete(NosqliteDb *db, const char *collection, const char *filter_json);

/* ---------- 读取 ---------- */

/*
 * 查询并按结果集返回(适合小结果集)。
 * filter/sort/fields 允许 NULL(表示缺省)。
 * limit_is_null = 1 表示不限制条数。
 */
NosqliteResult *nosqlite_find(NosqliteDb *db, const char *collection,
                              const char *filter_json, const char *sort_json,
                              const char *fields_json, uint64_t skip,
                              uint64_t limit, uint8_t limit_is_null);

/*
 * 游标查询(适合大结果集,分块加载,内存 O(批大小))。
 *
 *   NosqliteCursor *cur = nosqlite_find_cursor(...);
 *   const char *doc;
 *   while ((doc = nosqlite_cursor_next(cur)) != NULL) { ... }
 *   nosqlite_cursor_close(cur);
 *
 * 返回的指针在下一次 cursor_next 或 cursor_close 之前有效。
 */
NosqliteCursor *nosqlite_find_cursor(NosqliteDb *db, const char *collection,
                                     const char *filter_json, const char *sort_json,
                                     const char *fields_json, uint64_t skip,
                                     uint64_t limit, uint8_t limit_is_null);

/* 推进游标并返回当前文档 JSON;结束返回 NULL。 */
const char *nosqlite_cursor_next(NosqliteCursor *cur);

/* 关闭游标。 */
void nosqlite_cursor_close(NosqliteCursor *cur);

/* 统计匹配条数。affected = 条数。 */
NosqliteResult *nosqlite_count(NosqliteDb *db, const char *collection, const char *filter_json);

/* 列出全部集合。docs 为集合名 JSON 数组。 */
NosqliteResult *nosqlite_list_collections(NosqliteDb *db);

/* ---------- 兜底 ---------- */

/*
 * 执行一条 JSON 命令:
 *   {"op":"insert", "collection":"u", "doc":{...}}
 *   {"op":"find", "collection":"u", "filter":{...}, "sort":{...},
 *    "fields":{...}, "options":{"skip":0,"limit":10}}
 *   支持 op: insert/insert_many/get/find/update/upsert/delete/count/list_collections
 */
NosqliteResult *nosqlite_exec(NosqliteDb *db, const char *command_json);

/* ---------- 错误与释放 ---------- */

/* 本条线程最近一次错误的错误码(0 = 无错误)。 */
int32_t nosqlite_last_error_code(void);

/* 本条线程最近一次错误的结构化信息(无需释放,下次调用本库前有效)。 */
const NosqliteError *nosqlite_last_error(void);

/* 释放结果及其持有的全部数据(整块释放,只调一次)。 */
void nosqlite_result_free(NosqliteResult *r);

#ifdef __cplusplus
}
#endif

#endif /* NOSQLITE_H */
