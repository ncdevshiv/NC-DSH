# Agent Note: TypeScript 7 升级被第三方工具链阻塞（受阻，勿单独重试）

Status: proposed

[English](2026-08-25-typescript-7-third-party-blocker.md) | 中文

## 问题

TypeScript 6 → 7 是剩余升级中杠杆最高的一项——`tsc` 提速约 10 倍、原生的 typescript-go 编译器、且 `oxlint-tsgolint` 已运行在同一引擎上——但它撞上三个具体阻碍：一个没有第三方发布就无法解决，一个可用私有别名化解，一个是普通的源码修改。

## 阻碍项

### 硬阻塞：eslint-plugin-sonarjs 在 TS7 下无法加载

`ts-api-utils@2.5.0`（最新版本）被 `eslint-plugin-sonarjs@4.2.0` 加载后，在导入 `typescript` 包时无条件访问 `Type.Intrinsic`。TS7 的 npm 包只发布 `lib/getExePath.{d.ts,js}`、`lib/tsc.js` 与 `lib/version.{cjs,d.cts}`——没有 `Type` 枚举、没有 `ScriptTarget`、没有 `readConfigFile`，完全没有 Compiler API。该崩溃可从 oxlint 二进制复现：

```
Failed to parse oxlint configuration file.
  x Failed to load JS plugin: eslint-plugin-sonarjs
  |   TypeError: Cannot read properties of undefined (reading 'Intrinsic')
  |     at Object.<anonymous> (node_modules/ts-api-utils/lib/index.cjs:787:57)
  |     ...
  |     at Object.<anonymous> (node_modules/eslint-plugin-sonarjs/cjs/S6759/rule.js:62:24)
```

这会阻塞工作区内每一次 oxlint 调用，包括 lefthook pre-commit 的 `lint:contracts-ready` 门禁。`.oxlintrc.json` 在 `packages/**/*.{ts,tsx}` 块下启用了八条 `sonarjs/*` 规则（`no-duplicate-in-composite`、`no-all-duplicated-branches`、`no-identical-conditions`、`no-identical-expressions`、`no-identical-functions`、`no-duplicated-branches`、`no-duplicate-test-title`、`duplicates-in-character-class`）。`ts-api-utils` 声明的 peer 依赖是 `typescript: ">=4.8.4"`，但其运行时检查早于 TS7 桩包。只有上游发布能解决：某个处理 TS7 桩包的 `ts-api-utils` 版本、为 TS7 用户固定较旧 `ts-api-utils` 的 `eslint-plugin-sonarjs` 版本，或由 oxlint 原生规则覆盖这些已启用的 sonarjs 规则。

### 可用私有别名化解：每个 Compiler-API 驱动都需要 TS6 驱动

TS7 的 npm 包没有 Compiler API，而本仓库的自定义类型门禁直接驱动它：

- `scripts/verify-type-equiv.ts`（392 个 type-equiv 块）
- `scripts/doc-typecheck.ts`（Markdown 中 82 个围栏类型块）
- `scripts/verify-cordis-config.ts`（131 个组合文件）
- `scripts/verify-client-packages.ts`
- `scripts/publint-all.ts`
- `scripts/ts-project.ts`（约 15 个其他脚本共用的中央助手）
- `vitest.shared.ts`（测试中用于 `tsx` 风格转换的 vitest transformer）
- `packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts`
- `packages/typert/generator/src/analyzer.ts` 与 `tsdown-plugin.ts`（运行时 Typert 生成器，在模块加载时解析并转译 TypeScript）

在私有 `typescript-v6` 别名下导入旧版 `typescript@6.0.3` 并把这些文件指向它即可生效：`tsc -b` 继续使用新的 TS7 驱动，旧驱动只服务于这些特定脚本。这是已知模式——`oxlint-tsgolint` 已用其各平台预编译原生二进制以同样方式解耦。

### 更严格的类型约定在两个文件中暴露六个真实错误

TS7 重建暴露六个错误，全部真实可修：`packages/extensions/cordis-host-runner/src/index.ts` 中 4 处 `TS1361`——品牌函数 `CordisDynamicPluginId`、`CordisDynamicPackageId`、`CordisDynamicPluginRunId` 与 `ApprovalRequestId` 同经 `import type { ... } from './types.ts'` 导入的同名类型别名冲突（TS6 曾默默合并两者；TS7 拒绝；修法是把品牌函数移入 `types.ts` 并作为值与纯类型导入并列引入）；以及 `packages/client/ui-conversation/tests/views-type-chain.client.spec.tsx` 中 `TS2578` + `TS2769`——TS7 将槽位注册的组件 prop 签名与其 `inject` 回调返回值关联，在一个刻意负向的测试上给出更严格的报错（修法是把 `@ts-expect-error` 改为叠加 `@ts-ignore` 并注释说明新关联）。

