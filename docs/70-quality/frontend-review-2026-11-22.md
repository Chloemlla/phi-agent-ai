# 前端 UI/UX 与工程审查（2026-11-22）

> 状态：🌿 评审记录
> 日期：2026-11-22
> 范围：`src/` SolidJS 外壳、聊天界面、设置页、inline UI、样式、i18n、测试与构建
> 性质：只记录发现与建议，不修改实现
> 参照：`docs/design/SPEC.md`、`docs/60-frontend/multi-frontend.md`、`docs/70-quality/frontend-review-2026-11-20.md`、`AGENT.md`

本次为静态代码审查 + 工具验证；没有运行 Tauri 桌面窗口，因此没有做像素级视觉回归和真实键盘/读屏走查。已执行：`bun run test`（65 pass / 0 fail，9 个文件）、`bun run typecheck`、`bun run lint`（0 error / 47 warning）、`bun run format:check`、`bun run build`（产物 163 KB JS / gzip 51 KB，66 KB CSS / gzip 12.5 KB，字体约 3.5 MB）。

---

## 0. 结论

前端已经完成了 2026-11-20 审查中最重要的结构性整改：`App.tsx` 变薄，聊天状态进入 `ChatProvider`，新增 `bridge.ts`、`platform/`、`layout/`、`shells/`、`pages/`，DTO 由 Rust 生成，设置页按 desktop/mobile 拆成两个懒加载 view，长会话接入 `VirtualTranscript`，样式拆成有序模块，测试/lint/format/typecheck 都进入 CI。

当前的主要问题不再是"没有架构"，而是几处**信任、状态语义和可访问性**的具体缺陷：

- 空状态文案向用户承诺"每一步都需要批准"，与当前安全策略、README 和 SPEC 矛盾。
- Composer 主动移除了 textarea 的键盘焦点轮廓，只剩容器阴影。
- `currentRun` 取 session 的最后一条 run，在多 Agent 会话里可能是子 Run，导致运行中/失败状态判断错误。
- 设置页的 Android 返回事件有两个监听者同时处理，会把未保存的供应商编辑直接关掉。
- 保存供应商会把 `enabled` 和所有模型强制改成启用，静默覆盖 CLI/其他 shell 写入的状态。
- 上下文面板用字符比例估算 token 组成，却没有按 SPEC 标注"估算"。

| 维度 | 判断 | 说明 |
|---|---|---|
| 信息架构 | 强 | app / shells / pages / core / platform / layout 分层清晰 |
| 组件粒度 | 中–强 | 比上一轮明显改善；`Conversation`、`ProvidersPage` 仍偏大 |
| 状态管理 | 中 | ChatProvider 合理，但 run 选择和刷新竞态仍有问题 |
| 安全渲染 | 强 | allowlist、sandbox iframe、表达式解释器、65 个测试 |
| 可访问性 | 中–弱 | 焦点、tooltip/dialog 语义、对比度、触控尺寸仍有缺口 |
| 文案诚实性 | 弱–中 | 空状态与上下文估算两处与实现/Spec 不一致 |
| 性能 | 中–强 | 虚拟列表、coalescing、懒加载都到位；键盘焦点会退化为全量渲染 |
| i18n | 中 | 机制可用，但扁平 catalog、无 key 覆盖检查、存在死 key |
| 测试/CI | 中–强 | 纯函数与渲染器覆盖好；缺关键交互/状态机测试和 axe |

---

## 1. 相比 2026-11-20 的进展

上一轮审查的主要项已经落地：

- `ARCH-01/03`：命令和事件集中到 `src/bridge.ts`；组件不再直接写 `invoke`/`listen`。
- `ARCH-02`：`src/generated/` 由 ts-rs 生成；`src/types.ts` / `settings/types.ts` 只做 re-export 和窄化。
- `ARCH-06`：聊天状态进入 `src/core/chat/ChatProvider.tsx`；`App.tsx` 162 行，只负责 shell 选择与设置路由。
- `ARCH-07`：`reuseMessages` 按 message id 复用不可变消息；`VirtualTranscript` 超过 80 行后按测量高度窗口化。
- `ARCH-09/10`：`toolResults` 改为 memo；流式文本 64 ms 合并，`inlineUi` 增量扫描。
- `ARCH-12`：根级 `ErrorBoundary`。
- `ARCH-13/21`：设置页桌面/移动 view 与 section pages 懒加载。
- `ARCH-16`：`App`、`ProviderEditor`、`Markdown`、`RichHtml`、两套 CSS 已完成拆分。
- `ARCH-25`：Markdown/RichHtml/表达式/inline UI 有 394 行渲染器测试。
- `ARCH-29/30/32`：Bun test、ESLint（含 jsx-a11y）、Prettier、typecheck 都已在脚本和 CI 中。

