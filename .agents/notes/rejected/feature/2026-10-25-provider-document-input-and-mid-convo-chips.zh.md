# Agent Note: Provider document input and mid-conversation capability controls

Status: rejected — no provider behind this harness accepts document blocks, and the mid-conversation switches are withheld by design, so the controls would claim requests the transport cannot send.

[English](2026-10-25-provider-document-input-and-mid-convo-chips.md) | 中文

## Problem

对照竞品所做的 Settings → Models 一致性审查列出本页缺少的四项模型元数据控件：文档（`pdf`）输入、视频/音频输入、原生网页搜索标记，以及会话中改写系统提示标记。每一项都是对某个声明的勾选，而 harness 必须在该模型构造的每一个请求中兑现该声明；因此未经证实的声明不是外观差距：提供商会在轮次中途、消息已持久化之后拒绝请求，Session 随后不断重复一个不可能成功的请求。

文档其实已有可用路径。`FileBlock` 会持久化并记入日志，请求组装把每次出现投影为确定性的句柄文本，指明其只读的已保存路径，模型随后用文件系统工具打开它（[llm 类型](../../../../packages/llm/llm/src/types.ts)）。office-to-pdf 能力接缝是为文档*预览*转换字节，而非为模型输入。

## Proposal

通过 DeepSeek Files API 上传文档，将其序列化为引用 `file_id` 的 Anthropic Messages `document` 块，在模态词汇中声明 `pdf`，并按模型提供竞品的那些能力标记控件。

## Alternatives considered

**经由现有 Files 路径支持文档输入。** 依据提供商事实予以拒绝：DeepSeek 的 [Anthropic 兼容性页面](https://api-docs.deepseek.com/guides/anthropic_api) 支持 `text`、`image`、`thinking`、`tool_use`、`tool_result`、`server_tool_use`、`web_search`，且不支持 `document`。Files 存储在构造上仅限图像——`ImageMediaType`、png/jpeg/webp/gif 扩展名映射以及逐图字节上限——因此它的 `file_id` 没有可引用的对象。这是用户原先选择的方案，也正是本记录推翻的结论。

**在我们自己的层里拓宽模态词汇。** 予以拒绝：`PiAiModality` 即 `Model<Api>['input'][number]`，而 `MODALITY_GATE` 将其作为编译期漂移检查钉在 `text | image`；所安装的提供商库完全不带 `document` 或 `application/pdf` 序列化器。为一个传输层无法发出的模态命名，正是一致性审查禁止的假象。

**按模型暴露会话中系统提示与工具更新开关。** 予以拒绝：`COMPAT_GATES` 将 `supportsMidConvoSystemMessages`、`supportsMidConvoToolChanges`、`supportsMidConvoToolAdditions`、`supportsAdditionalTools`、`supportsToolSearch` 标记为 `withhold`，且 `assertOfferedCompatFields` 会拒绝它们，理由是 pi-ai 已安装目录会为确实需要的厂商设置它们。`conversationUpdates()` 读取这些标志以决定提示准入与工具历史投影，因此用户自行设置的声明会为一个可能并不兑现的模型改变缓存前缀复用。

**改为暴露 schema 已经提供的能力开关。** 在 UI 需要能力控件时选择此路：`supportsStrictTools`、`supportsStrictMode`、`supportsTemperature` 等已提供的字段确实进入请求组装，无需新增词汇即可呈现。

## Consequences

文档与媒体输入维持现状：只有文本与图像进入提供商，其他所有文件经由只读路径与工具送达。一致性清单减少四行，诚实可用的剩余行是已提供的 compat 集合。若要重新引入 `pdf`，需要一个 Messages 实现确实接受 `document` 的提供商，外加一个能命名它的上游模态；再次动工之前请先核实这两点。
