/*
 * C ABI 冒烟测试:用真实 C 程序验证链接、内存约定与基本语义。
 *
 * 构建(由 tests/c_api.rs 调起):
 *   cc smoke.c -I../include -L../target/release -lnosqlite -o smoke
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nosqlite.h"

static int failures = 0;

#define CHECK(cond, msg)                                                        \
    do {                                                                        \
        if (!(cond)) {                                                          \
            const NosqliteError *e = nosqlite_last_error();                     \
            fprintf(stderr, "FAIL: %s\n", msg);                                 \
            if (e) {                                                            \
                fprintf(stderr, "  code=%d msg=%.*s op=%.*s\n", (int)e->code,   \
                        (int)e->message.len, e->message.ptr,                    \
                        (int)e->op.len, e->op.ptr);                             \
            }                                                                   \
            failures++;                                                         \
        }                                                                       \
    } while (0)

/* 把切片复制成可 printf 的 C 字符串 */
static char *dup_slice(NosqliteSlice s) {
    char *out = (char *)malloc(s.len + 1);
    if (!out) return NULL;
    if (s.ptr) memcpy(out, s.ptr, s.len);
    out[s.len] = '\0';
    return out;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <db-path>\n", argv[0]);
        return 2;
    }
    const char *path = argv[1];

    /* ---------- 打开(自动创建) ---------- */
    NosqliteDb *db = nosqlite_open(path, 1);
    CHECK(db != NULL, "open 应成功");

    /* ---------- 插入 ---------- */
    NosqliteResult *r = nosqlite_insert(db, "users", "{\"name\":\"张三\",\"age\":30}");
    CHECK(r->code == NOSQLITE_OK, "insert 应成功");
    char *id = dup_slice(r->value);
    CHECK(id != NULL && strlen(id) > 2, "insert 应返回 _id");
    nosqlite_result_free(r);

    /* value 是裸 _id(不是 JSON 字符串),用作主键时需自己加引号 */
    char doc[256];
    snprintf(doc, sizeof(doc), "{\"_id\":\"%s\",\"name\":\"重复\"}", id);
    r = nosqlite_insert(db, "users", doc);
    CHECK(r->code == NOSQLITE_DUPLICATE_KEY, "重复主键应返回 DUPLICATE_KEY");
    nosqlite_result_free(r);
    {
        const NosqliteError *e = nosqlite_last_error();
        CHECK(e != NULL && e->code == NOSQLITE_DUPLICATE_KEY, "last_error 应给出冲突码");
        CHECK(e != NULL && e->op.len == 6, "last_error 应记录操作名 insert");
    }

    /* ---------- 批量插入 ---------- */
    r = nosqlite_insert_many(db, "users",
                             "[{\"name\":\"李四\",\"age\":20},{\"name\":\"王五\",\"age\":40}]");
    CHECK(r->code == NOSQLITE_OK, "insert_many 应成功");
    nosqlite_result_free(r);

    /* ---------- count ---------- */
    r = nosqlite_count(db, "users", NULL);
    CHECK(r->code == NOSQLITE_OK && r->affected == 3, "count 应为 3");
    nosqlite_result_free(r);

    /* ---------- find(一次性) ---------- */
    r = nosqlite_find(db, "users", "{\"age\":{\"$gte\":30}}", "{\"age\":-1}", NULL, 0, 0, 1);
    CHECK(r->code == NOSQLITE_OK && r->count == 2, "find 应返回 2 条");
    if (r->count == 2) {
        char *first = dup_slice(r->docs[0]);
        CHECK(first && strstr(first, "王五") != NULL, "排序后首条应为王五(age 40)");
        free(first);
    }
    nosqlite_result_free(r);

    /* ---------- 游标 ---------- */
    NosqliteCursor *cur = nosqlite_find_cursor(db, "users", NULL, "{\"age\":1}", NULL, 0, 0, 1);
    CHECK(cur != NULL, "find_cursor 应成功");
    int n = 0;
    int ordered = 1;
    int prev_age = -1;
    const char *json;
    while ((json = nosqlite_cursor_next(cur)) != NULL) {
        const char *p = strstr(json, "\"age\":");
        if (p) {
            int age = atoi(p + 6);
            if (age < prev_age) ordered = 0;
            prev_age = age;
        }
        n++;
    }
    CHECK(n == 3, "游标应遍历 3 条");
    CHECK(ordered, "游标结果应按 age 升序");
    nosqlite_cursor_close(cur);

    /* ---------- 更新(带新操作符) ---------- */
    r = nosqlite_update(db, "users", "{\"name\":\"李四\"}", "{\"$inc\":{\"age\":1},\"$set\":{\"vip\":true}}");
    CHECK(r->code == NOSQLITE_OK && r->affected == 1, "update 应修改 1 条");
    nosqlite_result_free(r);

    /* ---------- upsert:未命中则插入 ---------- */
    r = nosqlite_upsert(db, "users", "{\"name\":\"赵六\"}", "{\"$set\":{\"age\":18}}");
    CHECK(r->code == NOSQLITE_OK, "upsert 应成功");
    {
        char *v = dup_slice(r->value);
        CHECK(v && strstr(v, "inserted") != NULL, "upsert 未命中应返回 inserted");
        free(v);
    }
    nosqlite_result_free(r);

    /* ---------- get ---------- */
    r = nosqlite_get(db, "users", id);
    CHECK(r->code == NOSQLITE_OK && r->value.len > 0, "get 应返回文档");
    nosqlite_result_free(r);

    /* ---------- 投影 ---------- */
    r = nosqlite_find(db, "users", "{\"name\":\"张三\"}", NULL, "{\"name\":1,\"_id\":0}", 0, 0, 1);
    CHECK(r->code == NOSQLITE_OK && r->count == 1, "投影查询应返回 1 条");
    if (r->count == 1) {
        char *doc2 = dup_slice(r->docs[0]);
        CHECK(doc2 && strstr(doc2, "age") == NULL, "投影应裁掉未列入字段");
        free(doc2);
    }
    nosqlite_result_free(r);

    /* ---------- exec 兜底 ---------- */
    r = nosqlite_exec(db, "{\"op\":\"count\",\"collection\":\"users\"}");
    CHECK(r->code == NOSQLITE_OK && r->affected == 4, "exec count 应为 4");
    nosqlite_result_free(r);

    /* ---------- 集合列表 ---------- */
    r = nosqlite_list_collections(db);
    CHECK(r->code == NOSQLITE_OK && r->count == 1, "应有 1 个集合");
    nosqlite_result_free(r);

    /* ---------- 删除 ---------- */
    r = nosqlite_delete(db, "users", "{\"age\":{\"$lt\":25}}");
    CHECK(r->code == NOSQLITE_OK && r->affected == 2, "delete 应删除 2 条");
    nosqlite_result_free(r);

    /* ---------- 错误路径:非法 filter ---------- */
    r = nosqlite_find(db, "users", "{bad json}", NULL, NULL, 0, 0, 1);
    CHECK(r->code == NOSQLITE_INVALID_FILTER, "非法 filter 应返回 INVALID_FILTER");
    nosqlite_result_free(r);

    /* ---------- 关闭 ---------- */
    nosqlite_close(db);
    free(id);

    if (failures == 0) {
        printf("C ABI smoke: ALL PASSED\n");
        return 0;
    }
    fprintf(stderr, "C ABI smoke: %d failure(s)\n", failures);
    return 1;
}
