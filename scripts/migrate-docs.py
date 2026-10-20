#!/usr/bin/env python3
"""把 MkDocs Material 的 Markdown 转成 VitePress 语法。

只处理两件 MkDocs 专有语法:
  1. admonition  `!!! type "标题"` / `??? type "标题"`  →  `::: type 标题` / `::: details 标题`
  2. 图标短代码  `:material-rocket:` 等                 →  去掉(或换成文字)

其余 Markdown(表格、代码块、相对链接)两边通用,不做改动 —— 改动越少越安全。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# MkDocs admonition 类型 → VitePress 容器类型。
# VitePress 内置只有 tip / info / warning / danger / details,
# 其余映射到语义最近的内置类型。
TYPE_MAP = {
    "note": "info",
    "info": "info",
    "tip": "tip",
    "success": "tip",
    "abstract": "info",
    "question": "tip",
    "warning": "warning",
    "caution": "warning",
    "danger": "danger",
    "error": "danger",
    "bug": "danger",
    "failure": "danger",
    "example": "info",
    "quote": "info",
}

ADMONITION_RE = re.compile(r'^(?P<marker>!!!|\?\?\?)\s+(?P<kind>\w+)(?:\s+"(?P<title>[^"]*)")?\s*$')
ICON_RE = re.compile(r':material-[a-z0-9-]+:')
# MkDocs 的 attr_list 锚点 `## 标题 {#anchor}` → VitePress 的 `## 标题 {#anchor}`
# 两者语法相同,无需转换;但行内图标短代码(如 `:material-check:`)要去掉,
# 否则 VitePress 会原样输出冒号文字。
ICON_LEAD_RE = re.compile(r'^\s*:material-[a-z0-9-]+:\s*')


def convert(text: str) -> str:
    lines = text.split("\n")
    out: list[str] = []
    i = 0
    n = len(lines)

    while i < n:
        m = ADMONITION_RE.match(lines[i])
        if not m:
            out.append(lines[i])
            i += 1
            continue

        marker = m.group("marker")
        kind = m.group("kind")
        title = m.group("title")

        # `???` 是可折叠块 → details;`!!!` 是常显块 → 对应语义容器
        container = "details" if marker == "???" else TYPE_MAP.get(kind, "info")
        header = f"::: {container}" + (f" {title}" if title else "")
        out.append(header)

        # 收集紧随其后的缩进块(4 空格),去掉缩进后原样输出
        i += 1
        body: list[str] = []
        while i < n:
            line = lines[i]
            if line.strip() == "":
                body.append("")
                i += 1
                continue
            if line.startswith("    "):
                body.append(line[4:])
                i += 1
                continue
            break

        # 去掉尾部空行(容器闭合前的空行会让 VitePress 渲染出多余空隙)
        while body and body[-1] == "":
            body.pop()

        out.extend(body)
        out.append(":::")
        # 保持块后有一个空行
        if i < n and lines[i].strip() != "":
            out.append("")
        continue

    # 统一清理:行首图标短代码直接去掉,行内图标一并去掉。
    # 放在最后做,这样 admonition 内部的行也会被处理。
    result: list[str] = []
    for line in out:
        line = ICON_LEAD_RE.sub("", line)
        line = ICON_RE.sub("", line).rstrip()
        if line == "" and result and result[-1].strip() == "":
            continue
        result.append(line)

    return "\n".join(result)


def main() -> int:
    if len(sys.argv) != 3:
        print("用法: convert.py <源目录> <目标目录>", file=sys.stderr)
        return 2

    src = Path(sys.argv[1])
    dst = Path(sys.argv[2])
    count = 0

    for path in sorted(src.rglob("*.md")):
        rel = path.relative_to(src)
        target = dst / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(convert(path.read_text(encoding="utf-8")), encoding="utf-8")
        count += 1

    print(f"已转换 {count} 个文件 → {dst}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
