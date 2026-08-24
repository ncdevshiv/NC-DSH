# Agent Note: Spectrum port foundation: Provider seam, DeepSeek theme, Button

Status: proposed

[English](2026-08-22-spectrum-port-foundation.md) | 中文

## 问题

Web 客户端的交互原子需要焦点、按压与键盘语义，以及一套有人维护的视觉系统；而 [2026-07-19-web-styling-system](../../implemented/process/2026-07-19-web-styling-system.md) 中“不引入组件库”的裁决已无法满足这一需求。移植不能破坏按现有 `ui-primitives` API 编译的消费方包，且必须保持仅用 token（`--dsw-*`）的主题机制作为唯一颜色权威。

## 提案

通过包装接缝把 Web 客户端移植到 Adobe React Spectrum（`@adobe/react-spectrum` ^3.47.2）：`ui-primitives` 保持其导出的组件 API（包括 `onClick` 等原生形态的 props），内部改渲染 Spectrum 组件；消费方包继续照常编译，随后逐包迁移到原生 Spectrum props。

[2026-07-19-web-styling-system](../../implemented/process/2026-07-19-web-styling-system.md) 中不变的部分：所有自定义样式仍用 CSS Modules + clsx、颜色仅用 token、`ui-theme/src/styles/` 中的 `--dsw-*` 样式表仍是唯一颜色权威。

## 结构

- **主题**：`ui-primitives/src/spectrum/theme.ts` 通过展开原版 `defaultTheme` 构造 `deepseekTheme`，并向两个 scheme 槽各加入一个覆盖类（`spectrum-vars.module.css`）。每个覆盖值都是 `var(--dsw-*)` 引用，明暗切换依旧经由 `body[data-ds-dark-theme]`；两个槽刻意携带完全相同的覆盖，使 Provider 自身的 scheme 类在视觉上不起作用。
- **Provider 挂载**：`SpectrumSurface`（`ui-primitives/src/spectrum/Surface.tsx`）包裹 ui-layout 的 AppFrame 注册——根处一个 `<Provider>`，放在静态通道里，动态 bundle 因此不经过模块表请求 `@adobe/react-spectrum`。特性包只经 primitives 原子消费 Spectrum；若将来改变，再把该说明符加入 `PLATFORM_MODULES`。
- **Button**：基于 `@react-aria/button` 的 `useButton` 重建而非使用带样式的 Spectrum 组件，使每个原生属性与处理器原样透传，DOM 形态与移植前契约一致。
- **测试泳道**：jsdom 套件（`*.client.spec.*`）移入专用的 `client-dom` 项目，运行 `pool: 'vmForks'`。只有 vm 池能拦截外部模块 CSS；普通 forks 下任何导入 primitives barrel 的套件都会死于 Spectrum 的副作用 `.css` 导入（ERR_UNKNOWN_FILE_EXTENSION），而 Vitest 4.1.8 的内联模式在 Windows 路径上匹配不到。重定义 `document`/`window` 的 Node 半区套件（modules loader、locale boot）留在 forks 项目——VM 上下文的全局不可配置。
- **顺延移植顺序**：Input（TextField 需要标签/错误接线；其 onChange(value) 契约波及 42 处调用点，值得单独一轮）、Menu→ActionMenu+MenuTrigger、Modal→Dialog+DialogTrigger、Tooltip、Toast→toast 队列。内容渲染器（ANSI 终端、diff、JSON 树、markdown）永久保持自研——Spectrum 没有等价 DOM。

## 已考虑的替代方案

**维持自研无样式 primitives。** 旧裁决要求的现状；落败因为每个原子都在重复实现并重复测试 Spectrum 上游已维护的焦点、按压、禁用与键盘行为，重复的代价已经体现在 tooltip 与属性透传缺口上。

**react-aria-components 无样式形态。** 在与 Spectrum 的显式利弊评审中落败：它们只提供行为不给视觉，所有视觉决策仍留在仓库内——这正是本次移植要甩掉的成本。

**带样式的 `SpectrumButton` 组件。** Button 弃用该组件：它丢弃原生属性（已验证 `title` 到不了 DOM）并给 ref 一个焦点句柄而非元素本身，破坏了各消费方套件的 tooltip 契约；`useButton` hook 提供按压语义、禁用处理、焦点接线与键盘激活，元素归本包所有。

## 验收标准

开发所用的镜像源 Windows 主机上 `bun install`、全仓 typecheck 与 `bun run test:gui` 通过。合并前，`DSH_SNAPSHOT=replay bun run test:web` 与浏览器 GIF 流程须在具备 `.git` 的正常 checkout 上对着构建产物运行——构建步骤嵌入 git 提交哈希，缺 `.git` 处无法执行。

## 风险

bundle 因 react-aria 层增大；它搭在 index chunk 上且导入 react/jsx-runtime，绝不可加入 VENDOR_PACKAGES。视觉一致性在 replay 运行前未经证实——预计还需为本次覆盖集未触及的 spectrum-css-temp 变量做后续 token 映射修复。本片仅移植 Button；每个顺延原子都自带调用点迁移成本，Input 最大。
