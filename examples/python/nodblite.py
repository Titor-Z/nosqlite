"""nodblite C ABI 的 Python 绑定示例(仅用标准库 ctypes)。

用法:
    cargo build --release
    python3 examples/python/nodblite.py target/release/libnodblite.dylib

这段代码同时也是「任何语言都能进程内嵌入」的实证:
没有第三方包、没有服务进程,直接 dlopen 动态库即可。
"""

import ctypes
import json
import os
import tempfile


class Slice(ctypes.Structure):
    """对应 C 侧 NodbliteSlice。"""

    _fields_ = [("ptr", ctypes.c_char_p), ("len", ctypes.c_size_t)]


class Result(ctypes.Structure):
    """对应 C 侧 NodbliteResult。"""

    _fields_ = [
        ("code", ctypes.c_int32),
        ("count", ctypes.c_uint64),
        ("affected", ctypes.c_uint64),
        ("docs", ctypes.POINTER(Slice)),
        ("value", Slice),
    ]


class Error(ctypes.Structure):
    """对应 C 侧 NodbliteError。"""

    _fields_ = [
        ("code", ctypes.c_int32),
        ("code_str", Slice),
        ("message", Slice),
        ("op", Slice),
    ]


class Nodblite:
    """极简封装:演示正确的 argtypes/restype 声明与内存释放。"""

    def __init__(self, lib_path: str):
        self.lib = ctypes.CDLL(lib_path)
        L = self.lib

        # ---- 关键:句柄必须声明为 c_void_p ----
        # 漏掉 argtypes 时 ctypes 会按 int 传参,64 位指针被截断 → 段错误
        L.nodblite_open.restype = ctypes.c_void_p
        L.nodblite_open.argtypes = [ctypes.c_char_p, ctypes.c_uint8]
        L.nodblite_close.argtypes = [ctypes.c_void_p]

        for name in (
            "nodblite_insert",
            "nodblite_insert_many",
            "nodblite_get",
            "nodblite_count",
            "nodblite_find",
            "nodblite_update",
            "nodblite_upsert",
            "nodblite_delete",
            "nodblite_exec",
            "nodblite_list_collections",
        ):
            fn = getattr(L, name)
            fn.restype = ctypes.POINTER(Result)
            # 统一声明:首参句柄,其余 c_char_p / 整数,逐个覆盖见下
            fn.argtypes = [ctypes.c_void_p, ctypes.c_char_p]

        L.nodblite_get.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
        L.nodblite_count.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
        L.nodblite_delete.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
        L.nodblite_exec.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        L.nodblite_update.argtypes = [
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
        ]
        L.nodblite_upsert.argtypes = [
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
        ]
        L.nodblite_find.argtypes = [
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_uint64,
            ctypes.c_uint64,
            ctypes.c_uint8,
        ]

        L.nodblite_result_free.argtypes = [ctypes.POINTER(Result)]
        L.nodblite_last_error.restype = ctypes.POINTER(Error)
        L.nodblite_last_error_code.restype = ctypes.c_int32

    # ---------- 工具 ----------

    @staticmethod
    def _text(slice_: Slice) -> str:
        return slice_.ptr.decode("utf-8") if slice_.ptr else ""

    def _check(self, result):
        """结果非 0 码时抛异常,并附上结构化错误信息。"""
        code = result.contents.code
        if code == 0:
            return result
        err = self.lib.nodblite_last_error()
        e = err.contents
        msg = self._text(e.message) if err else "(无详细信息)"
        op = self._text(e.op) if err else "?"
        self.lib.nodblite_result_free(result)
        raise RuntimeError(f"[{code}] {op}: {msg}")

    # ---------- API ----------

    def open(self, path: str, create_if_missing: bool = True):
        handle = self.lib.nodblite_open(path.encode(), 1 if create_if_missing else 0)
        if not handle:
            code = self.lib.nodblite_last_error_code()
            raise RuntimeError(f"打开失败,错误码 {code}")
        return handle

    def close(self, db):
        self.lib.nodblite_close(db)

    def insert(self, db, collection: str, doc: dict) -> str:
        r = self._check(
            self.lib.nodblite_insert(db, collection.encode(), json.dumps(doc).encode())
        )
        out = self._text(r.contents.value)
        self.lib.nodblite_result_free(r)
        return out

    def get(self, db, collection: str, doc_id: str):
        r = self._check(self.lib.nodblite_get(db, collection.encode(), doc_id.encode()))
        text = self._text(r.contents.value)
        self.lib.nodblite_result_free(r)
        return json.loads(text) if text else None

    def find(
        self,
        db,
        collection: str,
        filter_: dict | None = None,
        sort: dict | None = None,
        fields: dict | None = None,
        skip: int = 0,
        limit: int | None = None,
    ) -> list:
        r = self._check(
            self.lib.nodblite_find(
                db,
                collection.encode(),
                json.dumps(filter_).encode() if filter_ is not None else None,
                json.dumps(sort).encode() if sort is not None else None,
                json.dumps(fields).encode() if fields is not None else None,
                skip,
                limit or 0,
                0 if limit is not None else 1,
            )
        )
        count = r.contents.count
        docs = [json.loads(self._text(r.contents.docs[i])) for i in range(count)]
        self.lib.nodblite_result_free(r)
        return docs

    def update(self, db, collection: str, filter_: dict, update_: dict) -> int:
        r = self._check(
            self.lib.nodblite_update(
                db,
                collection.encode(),
                json.dumps(filter_).encode(),
                json.dumps(update_).encode(),
            )
        )
        n = r.contents.affected
        self.lib.nodblite_result_free(r)
        return n

    def upsert(self, db, collection: str, filter_: dict, update_: dict) -> dict:
        r = self._check(
            self.lib.nodblite_upsert(
                db,
                collection.encode(),
                json.dumps(filter_).encode(),
                json.dumps(update_).encode(),
            )
        )
        text = self._text(r.contents.value)
        self.lib.nodblite_result_free(r)
        return json.loads(text)

    def delete(self, db, collection: str, filter_: dict) -> int:
        r = self._check(
            self.lib.nodblite_delete(
                db, collection.encode(), json.dumps(filter_).encode()
            )
        )
        n = r.contents.affected
        self.lib.nodblite_result_free(r)
        return n

    def count(self, db, collection: str, filter_: dict | None = None) -> int:
        r = self._check(
            self.lib.nodblite_count(
                db,
                collection.encode(),
                json.dumps(filter_).encode() if filter_ is not None else None,
            )
        )
        n = r.contents.affected
        self.lib.nodblite_result_free(r)
        return n


