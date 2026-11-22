# 前端 UI/UX 与工程审查（2026-11-20）

> 状态：🌿 评审记录
> 日期：2026-11-20
> 范围：`src/` SolidJS 外壳、设置页、字体、i18n、构建与工具链
> 性质：只记录发现与决策，不修改实现
> 参照：`docs/60-frontend/multi-frontend.md`、`docs/design/SPEC.md`、`AGENT.md`

审查期间 inline UI（`RichHtml.tsx`、`uiExpression.ts`、`InlineChart.tsx`）正在并发改动；本文记录当时可见的结构性问题，具体行号可能随后续提交漂移。

---

## 0. 结论

聊天主界面已经具备桌面/移动的响应式骨架，安全渲染方向正确；但整体仍是 **Tauri 绑定的单体 SPA**，与 `docs/60-frontend/multi-frontend.md` 的"薄外壳 + 命令/事件契约"目标有距离。短期可运行，继续加功能会先遇到跨端类型漂移、全量刷新、前端测试空白三个瓶颈。

| 维度 | 评价 | 证据 |
|---|---|---|
| 跨端契约 | 弱 | 31 处直接 `invoke`，分散在 5 个文件；TS 类型手写镜像；无 ts-rs/specta |
| 状态管理 | 弱–中 | `App.tsx` 约 630 行、16 个 signal；每个事件全量拉 bootstrap + session |
| 组件拆分 | 中–弱 | `ProviderEditor.tsx` 约 1070 行；两套 `ModelPicker`；`errorText` 重复 5 份 |
| 样式 | 中 | token 体系完整；但全局 CSS、死代码、断点分散 |
| i18n | 中 | 当前 key 100% 有中文；英文当 key，无复数/类型校验 |
| 安全渲染 | 强 | 无 `innerHTML`、allowlist、CSP；但自定义解析器没有测试 |
| 性能 | 弱–中 | Solid `<For>` 身份重建、`toolNames` O(n²)、流式全量 split |
| 测试/工具 | 弱 | CI 有 `tsc` + build；无 lint、无前端测试 |
| 构建 | 中–强 | Vite 配置明确；字体资产约 11MB |

---

## 1. UI/UX 发现

### 1.1 移动端设置页

- **[高] UX-01 桌面信息架构压缩到手机**：≤760px 时设置导航变横向标签（`src/settings/Settings.css` 的 max-width:760 媒体查询），供应商页仍是"列表 + 详情"上下堆叠（≤900px 单列）。手机上点供应商后编辑器在列表下方，长列表需要向下滚才能看到变化；没有 drill-down、返回列表或自动滚动。General/Memory 同样保持桌面表单密度。
- **[高] UX-02 横向导航溢出**：返回 + 三个 section 在 360dp 下接近或超过一屏；`.settings-nav` 用 `overflow-x:auto` 且无滚动提示。"返回对话"是第一个 item，向右滑会消失；打开设置固定落在"模型供应商"（`SettingsPage.tsx` 的 `createSignal("providers")`），切到"通用"需要横向找。
- **[高] UX-03 sticky 底栏不适合手机**：`editor-foot` 把保存、测试连接、设为当前、删除供应商放在同一条 sticky footer（`Settings.css`、`ProviderEditor.tsx`）。窄屏折成 2–3 行，占据底部并覆盖表单；删除与保存处于同一操作区。
- **[高] UX-04 设置页没有底部安全区**：`Settings.css` 中没有任何 `env(safe-area-inset-*)`；聊天页 dock 有（`App.css`），设置页的 sticky footer 和列表底部没有，Android 手势条/iOS home indicator 会压住按钮；横屏也没有 left/right inset。
- **[高] UX-05 字号与密度是桌面尺度**：设置输入框 13px（`App.css` 全局 input 规则），label 12px、hint 11px、badge 10.5px，没有手机级放大。Android 低于 Material 常用 14–16sp 输入/正文基线；iOS 对 <16px 输入框聚焦会自动放大。中文 10px 级标签尤其吃力。
- **[高] UX-06 工作区只能手输绝对路径**：`GeneralPage.tsx` 只有文本输入，没有目录选择器；手机上输入 Android 绝对路径基本不可用。
- **[高] UX-07 未保存草稿静默丢失**：切换供应商、点"添加供应商"直接覆盖 draft（`ProvidersPage.tsx`）；离开设置会卸载页面并清理 wizard 草稿。没有 dirty guard/确认。`Test connection` 还会先 `persistEdit()`，表面测试实际保存。
- **[中] UX-08 破坏性确认不统一**：删除供应商用 `window.confirm`，删除记忆用行内二次点击；前者是系统弹窗，后者二次确认只靠颜色变化。
- **[中] UX-09 触点偏小**：`pointer: coarse` 统一到 40px（`App.css`），低于 Android 推荐 48dp；`.editor-section.collapsible > summary`、`.thinking-block > summary`、`.run-notice-details summary` 等折叠标题未纳入放大。
- **[中] UX-10 键盘输入提示缺失**：Base URL、API key、模型 ID 等没有 `inputmode`、`autocapitalize="off"`、`autocorrect="off"`，Android 键盘可能自动大写或纠错。
- **[中] UX-11 返回行为缺失**：没有看到 Android 系统返回键/返回手势的桥接；在设置或抽屉里按系统返回可能退出应用。设置页每次重新打开都回到"模型供应商"。
- **[中] UX-12 错误提示位置与操作位置分离**：供应商保存/测试失败显示在 `editor-status`（表单顶部），操作按钮在 sticky 底部；长表单上点保存失败后提示可能在屏幕外。

