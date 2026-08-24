# Agent Note: Composer 上传按钮与文件夹感知拖放

Status: implemented

[English](2026-08-23-web-composer-upload-buttons.md) | 中文

## 问题

composer 此前完全没有进入附件 intake 的点击路径：前置加号按钮打开的是 slash 命令菜单（其既定职责），图片只能经剪贴板粘贴或整页拖放进入。用户想点 "+" 传文件时一无所获；拖入文件夹也毫无结果，因为直接读 `dataTransfer.files` 看不到目录内部。

## 决策

InputBar 工具行在加号旁新增两个并列启动圆钮：上传图片 打开隐藏的 `multiple` `<input type="file">`，其 accept 过滤镜像投影的媒体类型（无服务投影时用固定 v1 图片列表）；上传文件夹 打开隐藏的 `webkitdirectory` picker，经 ref 设置属性（React 没有对应 prop）。二者都走既有 `intakeImages` 预检——选中批次遵循与粘贴完全一致的规则：整批拒收、格式横幅优先、即时产品文案——且每次 change 后清空 value，同一位置可重复选择。两个按钮与拖放目标共用同一禁用门槛（locked、machine-busy 或无附件服务）。

整页拖放现在会向下深入：ui-attachment 的 `filesFromDataTransfer` 同步读取每个 item 的 entry（entry 句柄在事件轮次结束后失效），经分页 `readEntries` 走目录直到空页，按已访问 entry 身份防环，跳过不可读文件与出错子目录但保留兄弟分支，并在没有任何 item 暴露 entry、或遍历一无所获时回退到扁平 files 列表。blocked-drop 检查保持在遍历之前。

两个启动圆钮现为加号菜单中的菜单行（[Composer 加号菜单](2026-08-24-web-composer-plus-menu.md)）；不引入上传协议、wire 字段或新菜单组件，附件仍是仅图片的 v1。

## 已考虑的替代方案

- **把 "+" 改为 launcher 菜单**（Command/Upload 两行）：需要在 ui-input-trigger 的 `MenuView` 之外引入第二套浮层菜单，违反 composer 的既定决策，且两个手势都多付一次点击。[Composer 加号菜单](2026-08-24-web-composer-plus-menu.md) 已在两点上取代本条。
- **File System Access API（`showDirectoryPicker`）**：仅 Chromium 支持；`webkitdirectory` 用一个 input 覆盖 Chromium、Firefox、Safari。
- **在选择阶段于客户端过滤非图片**：静默丢文件与既定的整批拒收语义矛盾；格式横幅点名问题才是正路。
- **现在就把附件 seam 扩展到文档**：牵动宿主 admission、wire envelope、模型可见日志与提供方能力——这是 capability-seam 变更而非手势缺口；等文档支持真正被需要再做。

## 后果

文件夹上传由 admission 做图片过滤而非 picker：选到混合内容的文件夹会以格式横幅整批拒绝——诚实但对大型混合目录很吵。`webkitdirectory` 没有逐文件的对话框反馈，部分引擎完全忽略 `accept`，因此图片栏仍是确认面。遍历依赖 entry 对象仅在拖放事件轮次内有效，这正是「先同步收集」的原因。非图片文件卡片与上传进度仍延后（#2248），记录在 ui-attachment README limitations。这些手势汇入的 intake 预检由 [Web 图片摄入与限制对齐](2026-08-12-web-image-intake-and-limits-alignment.md) 负责。
