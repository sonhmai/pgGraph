#!/usr/bin/env python3
"""Check documentation references to local files and directories.

Usage:
  python3 scripts/check_doc_references.py

What this checks:
  1. Relative Markdown links in docs/**/*.mdx point at existing local files or
     directories. External URLs, mailto links, and pure anchors are ignored.
  2. Inline-code path references such as `graph/src/lib.rs`, `scripts/foo.py`,
     `docs/user_guide/index.mdx`, and `src/sql_facade/` point at existing
     repository paths.

The path scanner is intentionally conservative. It skips examples containing
shell variables, globs, placeholders, SQL calls, or ellipses so command snippets
do not create noisy false positives.
"""

from __future__ import annotations

import pathlib
import re
import sys
from urllib.parse import unquote

ROOT = pathlib.Path(__file__).resolve().parents[1]
DOCS_DIR = ROOT / "docs"

MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
INLINE_CODE = re.compile(r"`([^`]+)`")
PATH_PREFIXES = ("docs/", "graph/", "scripts/", "src/", "tests/", "fuzz/", "benches/")
SKIP_CHARS = ("$", "*", "<", ">", "...", "::", "(", ")")


def is_external_link(target: str) -> bool:
    return bool(re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target))


def strip_anchor_and_query(target: str) -> str:
    return target.split("#", 1)[0].split("?", 1)[0]


def resolve_doc_link(doc: pathlib.Path, target: str) -> pathlib.Path | None:
    target = strip_anchor_and_query(unquote(target.strip()))
    if not target or is_external_link(target):
        return None
    if target.startswith("/"):
        return resolve_mdx_route(DOCS_DIR / target.lstrip("/"))
    return resolve_mdx_route((doc.parent / target).resolve())


def resolve_mdx_route(path: pathlib.Path) -> pathlib.Path:
    """Resolve Nextra-style extensionless routes before existence checks."""
    if path.exists():
        return path
    if path.suffix == "":
        mdx = path.with_suffix(".mdx")
        if mdx.exists():
            return mdx
        index = path / "index.mdx"
        if index.exists():
            return index
    return path


def resolve_inline_path(raw: str) -> pathlib.Path | None:
    value = raw.strip().strip(".,;:")
    if not value.startswith(PATH_PREFIXES):
        return None
    if any(skip in value for skip in SKIP_CHARS):
        return None

    if value.startswith("src/"):
        return ROOT / "graph" / value
    if value.startswith(("tests/", "fuzz/", "benches/")):
        return ROOT / "graph" / value
    return ROOT / value


def main() -> int:
    failures: list[str] = []

    for doc in DOCS_DIR.rglob("*.mdx"):
        text = doc.read_text()
        for line_no, line in enumerate(text.splitlines(), start=1):
            for match in MARKDOWN_LINK.finditer(line):
                target = resolve_doc_link(doc, match.group(1))
                if target is not None and not target.exists():
                    failures.append(
                        f"{doc.relative_to(ROOT)}:{line_no}: missing Markdown link target {match.group(1)!r}"
                    )

            for match in INLINE_CODE.finditer(line):
                target = resolve_inline_path(match.group(1))
                if target is not None and not target.exists():
                    failures.append(
                        f"{doc.relative_to(ROOT)}:{line_no}: missing inline path {match.group(1)!r}"
                    )

    if failures:
        print("Documentation reference drift:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1

    print("Documentation local references are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