这些不是"文档承诺"，是代码里实际存在的结构。后续问题应按"具体缺陷/边界"处理，而不是重新设计整层。

---

## 2. 高优先级 UI/UX 与信任问题

### 2.1 空状态文案与安全策略矛盾

`src/components/Conversation.tsx:322-326` 的空状态写着：

> It can read your files, edit them and run commands — always with your approval.

中文 catalog（`src/i18n.ts:78-79`）同样承诺"每一步都需要你的批准"。但当前策略是：

- 工作区内 `fs_write` 自动放行；
- 只读命令与项目验证命令自动放行；
- 只有其余动作进入审批。

这与 `README.md:33`、`README.zh-CN.md:44/111`、`docs/design/SPEC.md:248` 都矛盾。空状态是用户第一次看到产品时对安全模型的承诺，不能保留旧文案。

建议改为类似："safe actions run automatically; everything else asks"（中文："安全动作自动执行，其余操作会先询问"），并在需要时链接到设置/安全说明。

### 2.2 上下文面板把估算值呈现为精确值

`ContextMeter.tsx:52-64` 用字符比例把 `usedTokens` 分摊到历史/工具/系统提示，`:136-175` 直接展示这些数字和一位小数百分比。代码注释和 `docs/design/SPEC.md:275` 都要求"按字符比例估算并标注"，`src/i18n.ts:132` 甚至已经有 `"Estimated from character share."` 的翻译，但 JSX 没有渲染它。

建议在面板图例上方或下方加一行明确标注"组成按字符比例估算"，并考虑把估算段的百分比改成整数或加 `~`，避免看起来像 provider 上报的精确值。

### 2.3 Session 删除没有确认

`Sidebar.tsx:378-390` 的菜单项 `Delete session` 直接调用 `chat.deleteSession`，没有二次确认；记忆和供应商删除都有 3–5 秒的二次确认。catalog 里仍保留的 `"Delete session {title}"`、`"Click again to delete"`、`"Cancel the active run before deleting this session."`（`i18n.ts:59-61`）目前没有使用，像是从旧实现遗留。

建议恢复两段式确认（或提供 undo），并复用记忆/供应商的确认语言与计时器模式。

### 2.4 设置页 Android 返回事件被两个监听者同时消费

`App.tsx:62-66` 在 Android 返回时 dispatch `phi:settings-back`；两个地方监听：

- `SettingsRoute.tsx:68-79`：只要 `mobileSection !== null` 就 `preventDefault()` 并关闭 mobile detail。
- `ProvidersPage.tsx:218-226`：编辑供应商时 `preventDefault()` 并 `requestExit()`（可能显示草稿守卫）。

事件监听器都会执行；`preventDefault()` 不会停止传播。实际顺序是：用户正在编辑供应商 → 按 Android 返回 → ProvidersPage 设置 `pendingExit`，SettingsRoute 同时把 `mobileSection` 设为 `null`，detail 被隐藏；草稿守卫留在隐藏的 ProvidersPage 里，用户看不到。即使不 dirty，返回也直接跳到设置首页，而不是"供应商编辑器 → 供应商列表 → 设置首页"。

建议：

- SettingsRoute 的 `onBack` 先检查 `event.defaultPrevented`，或 ProvidersPage 使用 `stopImmediatePropagation()`；
- 更好的做法是把设置导航改成 typed context/callback，而不是 `window` CustomEvent 字符串。

### 2.5 ChatPage 在打开设置时卸载，滚动位置丢失

`App.tsx:110-123` 用 `Show` 在 `ChatPage` 和 `SettingsRoute` 之间切换；打开设置会卸载 `Conversation`，返回聊天时 `VirtualTranscript` 的滚动位置、测量高度和折叠状态丢失，通常直接回到最新消息。Composer 的输入文本在 `ChatProvider` 里，不会丢；但阅读位置会丢。

