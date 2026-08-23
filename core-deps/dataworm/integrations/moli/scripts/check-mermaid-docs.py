#!/usr/bin/env python3
"""Validate Mermaid diagrams embedded in Markdown files.

The script always performs lightweight static checks that catch common reasons
Mermaid diagrams fail to render. If the Mermaid CLI (`mmdc`) is available on
PATH, it also renders each block to a temporary SVG so Mermaid's own parser is
used as the final authority.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


DEFAULT_DOC = Path("docs/chromium-module-tree-optimization-audit-2026-06-26.md")


@dataclass(frozen=True)
class MermaidBlock:
    index: int
    start_line: int
    end_line: int
    body: str

    @property
    def first_line(self) -> str:
        for line in self.body.splitlines():
            if line.strip():
                return line.strip()
        return "<empty>"


@dataclass(frozen=True)
class Diagnostic:
    level: str
    path: Path
    block_index: int
    line: int
    message: str

    def format(self) -> str:
        return (
            f"{self.path}:{self.line}: {self.level}: "
            f"mermaid block {self.block_index:02d}: {self.message}"
        )


def extract_blocks(path: Path) -> tuple[list[MermaidBlock], list[Diagnostic]]:
    diagnostics: list[Diagnostic] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    blocks: list[MermaidBlock] = []
    inside = False
    start_line = 0
    body: list[str] = []
    for line_no, line in enumerate(lines, 1):
        if not inside and line.strip() == "```mermaid":
            inside = True
            start_line = line_no + 1
            body = []
            continue
        if inside and line.strip() == "```":
            blocks.append(
                MermaidBlock(
                    index=len(blocks) + 1,
                    start_line=start_line,
                    end_line=line_no - 1,
                    body="\n".join(body),
                )
            )
            inside = False
            continue
        if inside:
            body.append(line)
    if inside:
        diagnostics.append(
            Diagnostic(
                "ERROR",
                path,
                len(blocks) + 1,
                start_line - 1,
                "unclosed Mermaid code fence",
            )
        )
    return blocks, diagnostics


ALLOWED_HEADERS = {
    "flowchart",
    "graph",
    "sequenceDiagram",
    "stateDiagram-v2",
    "classDiagram",
}
NODE_DEF_RE = re.compile(r"([A-Za-z][A-Za-z0-9_]*)\s*(?:\[|\(|\{|>|\[\[|\(\()")
PARTICIPANT_RE = re.compile(
    r"^\s*(?:participant|actor)\s+([A-Za-z][A-Za-z0-9_]*)\b(?:\s+as\s+(.+?))?\s*$"
)
SEQ_ARROW_RE = re.compile(
    r"^\s*([A-Za-z][A-Za-z0-9_]*)\s*(?:-+>>|-->>|->>|-->|->|-)\+?\s*"
    r"([A-Za-z][A-Za-z0-9_]*)\s*:"
)
STATE_TRANSITION_RE = re.compile(
    r"^\s*(\[\*\]|[A-Za-z][A-Za-z0-9_]*)\s*--?>\s*"
    r"(\[\*\]|[A-Za-z][A-Za-z0-9_]*)"
)
STATE_DECL_RE = re.compile(r'^\s*state\s+"[^"]+"\s+as\s+([A-Za-z][A-Za-z0-9_]*)\s*$')
CLASS_DECL_RE = re.compile(r"^\s*class\s+([A-Za-z][A-Za-z0-9_]*)\b")
CLASS_REL_RE = re.compile(
    r"^\s*([A-Za-z][A-Za-z0-9_]*)\s+"
    r"(?:<\|--|--\|>|\*--|o--|-->|<--|\.\.>|<\.\.)\s+"
    r"([A-Za-z][A-Za-z0-9_]*)"
)


def strip_label_strings(line: str) -> str:
    return re.sub(r'"[^"]*"', '""', line)


def static_check(path: Path, block: MermaidBlock) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    numbered_lines = [
        (block.start_line + offset, line)
        for offset, line in enumerate(block.body.splitlines())
        if line.strip()
    ]
    if not numbered_lines:
        return [
            Diagnostic("ERROR", path, block.index, block.start_line, "empty Mermaid block")
        ]

    header_line_no, header_line = numbered_lines[0]
    header = header_line.strip().split()[0]
    if header not in ALLOWED_HEADERS:
        diagnostics.append(
            Diagnostic(
                "ERROR",
                path,
                block.index,
                header_line_no,
                f"unsupported or unknown Mermaid header `{header_line.strip()}`",
            )
        )

    for line_no, line in numbered_lines:
        stripped = line.strip()
        if stripped.startswith("%%"):
            continue
        if line.count('"') % 2:
            diagnostics.append(
                Diagnostic("ERROR", path, block.index, line_no, "odd number of double quotes")
            )
        if "`" in line:
            diagnostics.append(
                Diagnostic(
                    "WARNING",
                    path,
                    block.index,
                    line_no,
                    "contains backticks; some Mermaid renderers fail on markdown labels",
                )
            )

    if header in {"flowchart", "graph"}:
        diagnostics.extend(check_flowchart(path, block, numbered_lines[1:]))
    elif header == "sequenceDiagram":
        diagnostics.extend(check_sequence(path, block, numbered_lines[1:]))
    elif header == "stateDiagram-v2":
        diagnostics.extend(check_state(path, block, numbered_lines[1:]))
    elif header == "classDiagram":
        diagnostics.extend(check_class(path, block, numbered_lines[1:]))
    return diagnostics


def check_flowchart(
    path: Path, block: MermaidBlock, lines: list[tuple[int, str]]
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    declared: set[str] = set()
    used: set[str] = set()
    edge_re = re.compile(
        r"\b([A-Za-z][A-Za-z0-9_]*)\b\s*(?:-->|---|-.->|==>|--x|--o)\s*"
        r"\b([A-Za-z][A-Za-z0-9_]*)\b"
    )
    for line_no, line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("%%"):
            continue
        for match in NODE_DEF_RE.finditer(line):
            declared.add(match.group(1))
        for match in edge_re.finditer(strip_label_strings(line)):
            used.update(match.groups())
        if re.search(r"(?:-->|---|-.->|==>|--x|--o)\s*$", stripped):
            diagnostics.append(
                Diagnostic("ERROR", path, block.index, line_no, "flowchart edge has no target")
            )
    for name in sorted(used - declared):
        diagnostics.append(
            Diagnostic(
                "WARNING",
                path,
                block.index,
                block.start_line,
                f"flowchart node `{name}` is used implicitly",
            )
        )
    return diagnostics


def check_sequence(
    path: Path, block: MermaidBlock, lines: list[tuple[int, str]]
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    declared: set[str] = set()
    used: set[str] = set()
    for line_no, line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("%%"):
            continue
        match = PARTICIPANT_RE.match(line)
        if match:
            declared.add(match.group(1))
            alias = match.group(2)
            if alias and not (
                (alias.startswith('"') and alias.endswith('"'))
                or (alias.startswith("'") and alias.endswith("'"))
            ):
                alias_tokens = alias.split()
                if len(alias_tokens) > 1:
                    diagnostics.append(
                        Diagnostic(
                            "ERROR",
                            path,
                            block.index,
                            line_no,
                            "sequence participant alias with spaces must be quoted "
                            "or converted to a single token; some Mermaid renderers "
                            "merge the next message line into the participant",
                        )
                    )
        match = SEQ_ARROW_RE.match(line)
        if match:
            used.update(match.groups())
        elif re.search(r"-{1,2}>>|-->|->", stripped) and ":" not in stripped:
            diagnostics.append(
                Diagnostic(
                    "ERROR",
                    path,
                    block.index,
                    line_no,
                    "sequence message arrow is missing a colon label",
                )
            )
    for name in sorted(used - declared):
        diagnostics.append(
            Diagnostic(
                "WARNING",
                path,
                block.index,
                block.start_line,
                f"sequence participant `{name}` is used before explicit declaration",
            )
        )
    return diagnostics


def check_state(
    path: Path, block: MermaidBlock, lines: list[tuple[int, str]]
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    declared: set[str] = set()
    used: set[str] = set()
    for line_no, line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("%%"):
            continue
        match = STATE_DECL_RE.match(line)
        if match:
            declared.add(match.group(1))
        match = STATE_TRANSITION_RE.match(line)
        if match:
            used.update(name for name in match.groups() if name != "[*]")
        elif "-->" in stripped:
            diagnostics.append(
                Diagnostic(
                    "ERROR",
                    path,
                    block.index,
                    line_no,
                    "state transition endpoint must be an identifier or [*]; "
                    'use `state "Label" as Id` for labels',
                )
            )
    for name in sorted(used - declared):
        diagnostics.append(
            Diagnostic(
                "WARNING",
                path,
                block.index,
                block.start_line,
                f"state `{name}` is used without explicit `state ... as ...` declaration",
            )
        )
    return diagnostics


def check_class(
    path: Path, block: MermaidBlock, lines: list[tuple[int, str]]
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    declared: set[str] = set()
    used: set[str] = set()
    for _, line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("%%"):
            continue
        match = CLASS_DECL_RE.match(line)
        if match:
            declared.add(match.group(1))
        match = CLASS_REL_RE.match(line)
        if match:
            used.update(match.groups())
    for name in sorted(used - declared):
        diagnostics.append(
            Diagnostic(
                "WARNING",
                path,
                block.index,
                block.start_line,
                f"class `{name}` is used without explicit class declaration",
            )
        )
    return diagnostics


def render_with_mmdc(path: Path, block: MermaidBlock, mmdc: str) -> list[Diagnostic]:
    with tempfile.TemporaryDirectory(prefix="mermaid-check-") as tmp:
        tmp_dir = Path(tmp)
        input_path = tmp_dir / f"block-{block.index:02d}.mmd"
        output_path = tmp_dir / f"block-{block.index:02d}.svg"
        input_path.write_text(block.body + "\n", encoding="utf-8")
        result = subprocess.run(
            [mmdc, "-i", str(input_path), "-o", str(output_path), "-q"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode == 0 and output_path.exists():
            return []
        message = (result.stderr or result.stdout or "mmdc failed without output").strip()
        return [
            Diagnostic(
                "ERROR",
                path,
                block.index,
                block.start_line,
                "Mermaid CLI render failed: " + message.replace("\n", " | "),
            )
        ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "paths",
        nargs="*",
        type=Path,
        default=[DEFAULT_DOC],
        help="Markdown files to validate",
    )
    parser.add_argument(
        "--require-mmdc",
        action="store_true",
        help="fail if Mermaid CLI (`mmdc`) is not available",
    )
    args = parser.parse_args()

    mmdc = shutil.which("mmdc")
    if args.require_mmdc and not mmdc:
        print("ERROR: `mmdc` was not found on PATH", file=sys.stderr)
        return 1

    all_diagnostics: list[Diagnostic] = []
    total_blocks = 0
    for path in args.paths:
        blocks, diagnostics = extract_blocks(path)
        all_diagnostics.extend(diagnostics)
        total_blocks += len(blocks)
        print(f"{path}: found {len(blocks)} Mermaid block(s)")
        for block in blocks:
            print(
                f"  block {block.index:02d}: "
                f"lines {block.start_line}-{block.end_line}: {block.first_line}"
            )
            all_diagnostics.extend(static_check(path, block))
            if mmdc:
                all_diagnostics.extend(render_with_mmdc(path, block, mmdc))

    if not mmdc:
        print(
            "NOTE: `mmdc` not found; only static Mermaid checks were run. "
            "Install @mermaid-js/mermaid-cli or pass --require-mmdc in CI.",
            file=sys.stderr,
        )

    errors = [diag for diag in all_diagnostics if diag.level == "ERROR"]
    warnings = [diag for diag in all_diagnostics if diag.level == "WARNING"]
    for diag in errors + warnings:
        print(diag.format(), file=sys.stderr)

    if errors:
        print(f"FAILED: {len(errors)} error(s), {len(warnings)} warning(s)", file=sys.stderr)
        return 1
    print(f"OK: checked {total_blocks} Mermaid block(s), {len(warnings)} warning(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
