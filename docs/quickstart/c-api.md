# 快速开始:C ABI

适合 C / C++ / Python(ctypes) / 其他能调 C 的语言。

## 获取库

**方式一:预编译包**(推荐)

从 [Releases](https://github.com/Titor-Z/nosqlite/releases) 下载对应平台的 `nodblite-ffi-<platform>.tar.gz`,内含:

- 动态库(`libnodblite.so` / `.dylib` / `nodblite.dll`)
- 静态库(`libnodblite.a`)
- 头文件 `include/nodblite.h`
- `.sha256` 校验

**方式二:自己构建**

```bash
cargo build --release
# 产物在 target/release/
```

## 完整 C 示例

```c
#include <stdio.h>
#include "nodblite.h"

int main(void) {
    // 1. 打开(不存在则创建)
    char err[512];
    NodbliteDb *db = nodblite_open(
        "app.nodb",
        NODBLITE_OPEN_CREATE | NODBLITE_OPEN_READWRITE);
    if (!db) {
        fprintf(stderr, "打开失败: %s\n", nodblite_last_error());
        return 1;
    }

    // 2. 插入
    NodbliteResult *res = nodblite_insert(
        db, "users", "{\"name\": \"张三\", \"age\": 30}");
    if (nodblite_last_error_code() != NODBLITE_OK) {
        fprintf(stderr, "插入失败[%d]: %s\n",
                nodblite_last_error_code(), nodblite_last_error());
        nodblite_result_free(res);
        nodblite_close(db);
        return 1;
    }

    // 3. 查询(arena 结果,一次释放)
    res = nodblite_find(
        db, "users", "{\"age\": {\"$gte\": 18}}",
        NULL /* sort */, NULL /* fields */, 0 /* skip */, 0 /* limit */);
    if (nodblite_last_error_code() == NODBLITE_OK) {
        size_t n = nodblite_result_count(res);
        for (size_t i = 0; i < n; i++) {
            const char *json;
            size_t len;
            nodblite_result_get(res, i, &json, &len);
            printf("%.*s\n", (int)len, json);
        }
    }
    nodblite_result_free(res);   // 整个结果一次释放,不用逐字段 free

    nodblite_close(db);
    return 0;
}
```

编译:

```bash
cc demo.c -Iinclude -Ltarget/release -lnodblite -o demo
./demo
```

## 三条设计原则 {#design}

这套 ABI 骨架照搬 SQLite(不透明句柄 + 错误码 + 游标),但修掉它因 2004 年 ABI 冻结而做不到的三件事:

### 1. arena 结果 —— 一次释放

查询返回的 `NodbliteResult*` 内部管理所有字符串,你只需 `nodblite_result_free()` **调用一次**。不用逐字段 free,不会漏。

### 2. 结构化错误 —— 可程序判断

```c
int code = nodblite_last_error_code();              // 数值码
const char *str = nodblite_last_error_code_str();   // "DUPLICATE_KEY" 等
const char *msg = nodblite_last_error();            // 人类可读
```

错误码覆盖:参数非法、后缀非法、主键冲突、过滤器语法错误、更新操作符错误、唯一索引冲突、IO 错误等 12 类。

### 3. 版本化 options —— 不需要 `_v2` 函数

```c
NodbliteOpenOptions opts = {0};
opts.struct_size = sizeof(opts);   // 版本化:未来加字段不用新函数
NodbliteDb *db = nodblite_open_ex("app.nodb", &opts);
```

## 游标(大结果集)

`nodblite_find` 会一次性把结果放进内存。大结果集请用游标逐条取:

```c
NodbliteCursor *cur = nodblite_find_cursor(
    db, "logs", "{}", NULL, NULL);
const char *json; size_t len;
while (nodblite_cursor_next(cur, &json, &len) == NODBLITE_OK) {
    // 处理单条
}
nodblite_cursor_close(cur);
```

游标内存为 **O(1)**,不是 O(结果集)。

## 线程

同一句柄可跨线程使用,但**不要并发 `close`**。错误信息是线程局部的。