建议让 ChatPage 保持挂载、用 `hidden`/`inert` 隐藏（设置页已经是这种 keepalive 模式），或至少在切换前保存/恢复 transcript 的 scrollTop。

### 2.6 初始平台判断会让移动端先闪一下桌面壳

`layout/environment.ts:23` 的 `platformName` 初值是 `"linux"`，要等 `ChatProvider.refresh()` 拿到 `bootstrap.platform` 后才 `setPlatform`（`ChatProvider.tsx:95`）。因此 Android/iOS 首帧会渲染 `DesktopShell`（含桌面标题栏和 caption 按钮），随后切到 `MobileShell`；macOS 首帧也会短暂显示 Windows/Linux 风格窗口按钮。

建议在启动前从同步可用的来源（Tauri `os` 插件、user agent 或注入的初始配置）确定 shell，或者在 bootstrap 完成前只渲染一个中性的启动壳。

---

## 3. 多 Agent 与运行状态

### 3.1 `currentRun` 取的是最后一条 run，不一定是根 Run

`ChatProvider.tsx:108-113`：

```ts
function currentRun(): Run | null {
  const runs = detail()?.session.runs ?? [];
  return runs.length > 0 ? runs[runs.length - 1] : null;
}
```

`SessionView.runs` 包含子代理的 child run（`RunScheduled` 按调度顺序追加）。一个 lead + subagent 的会话里，最后一条 run 很可能是 child。于是：

- `isRunning()` 用 child 的 state；child 已结束而 lead 还在跑时，Composer 会重新启用，发送后后端报 "session still has a running task"。
- `Conversation.failedRun`（`:415`）可能把 child 的失败显示成整个会话的失败 notice；`runActive`（`:418`）同样只看 child。
- Sidebar 反而正确：它使用后端 `SessionSummary.lastRunState`，Rust 的 `summary_of` 只取 `parent_run_id.is_none()` 的根 Run。

建议：会话级状态只取最后一个 `parentRunId == null` 的 run；子 Run 若要在 UI 中出现，应作为 Agent 活动/线程呈现，而不是复用会话 Runbar。`SessionView.agents` 目前完全未使用。

### 3.2 多 Agent 的 UI 还没有接上

`SessionView.agents`、`AgentView`、child run 的 `threadId`/`reportMessageId` 都已生成类型，但前端只渲染主线程消息（后端 `get_session` 也只返回主线程），所以子 Agent 的工作过程对用户不可见。这与 `AGENT.md` 的"desktop surfaces are still being connected"一致，不算隐藏 bug，但应在 UI 中明确：当前版本要么不暴露 delegate 工具，要么给多 Agent 一个最小可见面。

### 3.3 Run/状态仍是字符串联合

`status.ts:11-49`、`Sidebar.tsx:39-84`、`Conversation.tsx:395/415/418` 都硬编码 `"Scheduled"`、`"WaitingApproval"` 等字符串；`RunView.state` 生成类型是 `string`。这与后端的字符串状态契约叠加，任何新状态都不会被类型系统发现。建议在 Rust 定义 `RunState` 枚举并生成 TS union，前端 `switch` 可穷尽；`STATE_LABELS` 只保留显示映射。

---

## 4. 设置与数据一致性

### 4.1 供应商规则仍在前端复制，并会静默改状态

`ProvidersPage.tsx:104-151` 的 `buildConnection` 决定 auth 头（`x_api_key` / `bearer` / `none`），`mergeModelSpecs` 决定模型合并和大小写去重，`persistModels` 决定主模型；后端 `ProviderStore::upsert` 也有同一套规则。两条规则链已经开始分叉。

更具体的数据丢失：

- `buildConnection` 永远发送 `enabled: true`（`:116-118`），任何在 CLI/其他 shell 中设为 disabled 的供应商，通过 UI 保存后会被重新启用；UI 里没有 enabled 开关，用户看不到这次变化。
- `persistEdit` 把 `current.models` 全部映射为 `enabled: true`（`:345-350`），之前在 CLI 中禁用的模型会被重新启用。