### 1.2 中文字体与排版

- **[高] UX-13 mono 字体栈没有中文回退**：`--font-mono: "Cascadia Code", "JetBrains Mono", Consolas, monospace`。Cascadia/Consolas 无 CJK 字形，中文一旦落在 `.mono` 就会回退到系统字体（Windows 通常为宋体一类）。实际位置：`.session-time` 的"3 分钟前"、工具块的"调用 / 工具结果"、审批的"动作摘要 / 已过期"、`.tb-path` 的中文工作区路径、`field-hint.mono` 的"当前密钥"、`pre` 中的中文错误。同一行数字是 Cascadia、中文是另一套字，是"中文不统一"最直接的来源。
- **[高] UX-14 中文微字号偏小**：`.side-label` 10.5px、`.run-stat-label` 10px（≤760 为 9.5px）、`.model-row-effort` 9.5px、`.model-menu-head` 10px、`.risk-tag` 10.5px、`.provider-active` 10.5px、设置 hint 11px。这些尺寸配英文全大写可以，配中文可读性明显下降；`text-transform: uppercase` 对中文无效，留下的 `letter-spacing: 0.04–0.06em` 只让字更散。
- **[中] UX-15 系统 locale 混入**：`MemoryPage.formatTime` 用 `new Date(raw).toLocaleString()`，不跟随 app 语言；会话时间走 app locale（`format.ts`）。中文界面记忆日期可能出现系统语言/格式。
- **[中] UX-16 中文标点不统一**：Rust catalog 大量中文使用半角冒号（"命令执行失败: {error}"、"审批结果: {status}"、"未知命令: {command}"），前端 catalog 用全角"："（"连接失败：{error}"）。toast/banner 与设置页会同时出现两种标点；还有 `Token` / `tokens` 的中英混用。
- **[中] UX-17 对比度**：`--text-faint: #7d8792` 在白底 3.65:1、在 app 底色 3.37:1，低于 AA 小字 4.5:1；用于 10.5px `.session-time` 和 10px `.model-menu-provider`。
- **[低] UX-18 合成斜体**：只引入 normal 字形，Markdown/RichHtml 的 `<em>/<i>` 由浏览器合成倾斜；中文整体 slant 观感不自然。Figtree 的 italic 文件未引入。
- **[低] UX-19 初始 lang**：`index.html` 固定 `lang="en"`，要等 bootstrap 后 `setLang` 才更新；启动瞬间的加载/读屏语言是英文。
- **[规范] UX-20 中文 literal**：`src/status.ts` 的 `risk.includes("不可逆")` / `"可逆"` 违反 `AGENT.md` 的英文源码规则，并把风险判断绑定到中文文本。

### 1.3 其他移动端交互

