# Agent Note: Composer 加号菜单合并上传与命令启动器

Status: implemented

[English](2026-08-24-web-composer-plus-menu.md) | 中文

## 问题

Composer 工具行曾有三个并列的启动圆钮——`+` 打开命令菜单、上传图片 打开图片 picker、上传文件夹 打开文件夹 picker。三个图标承载同一个概念「向草稿添加内容」，而 web composer 的产品方向是近期 harness UI 收敛出的合并启动器模式：只保留一个可见的 `+`，由它的菜单罗列各项能力。[Composer 上传按钮](2026-08-23-web-composer-upload-buttons.md) 一文曾否决启动器菜单，理由有二：它会在 ui-input-trigger 的 `MenuView` 之外引入第二套浮层菜单，并且两个手势都要多藏一次点击。

## 决策

`+` 按钮是工具行唯一的启动器，点击打开一个由共享 ui-primitives `Menu` 构建的菜单——与同一行中 Access 控件渲染的是同一个原语。菜单共三行：上传图片、上传文件夹、一条分隔线，然后是 命令。上传两行点击的仍是原先两个隐藏 input（composer 仅有的两个 file input；accept 过滤与 `webkitdirectory` 均未改变），命令 行调用同样的 `toggleCommandMenu` 并传入 textarea 当前 selection，因此 ui-input-trigger 的 `MenuView` 仍是唯一的命令 pick 路径。每一行保留其被移除按钮原有的门槛：composer 无法接收图片时上传行禁用（`canAcceptDrop`），锁定或命令面缺席时命令行禁用，仅当没有任何行可达时启动器本身才禁用。一次选择会关闭菜单并在分发前把焦点还给 textarea——命令菜单要经 textarea 仲裁按键，OS picker 关闭后也会把焦点还给先前聚焦的元素。

当初否决的两条理由都有了答案：「第二套浮层菜单」在这里是对既有共享原语的复用而非新组件，多出的一次点击则是换取单一清晰入口的成本——粘贴与整页拖放仍是一步手势。旧文的否决仅在这一点上被取代；其 picker、intake 预检与文件夹感知拖放仍归旧文所有。

## 已考虑的替代方案

- **保留三个按钮**：三个图标承载一个概念，正是产品要求收敛的表面；给文件夹上传一个常驻图标，并不比菜单行更易发现。
- **`+` 只管附件，命令留在键入 `/`**：菜单将无法罗列该行提供的全部能力，而命令启动器本是 `+` 的既有角色。若命令菜单获得自己的专属入口可再议。
- **专门新造一个 popover**：会重复共享 `Menu` 已有的锚定（经 `side="top"` 向上展开）、外点与 Escape 关闭、禁用行与图标槽位。

## 后果

file input、intake 预检、限制与错误文案均未改动，粘贴、拖放与 picker 行为完全一致；收敛的只是入口。启动器的 tooltip 与可访问名称改用新的 `input.add` locale 键（'添加' / 'Add'），各行复用既有键，因此触发按钮朗读「添加」而非命令标签，屏幕阅读器用户经菜单行到达各项能力。`commandMenuOpen` 现在只参与排队插话仲裁，不再约束启动器外观。
