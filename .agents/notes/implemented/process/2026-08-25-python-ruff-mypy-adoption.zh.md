# Agent Note: ruff 与 mypy 为 Python SDK 及运行时载体提供 lint 与类型检查

Status: implemented

[English](2026-08-25-python-ruff-mypy-adoption.md) | 中文

## 问题

Python 子项目没有任何静态 lint 或类型分析，而 TypeScript 一侧运行类型感知 lint 加数十个 verify 门禁。Python 缺陷只在 pytest 阶段暴露，格式规范也不存在。

## 决策

Ruff 担任 linter 与 formatter，mypy 担任静态类型检查器，覆盖 `python/sdk` 与 `python/sdk-runtime`。配置随子项目走：`[tool.ruff]` 与 `[tool.mypy]` 两节写在各自子项目的 `pyproject.toml` 里，而非独立的共享配置文件。

规则选择为 `E/F/W`（pycodestyle/pyflakes）、`I`（isort）、`B`（bugbear）、`UP`（pyupgrade）与 `SIM`（simplify），行宽 100，target-version `py310`，双引号字符串。mypy 在公共界面（`api.py`、`models.py`）上以 strict 模式运行并启用 pydantic 插件做模型校验；在 `client.py`——其 transport、subprocess 与 threading 形态与静态类型相抵触——以及测试树上保持宽松。

lint 依赖走标准依赖组：`python/sdk` 的 `[dependency-groups].lint` 含 `ruff>=0.6.0`、`mypy>=1.11.0` 与 `types-deprecated`；`python/sdk-runtime` 只含 `ruff>=0.6.0`，它是配置胶水，没有需要类型检查的源码。

根脚本暴露工作流：`bun run python:sync`（对两个子项目按 `test+lint` 组执行 `uv sync`）、`bun run python:lint`（SDK 上执行 `ruff check` + `mypy src`，运行时载体上执行 `ruff check`）、`bun run python:format`（两者执行 `ruff format`）。`python:lint` 已加入 `hygiene` 链，CI 使其与 TypeScript 门禁并列运行。

采用过程应用了 ruff 的自动修复加三处手工修复：20 处导入顺序纠正与 8 个文件重排；移除 `tests/test_client.py:419` 处的死变量；将 `client.py:107` 的 `try/except/pass` 替换为 `contextlib.suppress(ProcessLookupError)`（`SIM105`）；以及在 `api.py` 中满足 strict mypy（`**kwargs: object` 改为 `**kwargs: Any`，为 `__exit__` 参数补齐类型）。理由与命令记录在 `python/development.md` 的 'Lint and type-check' 一节。

## 验证

`bun run python:lint` 通过：ruff 报告所有检查通过，mypy 在 5 个源文件中未发现问题。lint 后代码树上的 SDK pytest 套件：49 通过、10 跳过、3 失败——三个失败均为 `dsh-jsonrpc-agent-*` 上的 `WinError 193`，属于既有的缺失捆绑二进制问题，非本变更引入。

## 曾考虑的替代方案

**对所有模块统一 strict mypy。** 不予采用：把 strict 强加到 `client.py` 的 transport/subprocess/threading 代码与测试树上，产生的噪音会淹没 strict 本要守护的公共界面保证。

**flake8 + black + isort 组合。** 不予采用：三套工具三份配置，重复实现 ruff 用单个 Rust 核心二进制、每包一节 `[tool.ruff]` 就能提供的能力。

**Python 继续只靠运行时测试。** 不予采用：与 TypeScript 一侧门禁的平齐正是目的——缺陷应在 lint/类型检查阶段暴露，而不是只在 pytest 内部。

## 后果

Python 贡献通过与 TypeScript 工作相同的 `hygiene` 链获得 lint、format 与 strict 公共界面类型反馈。宽松区（`client.py`、测试树）是刻意划出的留白，新的 transport 形态代码只有具备理由才应落在那里。pytest 的 3 失败基线仍归捆绑二进制问题所有，与 lint 无关。