建议：`ProviderInput` 的这类派生/保留字段由 Rust 统一决定；UI 只提交用户改过的字段，或在保存时保留 `view.enabled` / `model.enabled` 的原值。auth 推导也应回 Rust（协议默认规则已经在 `Protocol::default_auth` 里）。

### 4.2 "测试连接"会先保存

`ProvidersPage.tsx:454-459` 的 `handleTest` 先 `persistEdit()`，再 `testProvider`。按钮在 dirty 时已经改名为 "Save & test"（`ProviderEditor.tsx:383-385`），所以文案不算欺骗；但如果用户只是想验证一个尚未决定保存的地址/密钥，操作仍会写入 provider 文件。建议把 test 做成对当前草稿的纯探测（后端已有 `preview_models` 的"不持久化"模式），或在按钮旁明确"会先保存"。

### 4.3 记忆列表静默截断 200 条

`MemoryPage.tsx:105-110` 调用 `listMemories(null)`，后端 `api.rs:179-188` 固定 `limit: 200`。搜索有 "Showing up to 50 matches." 提示，但普通列表没有"仅显示前 200 条"的提示。统计行会显示更大的 `total`，列表却只有 200 条。建议增加提示或分页。

### 4.4 `ModelSelection.providerId` 从未发送

`types.ts:53-58` 的选择包含 `providerId`，`ModelPicker` 也按 provider 生成；但 `bridge.ts:56-65` 的 `SendMessageInput` 只有 `sessionId/text/model/thinking`，`ChatProvider.tsx:136-140` 也没传 `provider_id`。后端 Tauri command 明确支持 `provider_id`。结果是 UI 里存了一个不会被使用的字段；如果 active provider 在外部被切换，选中的 model id 可能属于旧 provider。建议要么发送 `providerId`，要么删掉该字段。

### 4.5 Composer 只检查"有 provider"，不检查 key/config

`ChatProvider.tsx:115-127` 的 `composerDisabled` 只要求 `provider()` 存在、`selection()` 存在；`Bootstrap.model.configured` / `hasKey` 没有参与。一个没有 API key 的 provider 仍会让 Composer 可输入，发送后才报认证错误。建议在 `provider.hasKey === false && provider.auth !== "none"` 时禁用并给出明确 hint，或至少显示状态 chip。

### 4.6 General 页的未保存 workspace/language 没有守卫

供应商草稿有 leave guard，General 页没有。用户输入新工作区后直接离开设置会丢失输入；这比供应商草稿的破坏性小，但仍是同类问题。建议统一 dirty 检测/守卫，或在离开时提示。

---

## 5. 可访问性与触控

### 5.1 Composer 没有键盘焦点指示

`src/styles/app/dock.css:60-66` 明确对 `textarea` 的 `:focus`/`:focus-visible` 设置 `outline: none; box-shadow: none; border: none`，注释却说"textarea itself keeps the global :focus-visible ring"。实际只有 `.dock:focus-within` 的阴影变化（`:21-26`），这是很弱的焦点提示。Composer 是主输入面，键盘用户必须能清楚看到焦点。

建议恢复 textarea 的 `outline: 2px solid var(--accent-ink)`，或给 `.dock:has(textarea:focus-visible)` 一个明显的 2px accent 边框/外环；不要只靠 elevation。

### 5.2 Alt 面板的语义和键盘可达性

`Alt.tsx:226` 把所有气泡（包括可交互、可固定的上下文面板）都标成 `role="tooltip"`；但 tooltip 按 ARIA 规范不应包含可交互控件。`ContextMeter.tsx:212` 的触发按钮又写了 `aria-haspopup="dialog"`，语义自相矛盾。面板内包含可展开的 header 按钮和统计内容，键盘用户从触发按钮 Tab 时通常不会进入 Portal 末尾的面板，`aria-describedby`/`aria-expanded` 也没有。

建议：

- 普通 hover 信息继续用 tooltip，并通过 `aria-describedby` 关联；
- 可固定的上下文面板改成 `role="dialog"`（或 popover），触发按钮带 `aria-expanded`，打开时把焦点送入面板，Escape 关闭并把焦点还给触发按钮。

### 5.3 表单错误没有关联到字段

`ProviderConnectionSection.tsx:36-41` 等只设置了 `aria-invalid`，没有 `aria-describedby` 指向 `.field-error`。读屏用户听到"invalid"却听不到原因。建议给错误 `<span>` 生成 id，并在 input 上关联 `aria-describedby`。