- **[高] UX-21 Composer 在手机上无法换行**：无 Shift 的 Enter 一律 `preventDefault` 并发送；手机软键盘没有 Shift+Enter，多行提示无法输入，按回车就发送。
- **[高] UX-22 禁用原因只有 `title`**：`composerDisabledHint` 最终只作为模型 chip 的 `title`；触摸端没有 hover，有历史消息但没有 provider/模型时只看到一个死掉的输入框。
- **[高] UX-23 Runbar 标签被删掉**：≤520px 时 `.run-stat-label` 直接 `display:none`，只剩"3 1.2k↑/300↓ 0:42"这类无标签数字；读屏也失去 Steps/Time 名称。
- **[中] UX-24 安全状态在手机上不可见**：≤900px 隐藏 `.mode-pill`，Normal/Degraded/Maintenance 只能打开抽屉在底部看到。
- **[中] UX-25 关闭的抽屉仍可聚焦**：桌面收起只改 `margin-left`，没有 `visibility: hidden` / `inert`；≤900 才有 `visibility`，移动端宽度 >900 走 `.drawer-mode` 类时也没有。键盘 Tab 可进入看不见的会话列表；抽屉没有焦点陷阱和关闭后的焦点返还。
- **[中] UX-26 动态提示未宣告**：MemoryPage 的 notice、GeneralPage 的 Saved 没有 `aria-live`；审批卡出现也没有 live 区域（toast 有）。
- **[中] UX-27 只有 title 的元数据**：S3 规模、context window、model provider 名称等只靠 `title`，触摸端拿不到解释。

### 1.4 桌面/通用可访问性

- **[中] UX-28 placeholder 当 label**：工作区、记忆添加/搜索、模型搜索都只有 placeholder，没有 `<label>` / `aria-label`；输入后上下文消失。字段错误没有 `aria-describedby`。
- **[中] UX-29 记忆搜索静默截断**：`search_memories` 固定 `limit: 50`，界面没有"仅显示前 50 条"的提示。
- **[低] UX-30 导航语义**：设置 tab 的 active 只有 class，没有 `aria-current`；`aside` 没有 `nav` 语义，设置内容没有 `main` landmark。输入框 `:focus-visible` 去掉 outline，只剩 1px 边框和很淡的软色阴影。
- **[低] UX-31 每次进入设置重置 section**：设置页不恢复上次 section。

---

## 2. 前端架构与工程发现

### 2.1 跨端契约

- **[高] ARCH-01 没有契约层**：组件直接 `invoke("list_providers")`、`invoke("get_session")`，`App.tsx` 直接 `listen("phi://event")` / `"phi://model-stream"`。命令名、参数名、事件名都是字符串，靠约定而非编译期约束。
- **[高] ARCH-02 没有生成类型**：`src/types.ts` 和 `src/settings/types.ts` 都写着 "mirrored from Rust"，是手工维护的。Rust 改一个字段，前端只会在运行时发现；项目文档要求的"以 Rust 类型为单一事实源生成 TS 类型"尚未落地（没有 ts-rs / specta / tauri-specta）。
- **[高] ARCH-03 没有适配器**：所有页面绑死 Tauri IPC，组件无法在浏览器直接跑，也无法用 mock bridge 做单元测试；文档里的 Web 控制台/远端节点无法复用这套 UI。
- **[高] ARCH-04 没有 cursor/增量**：`refresh()` 每次重新 `get_bootstrap` + `get_session`，把事件退化成全量快照刷新。协议版本、subscribe、resume 都还没有。
- **[中] ARCH-05 错误模型是字符串**：Rust 侧统一 `Result<T, String>`，前端把 `unknown` 转成 string 的逻辑复制了 5 份（`App.tsx`、General、Memory、Providers、ProviderEditor）；UI 无法区分校验/网络/认证错误。

### 2.2 状态与渲染

