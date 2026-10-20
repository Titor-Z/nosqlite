# 快速开始:命令行

命令行对齐 `sqlite3`:默认直接进入 REPL,**管理命令用点号,CRUD 不带点**。

## 启动

```bash
# 无参数:进入未绑定状态
nodblite

# 带路径:打开(不存在则自动创建)
nodblite app.nodb
```

## 点命令(管理)

| 命令 | 作用 |
|---|---|
| `.open <file>` | 打开数据库(不存在则创建);无参数 = 关闭当前绑定 |
| `.databases` | 显示当前绑定 |
| `.collect` | 列出全部集合 |
| `.export <集合> <文件>` | 导出为 JSON Lines |
| `.import <集合> <文件>` | 导入(JSONL 或 JSON 数组,单事务) |
| `.stats` | 显示文件与集合统计 |
| `.help` | 帮助 |
| `.quit` / `.exit` | 退出 |

## CRUD(不带点)

```text
insert <集合> <json>
find <集合> [filter] [--sort <json>] [--skip <n>] [--limit <n>] [--fields <json>]
count <集合> [filter]
update <集合> <filter> <update>
delete <集合> <filter>
drop <集合>
```

## 完整会话示例 {#session}

```text
$ nodblite app.nodb
已打开数据库: app.nodb
输入 `.help` 查看命令用法,输入 `.quit` 退出。
nodblite> insert users {"name": "张三", "age": 30, "tags": ["admin"]}
已插入文档,_id = 6ad652e28e3718a299000000
nodblite> insert users {"name": "李四", "age": 25}
已插入文档,_id = 6ad652e28e3718a299000001
nodblite> count users
集合 `users` 满足条件的文档共 2 条
nodblite> find users {"age": {"$gte": 26}} --sort {"age": -1}
找到 1 条文档:
{"_id": "6ad652e28e3718a299000000", "name": "张三", "age": 30, ...}
nodblite> .collect
users
nodblite> .quit
再见!
```

## 导入导出

```bash
# 导出(JSON Lines,每行一条)
.export users backup.jsonl

# 导入(单事务,快)
.import users backup.jsonl
```

> `.import` 支持 JSON Lines 和 JSON 数组两种格式。整批在**一个事务**内完成,失败整体回滚。

## 提示

- 用单引号包裹 JSON,避免 shell 展开 `$set`、`$gte` 之类的操作符;
- 未绑定数据库时执行 CRUD 会提示先 `.open`;
- `.nodb` 后缀强制校验,其他后缀会拒绝。
