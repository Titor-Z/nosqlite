/*
 * C ABI 冒烟测试:用真实 C 程序验证链接、内存约定与基本语义。
 *
 * 构建(由 tests/c_api.rs 调起):
 *   cc smoke.c -I../include -L../target/release -lnodblite -o smoke
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "nodblite.h"

static int failures = 0;

#define CHECK(cond, msg)                                                        \
    do {                                                                        \
        if (!(cond)) {                                                          \
            const NodbliteError *e = nodblite_last_error();                     \
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
static char *dup_slice(NodbliteSlice s) {
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
    NodbliteDb *db = nodblite_open(path, 1);
    CHECK(db != NULL, "open 应成功");

    /* ---------- 插入 ---------- */
    NodbliteResult *r = nodblite_insert(db, "users", "{\"name\":\"张三\",\"age\":30}");
    CHECK(r->code == NOSQLITE_OK, "insert 应成功");
    char *id = dup_slice(r->value);
    CHECK(id != NULL && strlen(id) > 2, "insert 应返回 _id");
    nodblite_result_free(r);

    /* value 是裸 _id(不是 JSON 字符串),用作主键时需自己加引号 */
    char doc[256];
    snprintf(doc, sizeof(doc), "{\"_id\":\"%s\",\"name\":\"重复\"}", id);
    r = nodblite_insert(db, "users", doc);
    CHECK(r->code == NOSQLITE_DUPLICATE_KEY, "重复主键应返回 DUPLICATE_KEY");
    nodblite_result_free(r);
    {
        const NodbliteError *e = nodblite_last_error();
        CHECK(e != NULL && e->code == NOSQLITE_DUPLICATE_KEY, "last_error 应给出冲突码");
        CHECK(e != NULL && e->op.len == 6, "last_error 应记录操作名 insert");
    }

    /* ---------- 批量插入 ---------- */
    r = nodblite_insert_many(db, "users",
                             "[{\"name\":\"李四\",\"age\":20},{\"name\":\"王五\",\"age\":40}]");
    CHECK(r->code == NOSQLITE_OK, "insert_many 应成功");
    nodblite_result_free(r);

    /* ---------- count ---------- */
    r = nodblite_count(db, "users", NULL);
    CHECK(r->code == NOSQLITE_OK && r->affected == 3, "count 应为 3");
    nodblite_result_free(r);

    /* ---------- find(一次性) ---------- */
    r = nodblite_find(db, "users", "{\"age\":{\"$gte\":30}}", "{\"age\":-1}", NULL, 0, 0, 1);
    CHECK(r->code == NOSQLITE_OK && r->count == 2, "find 应返回 2 条");
    if (r->count == 2) {
        char *first = dup_slice(r->docs[0]);
        CHECK(first && strstr(first, "王五") != NULL, "排序后首条应为王五(age 40)");
        free(first);
    }
    nodblite_result_free(r);

    /* ---------- 游标 ---------- */
    NodbliteCursor *cur = nodblite_find_cursor(db, "users", NULL, "{\"age\":1}", NULL, 0, 0, 1);
    CHECK(cur != NULL, "find_cursor 应成功");
    int n = 0;
    int ordered = 1;
    int prev_age = -1;
    const char *json;
    while ((json = nodblite_cursor_next(cur)) != NULL) {
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
    nodblite_cursor_close(cur);

    /* ---------- 更新(带新操作符) ---------- */
    r = nodblite_update(db, "users", "{\"name\":\"李四\"}", "{\"$inc\":{\"age\":1},\"$set\":{\"vip\":true}}");
    CHECK(r->code == NOSQLITE_OK && r->affected == 1, "update 应修改 1 条");
    nodblite_result_free(r);

    /* ---------- upsert:未命中则插入 ---------- */
    r = nodblite_upsert(db, "users", "{\"name\":\"赵六\"}", "{\"$set\":{\"age\":18}}");
    CHECK(r->code == NOSQLITE_OK, "upsert 应成功");
    {
        char *v = dup_slice(r->value);
        CHECK(v && strstr(v, "inserted") != NULL, "upsert 未命中应返回 inserted");
        free(v);
    }
    nodblite_result_free(r);

    /* ---------- get ---------- */
    r = nodblite_get(db, "users", id);
    CHECK(r->code == NOSQLITE_OK && r->value.len > 0, "get 应返回文档");
    nodblite_result_free(r);

    /* ---------- 投影 ---------- */
    r = nodblite_find(db, "users", "{\"name\":\"张三\"}", NULL, "{\"name\":1,\"_id\":0}", 0, 0, 1);
    CHECK(r->code == NOSQLITE_OK && r->count == 1, "投影查询应返回 1 条");
    if (r->count == 1) {
        char *doc2 = dup_slice(r->docs[0]);
        CHECK(doc2 && strstr(doc2, "age") == NULL, "投影应裁掉未列入字段");
        free(doc2);
    }
    nodblite_result_free(r);

    /* ---------- exec 兜底 ---------- */
    r = nodblite_exec(db, "{\"op\":\"count\",\"collection\":\"users\"}");
    CHECK(r->code == NOSQLITE_OK && r->affected == 4, "exec count 应为 4");
    nodblite_result_free(r);

    /* ---------- 集合列表 ---------- */
    r = nodblite_list_collections(db);
    CHECK(r->code == NOSQLITE_OK && r->count == 1, "应有 1 个集合");
    nodblite_result_free(r);

    /* ---------- 删除 ---------- */
    r = nodblite_delete(db, "users", "{\"age\":{\"$lt\":25}}");
    CHECK(r->code == NOSQLITE_OK && r->affected == 2, "delete 应删除 2 条");
    nodblite_result_free(r);

    /* ---------- 错误路径:非法 filter ---------- */
    r = nodblite_find(db, "users", "{bad json}", NULL, NULL, 0, 0, 1);
    CHECK(r->code == NOSQLITE_INVALID_FILTER, "非法 filter 应返回 INVALID_FILTER");
    nodblite_result_free(r);

    /* ---------- open_ex:选项结构 + struct_size 校验 ---------- */
    {
        NodbliteOpenOptions opts;
        memset(&opts, 0, sizeof(opts));
        opts.struct_size = sizeof(NodbliteOpenOptions);
        opts.create_if_missing = 1;

        /* 后缀必须是 .nodb(引擎硬性要求) */
        char ex_path[512];
        snprintf(ex_path, sizeof(ex_path), "%s_ex.nodb", path);
        NodbliteDb *db2 = nodblite_open_ex(ex_path, &opts);
        CHECK(db2 != NULL, "open_ex(struct_size 正确)应成功");
        if (db2) {
            nodblite_close(db2);
        }

        /* struct_size 过小 → 必须拒绝,不能越界读 */
        opts.struct_size = 0;
        NodbliteDb *bad = nodblite_open_ex(ex_path, &opts);
        CHECK(bad == NULL, "open_ex(struct_size=0)应失败");
        CHECK(nodblite_last_error_code() == NOSQLITE_INTERNAL,
              "open_ex 失败应给出 INTERNAL 码");

        /* flags 非 0 → 拒绝 */
        opts.struct_size = sizeof(NodbliteOpenOptions);
        opts.flags = 1;
        CHECK(nodblite_open_ex(ex_path, &opts) == NULL, "open_ex(flags!=0)应失败");

        remove(ex_path);
    }

    /* ---------- 关闭 ---------- */
    nodblite_close(db);
    free(id);

    if (failures == 0) {
        printf("C ABI smoke: ALL PASSED\n");
        return 0;
    }
    fprintf(stderr, "C ABI smoke: %d failure(s)\n", failures);
    return 1;
}