### 5.4 对比度：`--text-faint` 仍用于小字

`tokens.css:23` 的 `--text-faint: #7d8792` 在白底约 3.65:1、app 底色约 3.4:1，低于 WCAG AA 小字 4.5:1。它目前仍用于：

- `.context-note` 12.5px（`context-meter.css:219-224`）；
- `.alt-session-exact` 11px（`sidebar.css:236-241`）；
- 若干图标/装饰（这些不受 4.5:1 约束）。

建议把文字用途改为 `--text-muted`（约 5.3:1），或整体加深 `--text-faint`，只保留给图标/点。

### 5.5 触控目标未达到项目自己的 44px 目标

上一轮把 coarse pointer 目标提升到 44px，但仍有：

- `.context-ring` 30×30（`context-meter.css:3-8`），Composer 上的主要按钮；
- `.ui-select--ghost` 40px（`select.css:183-185`），模型/思考选择器；
- `.activity-trigger` 40px（`responsive.css:91-93`）；
- `.session-more` 40px（`sidebar.css:359-364`）；
- `.message-dot-hit` 16×18（`messages.css:46-54`），时间气泡触发点。

这些不都违反 WCAG 2.5.8 的 24px AA 下限，但违反了项目在上一轮设定的 44px 目标。建议至少扩大 context ring 和 ghost select 的命中区（伪元素或透明 padding），并决定 message dot 是否值得作为触控目标。

### 5.6 reduced-motion

`prefers-reduced-motion` 已按 DEC-01 移除；这是已记录的取舍，本次不重新讨论。`prefers-reduced-transparency` 在 `alt.css:45-51` 有降级，是正面项。

---

## 6. 性能

### 6.1 键盘焦点会让虚拟列表退化为全量渲染

`VirtualTranscript.tsx:84-105` 在 `keyboardFocus()` 为真时返回全部行；`:178-193` 的 `onFocusIn` 只要焦点进入 transcript 就设为 true。`Conversation.tsx:661` 在 rows > 80 时给滚动容器 `tabindex=0`。因此键盘用户第一次 Tab 进 transcript 就会挂载所有行——长会话（几千条消息）可能直接卡死。

建议：不要在焦点进入时就渲染全部；只保证当前聚焦行和临近行挂载，或使用 roving tabindex/aria-activedescendant；如果为了原生 Tab 顺序必须全量，应设置一个上限（如 500 行）并提示。

### 6.2 无界缓存

- `VirtualTranscript.tsx:39` 的 `heights` Map 以 row key 累积，不随 session 切换清理；消息量长期增长。
- `activity.tsx:57` 的 `disclosureOpen` Map 以消息/工具 id 累积，没有像 `groupCache`/`approvalRows` 那样剪枝。
- `Conversation.tsx:59-65` 的 `groupCache`/`approvalRows` 会剪枝，`userRows`/`blockRows` 是 WeakMap，设计正确。

建议把 `disclosureOpen` 和 `heights` 的生命周期绑定到 `ChatProvider` 或当前 session，切换/删除 session 时清理；或使用有界 LRU。

### 6.3 全量快照刷新仍在

每个 durable event 仍会在 120ms 防抖后重新 `get_bootstrap` + `get_session`（`ChatProvider.tsx:92-106`、`usePhiStream.ts:37-42`），后端返回整份 session。前端已经用 `reuseMessages` 和虚拟列表降低了渲染成本，但 IPC 序列化/克隆仍是 O(messages)。这是 `frontend-review-2026-11-20.md` 的 ARCH-04，当前仍开放；需要后端 cursor/增量投影才能根治。

### 6.4 其他

- `Conversation.tsx:420-499` 每次 detail 刷新会先构建 `toolResults` Map，再在 `rows` 里重建 `toolUseIds`，对消息做两遍扫描；可以合并成一次。
- 构建预算表现良好：初始 JS gzip 51 KB、CSS gzip 12.5 KB，设置页按需 chunk（Providers 10.2 KB gzip），字体约 3.5 MB 本地资产。桌面应用可接受；若未来做 Web 预览，需要重新评估中文字体。
- `ContextMeter` 的百分比/指标每秒 tick 只更新 `now`，不会重算 `rows`；当前粒度合理。

