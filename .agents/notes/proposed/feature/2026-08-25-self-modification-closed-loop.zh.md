# Agent Note: Self-modification closed loop — staged capability program

Status: proposed

English | [2026-08-25-self-modification-closed-loop.md](2026-08-25-self-modification-closed-loop.md)

## Problem

既定目标：DSH 应当知晓自身的一切崩溃与错误；了解它的用户（画像、行为、挫败感）及其项目与遇到的问题；把实时自我反思编译为具体的修改建议；在不崩溃的情况下实时修改自身组成；并通过 生成 → 测试 → 切换 → 退役 的流程发布自身的替代版本。如今只有零散的片段，且均经本次能力审计核实：模型编写的插件仅限会话作用域、重启即消失，而内建部分不可触碰；invariant 服务从未随发行组合挂载，遥测默认 DISABLED 且尽力而为的投递会在崩溃恰恰需要这些记录时丢失它们；用户反馈、审批与标题已被持久化记录，却从未跨会话关联，对搜索抽取也不可见；没有任何反思回路能把观察转化为提案；自研包的切换具备版本机制，却没有测试前置、没有自动回滚、也无法在重启后存活。

## Proposal

按五个各自独立有价值的阶段推进，顺序刻意安排为：系统先学会对自己诚实，再学会改写自己；[SELF-MODIFICATION-AUDIT.md](../../../../SELF-MODIFICATION-AUDIT.md) 持有证据与包级细节：

1. 看见崩溃 —— 在所有发行组合挂载 `dsh-invariants`；新增 `uncaughtException`/rejection 崩溃报告器并持久化崩溃记录；实现一直被推迟的遥测 outbox。
2. 关联用户真相 —— 扩展 `extractSessionEventText` 覆盖 feedback/approval/title 事件；把 message-feedback 领域接入 session-query 读模型；新增 `user-model` storage-domain，将匿名 id → workspace → 会话 → 各类信号连接起来。
3. 反思 —— 一个 retrospective 能力，在失败后消费 session-query 与遥测摘要，产出可供人审阅的提案（候选 Agent Note、preset 补丁、dynamic-package 草稿）。
4. 自我修改持久化 —— 动态包落盘存储并在启动时重放，同时记住已授予的批准；在扩大触及范围之前，先为主机侧（host-only）激活写下明确的安全立场。
5. 金丝雀切换 —— 为 runner 增加 `canary` 模式：先对候选执行断言验证，通过才晋升，并经由既有 kept-pointer 机制接上自动回滚。

## Alternatives considered

**在 harness 旁建一个单体 meta-agent。** 否决：它与 capability-seam 架构相抵触，重复实现 session-query 与遥测，并把风险集中在一个插件而非五个可审计阶段上。

**先接云分析/SaaS 管道。** 否决：耐久性与隐私优先——在任何数据离开本机之前必须先有持久化 outbox，而挫败感建模默认只在本地聚合。

**推迟到痛点爆发。** 否决：目标已被明确委托，而且没有阶段 1 的阶段 4–5 只会用更高速度放大现有盲区。

## Acceptance criteria

阶段 1：每个发行组合都挂载 invariants；中途杀死宿主会留下可读的磁盘崩溃记录；排队中的遥测能在杀死后幸存。阶段 2：`/feedback` 文本与负面评分支持跨会话检索；一条查询能给出按 workspace 聚合的问题时间线。阶段 3：失败轮次之后，生成的回顾能指名成因并落成供人审阅的具体提案。阶段 4：一个已定义的动态插件在重启后幸存且保留其授权；host-only 激活策略被成文并由组合测试强制。阶段 5：失败的候选包永不晋升并被自动回滚；通过的候选在重启后接管。

## Risks

Host-only 激活目前完全无需批准（`cordis-host-runner/src/index.ts:270-275`）——若不先落定安全立场，持久化与更广的触及会成倍放大这一暴露面。挫败感/用户建模属于敏感数据；聚合保持本地，且只派生自已持久化的事件。若批准记忆比同意本身更长寿，重启后仍存活的自我修改会成为提权通道——授权过期机制属于阶段 4 的设计内容。针对 pre-release 立场的范围压力受阶段独立性约束：每个阶段单独交付价值，随时可以暂停而不搁浅已完成的工作。