- **[高] ARCH-06 所有状态挂在根组件**：`App.tsx` 同时管理系统启动、会话列表、当前会话、运行状态、流式文本、审批、模型选择、抽屉、设置路由、定时器和两个 Tauri 事件订阅；没有 store、reducer、context 或资源抽象。
- **[高] ARCH-07 全量刷新 + Solid `<For>` 身份问题**：`scheduleRefresh` 每 120ms debounce 一次，每次都 `setDetail(新对象)`。Solid 的 `<For>` 按引用做 key，`invoke` 反序列化出来的 message 每次都是新对象，导致整条 transcript 的 MessageRow 销毁重建：展开的 tool/thinking `<details>` 收回、复制状态丢失、inline UI 表单值丢失、Markdown 每次刷新重新解析。
- **[高] ARCH-08 领域逻辑放错层**：`src/status.ts` 用 `toolId === "shell_exec"` → high、`fs_write` → medium 硬编码风险等级；`ProvidersPage.buildConnection` 决定 auth 头、`mergeModelSpecs`/primary 选择、`normalizeModelId` 去重。风险与 provider 规则的权威在 Rust，前端成了第二个实现。
- **[中] ARCH-09 `toolNames()` 是 O(n²)**：`Conversation.tsx` 每次遍历全部消息和 block 构建 Map，却在 `<For>` 每一行里调用一次；消息越多越慢。
- **[中] ARCH-10 流式路径是 O(n²)**：每个 token 都重建 `streamText`，`splitInlineUi` 扫描全部已接收文本，`RichHtml` 的 `createMemo` 又对整段 HTML 重新 `DOMParser`；长回复 + inline HTML 在手机上会卡。
- **[中] ARCH-11 没有虚拟列表**：所有历史消息、工具结果、`pre` 内容一次性进 DOM；CSS 的 `max-height` 只限制可视高度，不减少节点和文本量。
- **[中] ARCH-12 没有 ErrorBoundary**：任一渲染器（尤其 RichHtml/表达式求值）抛异常会白屏，而消息内容是模型生成的不可控输入。
- **[中] ARCH-13 设置页全量 mount**：`SettingsPage` 始终渲染三个 pane，打开设置就请求 memory/providers，即使只看 General。草稿保留是优点，但应 lazy mount + keepalive。
- **[中] ARCH-14 流式交接逻辑在根组件**：`prunePendingCalls`、2 秒 fallback、`streamRunId/streamSessionId/runTerminal` 散在 `App.tsx` 和 `format.ts`，靠注释解释不变量；最需要状态机和单元测试的部分没有测试。
- **[中] ARCH-15 运行状态语义是字符串**：`Run.state` 只是 `string`，`isActiveState` 自己维护集合；事件 payload 是 `{ type?: string }`，拼写错误不会被类型系统拦住。

### 2.3 组件与样式

- **[中] ARCH-16 大文件趋势**：`App.tsx` 约 632、`ProviderEditor.tsx` 约 1071、`Markdown.tsx` 约 631、`RichHtml.tsx` 约 591、`uiExpression.ts` 约 627、`InlineChart.tsx` 约 305、`App.css` 约 2817、`Settings.css` 约 1113 行。规模尚可，但已在向功能域膨胀。
- **[中] ARCH-17 两套 `ModelPicker`**：`src/components/ModelPicker.tsx` 是聊天用的；`ProviderEditor.tsx` 内又定义同名 `ModelPicker` 负责拉取/勾选模型。名字相同、职责不同、CSS 类名不同，后续维护容易混。
- **[中] ARCH-18 状态模式复制粘贴**：`errorText` ×5，`loading/busy/error/notice` 在 General/Memory/Providers 各写一遍；没有 `createAsyncAction`、`useAsync` 或统一表单原语；按钮/输入框只是 CSS 类，label、aria、错误关联无法统一。
- **[中] ARCH-19 CSS 全局命名空间**：token 集中在 `App.css` 是优点，但组件类名（`.field`、`.btn`、`.settings-card`）没有前缀/模块/`@layer`；`Settings.css` 隐式依赖 `App.css` 变量。文档规划了插件 UI slot/region，全局 CSS 会直接冲突。
- **[中] ARCH-20 死代码与断点分散**：`.overlay`、`.dialog`、`.dialog-*`、`.switch` 已无组件使用；断点同时存在于 JS（`NARROW_QUERY = 900`、`matchMedia("max-width:760px")`）、`App.css`（1100/900/760/520）和 `Settings.css`（1000/900/760/560）。
- **[中] ARCH-21 没有按需加载**：`SettingsPage` 静态 import，设置页、供应商编辑器、Markdown/RichHtml/表达式解释器全部进首包；当前构建约 145KB JS、453KB CSS（CSS 大头是字体脸）。

### 2.4 i18n