---

## 7. 架构与工程结构

### 7.1 做得好的部分

- 目录按职责分开：`app/`（装配）、`shells/`（桌面/移动 chrome）、`pages/`（页面）、`core/chat`（状态）、`platform`（原生能力适配）、`layout`（环境信号）、`ui`（通用控件）、`components`（聊天/渲染组件）、`settings`（设置功能与样式）。
- `bridge.ts` 是单一 Tauri 边界；`platform/index.ts` 是单一原生能力边界；组件不直接 import Tauri。
- 设置页 desktop/mobile 分离并 lazy；section pane 首次访问后 keepalive，草稿可跨 section 保留。
- CSS 已按顺序拆成模块，`Settings.css` / `app.css` 只是 barrel；tokens 集中在 `tokens.css`。
- 渲染安全分层清楚：Markdown 纯解析 → RichHtml allowlist 重建 → RichSandbox 脚本禁用 iframe；表达式解释器无 eval；测试覆盖了主要攻击面。

### 7.2 仍然偏大的模块

| 文件 | 行数 | 问题 |
|---|---:|---|
| `components/Conversation.tsx` | 708 | 行模型构建、缓存、streaming、run notice、approval 行混在一起 |
| `settings/ProvidersPage.tsx` | 655 | 列表、wizard、草稿守卫、连接/模型持久化、测试/激活/删除 |
| `components/uiExpression.ts` | 660 | 纯解释器，尚可；可按 tokenizer/parser/evaluator 拆 |
| `styles/app/inline-ui.css` | 1191 | inline UI 设计系统，建议按 layout/cards/metrics/charts/forms 拆 |
| `components/activity.tsx` | 407 | 可接受；可与 Conversation 的行模型进一步分离 |
| `i18n.ts` | 393 | 扁平 catalog（302 个 key），无类型化 message id |

`Conversation.tsx` 最值得先拆：把 `441-553` 的 rows 构建、`estimateRow`、row cache 抽成 `transcriptModel.ts` 纯函数/有状态 store，组件只负责渲染。这样可以直接为“多 Run 选择、分组、结果配对”写单元测试，而不是继续依赖 headless 端到端。

`ProvidersPage.tsx` 建议抽 `useProvidersPage`（加载、dirty、wizard）和 `ProviderListView` / `ProviderEditorView`；连接/模型规则应回 Rust（见 §4.1）。

### 7.3 目录命名还有两处混用

- `settings/GeneralPage.tsx`、`MemoryPage.tsx`、`ProvidersPage.tsx` 是页面，却放在 `settings/`；路由壳在 `pages/split/settings/`。建议把页面移到 `pages/settings/`，`settings/` 只保留共享表单、类型、样式和 provider 逻辑。
- `layout/environment.ts` 和 `platform/index.ts` 都涉及平台；前者是布局/指针环境，后者是原生能力。建议把前者改名为 `layout/viewport.ts` 或 `shellEnvironment.ts`，减少“platform”重名。

### 7.4 设置导航用全局 CustomEvent 字符串

`phi:settings-leave` / `phi:settings-back` 在 `App.tsx`、`SettingsRoute.tsx`、`ProvidersPage.tsx` 三处手写。它们没有类型、没有统一常量、没有优先级，正是 §2.4 那类 bug 的温床。建议用 `SettingsNavigationContext`（提供 `requestBack`、`requestLeave` 和 `registerGuard`）或把导航状态提升到 `SettingsRoute` 内，通过 props 传递。

### 7.5 i18n

- 英文原文当 key、扁平 `Record<string,string>`；改英文文案会静默丢掉中文翻译。
- 没有 key 覆盖测试；catalog 中至少有 `"Delete session {title}"`、`"Click again to delete"`、`"Cancel the active run before deleting this session."`、`"Estimated from character share."`、`"Input {input} · Output {output} tokens"` 等当前未被引用的条目。
- `t()` 找不到翻译时静默回退英文；新增字符串不会在 CI 失败。建议拆成按 feature 的 catalog 模块，并加一个“源码中使用到的 key ⊆ catalog”的测试（动态 key 如 `stateLabel` 可维护显式白名单）。

### 7.6 事件契约仍偏弱