## 提案

暂缓 TS7 升级。第一个阻碍在本仓库内无解：`ts-api-utils` 与 `eslint-plugin-sonarjs` 是仓库之外的第三方包，任何 fork 或本地覆盖的成本都高于收益。pre-release 的 "foundation over blast radius" 立场仍然成立，但破坏整条 lint 流水线的基础不是正确的基础。`typescript` 下限在全部七个清单位置保持 6.x：根目录、`apps/web`、`native/landlock-run`、`packages/client/web`、`packages/lsp/lsp-stdio`、`packages/session/session-persistence-sqlite` 与 `packages/typert/generator`。

当下列任一上游发布落地时重开本记录：

- `ts-api-utils` 发布处理 TS7 桩包的版本；
- `eslint-plugin-sonarjs` 发布兼容 TS7 的版本；
- oxlint 发布覆盖八条已启用 sonarjs 规则的原生规则。

任一落地之时，升级即成为大约一天的 PR，因为两个可解部分均已验证且可重复。对 Compiler-API 驱动而言，经验证的做法是在私有别名下导入旧版 TypeScript：

```sh
bun add --dev 'typescript-v6@npm:typescript@6.0.3'
# in each affected file:  import ts from 'typescript-v6'
```

重排接线覆盖 18 个脚本、1 个共享助手、2 个源码文件与 1 个测试夹具；上述六个更严格约定的修复照此重复。

## 调查证据

本次调查把 `typescript` 下限提升到七个清单位置中的 `^7.0.2`，单独确认了 cordis-host-runner 与 views-type-chain 两处修复，添加 `typescript-v6` 别名并完成上述接线，随后在本地验证：

- `bun run typecheck:contracts-ready` → 六处源码修复后 exit 0；
- `bun run verify-type-equiv` → 392 个 type-equiv 块通过；
- `bun run doc-typecheck:contracts-ready` → 82 个块编译通过，731 个 type-equiv 块跳过（另行检查），894 个配对衍生物；
- `bun run verify-cordis-config` → 131 个配置文件通过；
- `bun run verify-client-packages` → 1 处违规，属既有且与本工作无关；
- `bun run publint` → 既有仓库级 `./src/*` glob 警告；
- `bunx vitest run packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts` → 经 `typescript-v6` 驱动 2/2 通过。

随后调查撞上 sonarjs 阻碍：整条 lint 流水线无法启动，且不存在树内规避手段。升级已在工作分支上回退（`feat/typescript-7-2026-08-25`，现与 `dev` 处于同一 SHA），因此本次调查不随附任何代码变更——只有本记录本身。在当前 `.oxlintrc.json` 下对任意源码文件运行工作区 oxlint 二进制即可复现该阻塞。

## 曾考虑的替代方案

**用最接近的 oxlint 原生等价物替换八条 sonarjs 规则。** OXC 提供 `no-duplicate-in-import`、`no-duplicate-key`、`no-duplicate-string`，以及语义不同于 sonarjs 的 `no-duplicated-branches`，因此无法在不损失覆盖率的情况下 1:1 替换；即便接受部分替换，也要花一到两天在既有源码上验证等价性，且结果仍是 lint 覆盖率退化，而上游修复才是正确出路。

**将 `ts-api-utils` 与 `eslint-plugin-sonarjs` fork 为本地 vendor 包。** 被"优先维护中的依赖而非手写"规则禁止，且该 fork 需要针对上游的永久维护。

**单独把 `typescript-v6` 别名作为准备性 PR 先行落地。** 没有升级时别名毫无用处；两者要么一起，要么都不。

## 验收标准

暂缓状态保持可观察：`typescript` 下限在提案所列的全部七个清单位置读作 6.x；oxlint 流水线——包括 lefthook `lint:contracts-ready` pre-commit 门禁——在启用八条 sonarjs 规则时保持绿色；一旦提案所列的任一上游发布落地，本记录即行重开，并在升级落地前对照当时的脚本清单重新验证别名配方与六处源码修复。

## 风险

暂缓使每次开发者与 CI 类型检查运行都推迟获得约 10 倍的 `tsc` 提速。sonarjs 规则覆盖率冻结：记录保持暂缓期间，涉及那八条规则的改进无法落地。别名配方会腐化：脚本进出 Compiler-API 驱动集合时，经过验证的重接清单（18 脚本、1 助手、2 源文件、1 夹具）会过时，因此重开时需要重新审计受影响文件清单，而不是盲目重放。