- **[中] ARCH-22 i18n 机制原始**：`ZH: Record<string, string>` + 英文原文当 key；改英文文案会让中文翻译静默失效；无类型化 message ID、无 ICU/复数规则，英文会出现 `fact(s)`；`tArgs` 只是 `split/join`，无转义/格式化/fallback 告警。
- **[中] ARCH-23 前后端 catalog 分叉**：Rust 与前端各一套 catalog，标点风格已经分叉；日期格式化一部分走 `format.ts`，一部分走系统 locale；没有统一的 message format 层。
- **[中] ARCH-24 无自动化检查**：没有 i18n key 覆盖测试、没有 lint，`status.ts` 的中文 literal 和缺失/多余 key 不会被 CI 拦住。

### 2.5 解析与渲染

- **[高] ARCH-25 自定义解析器没有测试**：`Markdown.tsx`、`RichHtml.tsx`、`uiExpression.ts`、`InlineChart.tsx`、`inlineUi.ts` 合计约 2200 行自定义解析/求值/渲染代码，处理不可信模型输出。allowlist、节点预算、深度限制、URL 限制方向正确，但没有任何测试，是当前前端最大的风险点。
- **[中] ARCH-26 流式/持久语法不一致**：`inlineUi.ts` 只认 ` ```html `；`Markdown.tsx` 支持 `~~~` fence 和顶层 raw HTML 块。流式时显示为文本，落盘后变成界面块，会有可见跳变。
- **[中] ARCH-27 inline UI 动作链路未统一**：`onSend`/`onOpenUrl` 需要从 `App → Conversation → ToolBlock → Markdown → RichHtml` 多层 prop 透传；如果 inline UI 继续长表单/按钮，需要统一 action/command 层。
- **[中] ARCH-28 模型生成控件缺少边界提示**：inline UI 可以生成看起来像宿主控件的表单/按钮，并通过 `data-send` 直接触发用户消息；安全上不执行代码、仍走 policy/approval，但 UI 上需要明确"这是模型生成的内容"的边界，避免界面欺骗/诱导点击。

### 2.6 工程化与构建

- **强的部分**：`tsconfig` 开 `strict`、`noUnusedLocals`、`noUnusedParameters`；CI 有 `bunx tsc --noEmit` 和 `bun run build`，Rust 测试覆盖重；Vite 目标明确（es2021、woff2-only、modern CSS）；完全没有 `innerHTML` / `eval` / `new Function`，CSP 已配置。
- **[高] ARCH-29 没有前端测试**：没有 Vitest/Testing Library/Playwright；Markdown、RichHtml、uiExpression、inlineUi、流式交接、provider merge、i18n key 覆盖都没有测试。
- **[中] ARCH-30 没有 lint/format**：没有 ESLint/Prettier/Biome，也没有 `typecheck`/`lint`/`test` 的 package script；CI 靠 `bunx tsc`。
- **[中] ARCH-31 不可测试的页面**：页面直接调用 Tauri `invoke`/`listen`，没有可注入 bridge，组件无法脱离 Tauri 运行。
- **[中] ARCH-32 没有 a11y 自动检查**：没有 axe/eslint-jsx-a11y；上一节已有 label、aria-live、focus 管理问题。
- **[低] ARCH-33 字体资产偏大**：当前 `dist/assets` 约 12MB，其中 Noto Sans SC 约 11MB；约 420 个 woff2。运行时按 `unicode-range` 懒加载是对的，但安装包/首包体积成本高。

---

## 3. 决策记录

### 3.1 DEC-01 废除 `prefers-reduced-motion`

- **决定人/时间**：用户，2026-11-20。
- **状态**：已实施（2026-11-22）。
- **决定**：移除 `prefers-reduced-motion: reduce` 分支，不再在系统要求减少动效时关闭 pulse/spin/rise 等动画。
- **理由**：用户判断动画性能开销几乎可以忽略，保留动效可以提升体验。
- **实施**：删除 `src/App.css` 中唯一的 `@media (prefers-reduced-motion: reduce)` 块；动画在所有系统设置下保持启用。
- **影响**：`docs/design/SPEC.md` §7.4 / §8 已同步改为"不提供 reduced-motion 降级"。
- **已知取舍**：对前庭敏感/晕动症用户失去系统级的动效关闭能力。记录为已接受的取舍，不在本次实施中保留降级分支。
- **验收**：`src/` 不再出现 `prefers-reduced-motion`；SPEC 对应条款已更新；动效在减少动效的系统设置下仍正常播放。

### 3.2 DEC-02 字体精简

- **用户观察**：字体用得太多，体积膨胀。
- **状态**：已实施（2026-11-22）。
- **现状（实施前）**：三个字族 Figtree / Noto Sans SC / Cascadia Code；字重 Figtree 400/500/600/700、Noto 400/500/600/700、Cascadia 400/600；`dist/assets` 约 12MB，其中 Noto Sans SC 约 11MB（4 个字重 × 101 个 `unicode-range` 子集），Cascadia Code 约 336KB，Figtree 约 80KB；CSS 约 453KB。
- **判断**：体积主要来自 **Noto Sans SC 的字重 × 子集数量**，不是字族数量。Cascadia/Figtree 合计不到 0.5MB，先裁 Noto 收益最大。
- **实施（2026-11-22）**：Figtree 改为 `latin-*` 4 个字重；Noto Sans SC 改为 `chinese-simplified-{400,500,600}` 三个单子集；Cascadia Code 改为 `latin-{400,600}`。结果：`dist/assets` 约 3.7MB，字体约 3.4MB、11 个 woff2（含 Figtree 两个斜体），CSS 约 61KB。
- **权衡**：`unicode-range` 分片降低首屏加载量但增加包体；单子集文件降低包体但首次加载中文要拉完整子集。桌面/Android 都是本地资产，本次以包体优先；`chinese-simplified` 单子集只覆盖简体，繁体/生僻字会回退到系统 CJK 字体。
- **验收指标**：字体资产 < 5MB；`@font-face` 数量显著下降；CSS < 200KB；中文和等宽数字的视觉不回退。已达到。
- **文档影响**：`docs/design/SPEC.md` §2.1/§2.2 的字重与打包约定已同步；`--font-mono` 增加 Noto Sans SC 作为中文回退。

---

## 4. 建议的后续顺序

1. **补契约层**：建 `src/bridge/`，集中命令/事件/错误；用 ts-rs 或 tauri-specta 从 Rust 生成 TS；组件只依赖 bridge。
2. **修渲染模型**：按 message id 做 keyed reconciliation（或改用 store 合并），memo `toolNames`，避免每次事件重建整条 transcript；把流式交接从 `App.tsx` 抽成有测试的 reducer/state machine。
3. **在 inline UI 继续长大之前加测试**：Vitest + `@solidjs/testing-library`，优先覆盖 Markdown/RichHtml/uiExpression/inlineUi、provider merge、格式化和 i18n key 覆盖；同时引入 ESLint/Biome 和 axe。
4. **拆分单体**：Settings 按需加载；拆开 `ProviderEditor` 的连接表单与模型选择器；抽出共享字段/异步动作原语；CSS 用模块或前缀隔离。
5. **领域规则回 Rust**：风险等级、provider auth 推导、模型合并/主模型选择由后端给出派生字段，UI 只展示。
6. **字体与动效决策落地**：按 DEC-01 移除 reduced-motion；按 DEC-02 先裁 Noto 字重/子集，再评估是否减少字族。
7. 最后再做 Android 返回键/导航历史、远端适配器和插件 UI slot，这些依赖前面的 bridge/navigation 层。

---

## 5. 文档影响

- 本文件登记在 `docs/README.md`。
- DEC-01 已同步 `docs/design/SPEC.md` §7.4/§8。
- DEC-02 已同步 `docs/design/SPEC.md` §2.1/§2.2。
- 若后续把"移除 reduced-motion"视为范围移除，可在 `docs/80-roadmap/scope-additions.md` 追加一行引用本节；本文件先保留完整决策上下文。

---

## 6. 实施记录（2026-11-22）

第一批：

- **动效**：删除 `src/App.css` 的 `prefers-reduced-motion` 分支；同步 `docs/design/SPEC.md` §7.4/§8。
- **字体**：Figtree 收为 latin 4 字重，Noto Sans SC 收为 `chinese-simplified` 400/500/600，Cascadia Code 收为 latin 400/600；`--font-mono` 增加 Noto Sans SC 中文回退。结果见 DEC-02。
- **强调色**：恢复天蓝 `--accent` / hover / active / soft / softer / border / bright（对应 `22b506a`）；为满足对比度，`--accent-ink` 取 `#2565C7`、`--on-accent` 取 `#0B1F33`，焦点环改用 `--accent-ink`。
- **移动端**：设置页导航换行；供应商页 drill-down；设置页安全区；触摸设备输入 16px；Composer 触摸 Enter 换行 + 可见禁用原因；Runbar 保留标签；coarse 折叠标题 40px。
- **性能**：`reuseMessages` 按 message id 复用不可变消息；`toolNames` 改 `createMemo`。
- **可访问性/文案**：设置页错误/状态加 `role`；输入补 `aria-label`；记忆时间跟随 app 语言；搜索截断提示；`errorText` 收敛；死 CSS/重复规则清理。
- **第二批**：
  - 设置页 Back 会清理 wizard 残留；跨断点保留草稿；删除按钮移出 sticky footer 并改为内联二次确认；切换供应商/新建时增加未保存草稿守卫，离开设置页（Back/标题栏/Android 返回）也会拦截并确认；保存/测试/删除失败会弹 danger toast；记忆删除武装时图标改为对勾；恢复上次 section；Memory/General 首次访问才挂载；移动端字号与触控命中区提升。
  - 聊天：窄屏/移动端保留模式状态；审批出现时通过 sr-only live region 播报数量；Composer 粗指针检测改为响应式；中文微标签提升到 11px；触摸命中区提升到 44px；中文 strong 使用真实 600 字重。
  - 后端：新增结构化 `risk_level` 字段（`ApprovalSpec` / `ApprovalRequested` / `PendingApprovalView`），前端不再依赖中文风险句；`phi-i18n` 中文标点统一为全角。
  - 工程：Android 硬件返回键接入；根级 `ErrorBoundary`；工作区目录选择器（`tauri-plugin-dialog`）；Settings 页面独立 chunk；`typecheck`/`test`/`check` 脚本；新增 26 个纯函数测试；`tests/` 加入版本控制白名单。