`bridge.ts:140-145` 的 `PhiEventPayload` 只声明了 `type/runId/callId`，`Run.state` 是 `string`。后端已有 typed `DomainEvent` 和 `RunView`，建议把事件类型也生成到 TS（至少生成 `RunState`/`DomainEventType` 的字面量联合），避免前端用字符串集合维护 `TERMINAL_EVENTS`、`FAILED_STATES`。

### 7.7 文档与实现漂移

- `docs/design/SPEC.md:299` 说 primary 按钮白字；实际 tokens 用 `--on-accent` 深色字（这是为对比度做的正确调整，应更新 SPEC）。
- `SPEC.md:350` 说 Alt 气泡 hover 跟随指针；实际 `Alt.tsx` 明确锚定触发元素（避免闪烁，代码更好，应更新 SPEC）。
- `SPEC.md:275` 要求上下文组成"估算并标注"，代码没标注（代码应改）。
- 空状态文案与 README/策略矛盾（代码应改）。

---

## 8. 测试、Lint 与构建

已执行并全部通过：

- `bun run test`：65 pass / 0 fail，9 个文件，3.37s。
- `bun run typecheck`：通过。
- `bun run lint`：0 error / 47 warning（主要是 `solid/reactivity` 对回调 prop 和 Tauri 事件 handler 的误报/可接受的快照初始化）。
- `bun run format:check`：通过。
- `bun run build`：通过；初始 JS 163 KB / gzip 51 KB，CSS 66 KB / gzip 12.5 KB，字体约 3.5 MB。

测试结构：

- `tests/renderers.test.ts`（394 行）覆盖 RichHtml allowlist、sanitizer、SVG、表达式、图表等，是当前最有价值的测试。
- `tests/i18n.test.ts` 只覆盖 `tPlural`/`formatList`，没有 catalog 完整性。
- `tests/inlineUi.test.ts`、`uiExpression.test.ts` 覆盖解析器/表达式边界。
- 没有测试 `ChatProvider`/`usePhiStream` 的流式交接、`VirtualTranscript` 的窗口/焦点行为、`SettingsRoute`/`ProvidersPage` 的草稿守卫和返回逻辑、多 Run 选择、供应商保存的 `enabled` 语义。

建议：

- 补 Vitest/Bun 的 DOM 交互测试：ChatProvider 的 send/refresh/stream handoff、VirtualTranscript 的 pinned/scroll/focus、ProvidersPage 的 dirty/back/save、session 列表的删除确认。
- 加 `axe-core`（或 eslint-plugin-jsx-a11y 已部分覆盖）到关键页面渲染测试。
- CI 增加 `bun run types:generate && git diff --exit-code src/generated`，防止 Rust DTO 改动后 TS 生成物漂移。
- 对 47 个 lint warning 分类：可接受的快照/回调写法加注释或局部 disable，真实问题修掉；不要长期保留“全绿但有 47 条 warning”的状态。

---

## 9. 建议的修复顺序

1. **文案与信任**：改空状态安全承诺；给上下文组成加"估算"标注；同步 SPEC/README。
2. **焦点与运行状态**：恢复 Composer 焦点环；`currentRun` 只取根 Run；修复 Android 返回事件冲突。
3. **设置数据一致性**：保存 provider 时保留 `enabled` / 模型 `enabled`；把 auth/merge/primary 规则收回 Rust；明确 test 会保存。
4. **破坏性操作与触控**：session 删除二次确认；context ring/ghost select 命中区提升到 44px；`--text-faint` 文字用途换 `--text-muted`。
5. **可访问性语义**：Alt 可交互面板改 dialog/popover，补 `aria-expanded`/`aria-describedby`；表单错误关联字段。
6. **性能与结构**：VirtualTranscript 键盘焦点窗口化；清理 `heights`/`disclosureOpen`；拆 Conversation 行模型和 ProvidersPage 状态。
7. **契约与工程**：生成 RunState/事件 union；i18n key 覆盖测试；generated 类型 CI 检查；补交互测试与 axe。

---

## 10. 文档影响

- 本文件登记在 `docs/README.md`。
- 建议同步更新 `docs/design/SPEC.md` 的按钮文字、Alt 定位、空状态文案和上下文估算标注；本次只记录，不改实现。
- 本次只做审查，未修改前端代码。
