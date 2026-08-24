# Agent Note: Composer plain-text typing echoes to the glyph layers in one task

Status: implemented

[English](2026-08-23-composer-typing-echo-one-task.md) | 中文

## Problem

Composer 把原生光标留在透明 textarea 里，而每一个可见字形都由 React 渲染的 backdrop 绘制——正是这层两层拆分让 claim token 高亮、引用 chip 与幽灵提示成为可能（[两层文本共用同一个滚动容器](2026-07-31-composer-text-layers-share-one-scrollport.md)）。这一拆分也让两侧走在不同的更新路径上：光标在按键任务内移动，backdrop 却要等到组件的下一次 React commit 才重绘。主线程繁忙时没有任何东西为这次 commit 设界——流式 transcript 渲染与它共享同一条线程——因此在活跃会话里打字时，commit 可能晚到数帧：最新输入的字形尚未可见，光标却已画在它们前方，闪烁的光标线看起来落在周围文字的后面、或插在字与字之间。单滚动容器那次的修复把光标与其字形在滚动偏移上绑死；而由 React 调度拥有的逐键间隔始终敞开着。

## Decision

只要该帧保持无装饰——按键前后都不出现 claim token、chip、词法高亮或幽灵提示——`InputBar.onChange` 就在同一事件任务内同步修补两个文本层的 Text 节点。机器先完成 dispatch，再由对 dispatch 后快照执行 `deriveDecorations` 作出判定；修补写入的字符串与随后 commit 写入的完全一致，因此协调（reconciliation）不会带来任何视觉变化。一个 ref（`plainEchoRef`）记录 DOM 当前显示的草稿，让快于 commit 的连续按键持续走快速路径；带装饰的帧回落到常规 commit 路径，结构性 DOM 变更属于那里。粘贴、撤销、剪切与 chip 操作维持既有路径不变。

## Testing

`input-bar.client.spec.tsx` 在 act 窗口内部——任何 commit 尚未运行之前——断言修补已经生效，并断言带词法高亮的帧会跳过 echo、经 commit 构建其标记范围。`.artifacts/caret-probe/` 下的 Playwright 探针测得了本决策所泛化的健康帧基线：headless 下空闲与合成拥塞时追赶滞后均为零，受信点击的光标落点精确——证明缺陷存在于真实页面的 commit 延迟而非几何形状。

## Alternatives considered

**让 textarea 自己显示字形，把装饰画在其上方。** 否决：引用 chip 会用领域图标替换首字符，这要求 textarea 自身文本保持透明；把字形所有权翻回 textarea 会破坏这层拆分本要提供的 chip 外观。

**在合成器上自绘光标。** 否决：自绘光标会移除 IME 输入所依赖的原生组字（composition）光标，而本产品用中文与英文打字的频率相当。

**压缩 transcript 渲染成本直到 commit 总是及时。** 否决：有价值的工作，但它只是缩小间隔而非闭合间隔——任何渲染预算都无法承诺 commit 落在按键自己的任务内，而这正是 echo 以构造方式提供的性质。

## Consequences

得到：对所有纯文本草稿打字，可见字形与按键落在同一任务内，光标与字形的分离不再取决于 React 调度、store 流量或主线程负载。付出：每次按键在事件路径上多一次 `deriveDecorations` 扫描；且 echo 的正确性依赖于以 dispatch 后的机器状态（而非渲染后的 DOM）校验无装饰前提——ref 弥合了修补与 commit 之间的窗口，这是重构任一层时必须一并携带的一份额外状态。