- **第三批（文件拆分与边界收敛）**：
  - `App.tsx` 729 → 382 行；流/事件、模型选择、纯 helper 拆到 `src/app/`。
  - `ProviderEditor.tsx` 1150 → 409 行；连接表单、模型列表、wizard 模型选择、共享表单 helper 拆为独立文件。
  - `Markdown.tsx` 673 → 271 行；纯 parser 拆到 `markdown/parse.ts`。
  - `RichHtml.tsx` 1063 → 372 行；allowlist、属性、SVG、表单、状态拆到 `rich-html/`。
  - `App.css` 3299 → 15 行 barrel；15 个有序模块在 `src/styles/app/`，拼接字节级一致。
  - `Settings.css` 1212 → 10 行 barrel；10 个有序模块在 `src/settings/styles/`，拼接字节级一致。
  - 新增 `src/bridge.ts` 集中命令/事件边界；页面不再直接写 `invoke`/`listen` 字符串。
  - 抽屉增加焦点陷阱与关闭后焦点返还；移动端设置页显示节点存储说明；触摸命中区统一到 44px。
  - 新增 happy-dom + Babel Solid 的 DOM 级渲染测试（9 个），合计 35 个前端测试。
- **第四批（类型、性能、静态检查）**：
  - 用 `ts-rs` 从 Rust DTO 生成 `src/generated/`（34 个 TS 文件）；`bun run types:generate` 可重跑，`src/types.ts` / `src/settings/types.ts` 改为 re-export 生成类型。
  - `Conversation` 接入 `VirtualTranscript`：≤80 行保持原 DOM，超过后按测量高度窗口化；自动滚动、details 状态、键盘焦点保持；流式文本 64ms 合并 + 增量 `splitInlineUi`，避免每 token 全量重解析。
  - 新增 ESLint flat config（TS + Solid + jsx-a11y）与 Prettier；`lint`/`format`/`format:check` 脚本与 CI 步骤；`bun run lint` 0 error（56 个 intentional-pattern warning），`format:check` 通过。

原审查清单的后续项已清空；更深一层的架构项（事件游标/增量投影、provider 规则全部回 Rust、i18n ICU/复数）另行规划。