def main(lib_path: str) -> None:
    db_path = os.path.join(tempfile.gettempdir(), f"nodblite_demo_{os.getpid()}.nodb")
    if os.path.exists(db_path):
        os.remove(db_path)

    api = Nodblite(lib_path)
    db = api.open(db_path)

    uid = api.insert(db, "users", {"name": "张三", "age": 30, "tags": ["rust"]})
    print("insert ->", uid)

    api.insert(db, "users", {"name": "李四", "age": 20, "tags": ["go"]})

    # $inc + $addToSet(原子更新,无需读-改-写)
    print(
        "update ->",
        api.update(
            db,
            "users",
            {"name": "张三"},
            {"$inc": {"age": 1}, "$addToSet": {"tags": "db"}},
        ),
    )

    # Date 使用 Extended JSON
    print(
        "upsert ->",
        api.upsert(
            db,
            "events",
            {"name": "发布"},
            {"$set": {"at": {"$date": "2026-09-16T10:00:00Z"}}},
        ),
    )

    print("find ->", api.find(db, "users", {"age": {"$gte": 20}}, {"age": -1}, {"name": 1}))
    print("count ->", api.count(db, "users"))
    print("get ->", api.get(db, "users", uid))
    print("delete ->", api.delete(db, "users", {"age": {"$lt": 25}}))

    api.close(db)
    os.remove(db_path)
    print("Python ctypes 示例: 全部通过")


if __name__ == "__main__":
    import sys

    main(sys.argv[1] if len(sys.argv) > 1 else "target/release/libnodblite.dylib")
