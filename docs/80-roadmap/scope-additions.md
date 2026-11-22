# 范围增补记录

> 状态：🌿 · 最后更新：2026-11-22
> 上游：原始设计文档集（2026-11-08 版，`00-overview/` 至 `80-roadmap/` 与 `adr/`）
> 用途：记录**原设计文档里没有、后续实现阶段新增**的能力。设计文档描述目标，本文件记录两者之间的增量；被移除的增补也保留，不抹掉历史。

---

## 0. 判定口径

- 只记录「原本没有的」：原始设计文档未描述的界面、协议、命令或产品能力。
- 实现细节（目录、函数、依赖）只在解释增补时点到为止，不写成实现说明。
- 每条记录：是什么、为什么加、落在哪里、当前状态、对文档的影响。
- 新增增补时先在本文件 §1 加一行，再在 §2 加一节；如果某块内容大到值得独立成篇，另起 `docs/` 新文件并在 `docs/README.md` 登记。

---

## 1. 增补一览

| # | 增补 | 类型 | 状态 | 主要位置 |
|---|---|---|---|---|
| 1 | Intelligent UI | 界面 | **已移除**（先被工作台视图取代，现为内联 HTML 界面块） | 残留代码已删除 |
| 2 | 工作台视图（Workbench view） | 界面 + 协议 | **已移除**（被内联 HTML 界面块取代） | 残留代码已删除 |
| 3 | Provider 管理面 | 设置 / CLI | 已实现（后端、CLI、桌面设置页） | `provider_store.rs`、`model_catalog.rs`、`src/settings/` |
| 4 | 多语言（i18n） | 基础设施 | 已实现（含桌面切换入口） | `phi-i18n`、`src/i18n.ts`、设置 → 通用 |
| 5 | 离线演示模型 | 模型 | **已移除**（2026-11-15，被真实 Provider 与测试夹具取代） | 无（测试用进程内脚本化 HTTP provider） |
| 6 | 无头 CLI | 外壳 | 已实现 | `phi-headless` |
| 7 | 文档增补规则 | 流程 | 已生效 | `AGENT.md` 第 6 条、本文件 |
| 8 | 视觉设计规格 v1 | 界面 | 已实现 | `docs/design/SPEC.md`、`src/App.css` |
| 9 | 记忆 MVP 的落地增量 | 运行时 + 界面 | 已实现（工具、设置页、CLI、golden 集） | `phi-plugins/src/memory/`、`phi-adapters/src/memory_sqlite.rs`、`src/settings/MemoryPage.tsx` |
| 10 | 移除 Run 级预算硬上限 | 运行时 + 界面 | 已移除（保留用量计量与取消） | `loop_plugin.rs`、`src/components/Runbar.tsx` |
| 11 | 模型目录改为可替换数据 | 模型 | 已实现（维护当前模型家族） | `src-tauri/assets/model-catalog.json`、`model_catalog.rs` |
| 12 | 只记 token，不再计价 | 运行时 + 界面 | 已实现（移除全部价格字段） | `phi-contracts`、`model_gateway.rs`、`src/components/Runbar.tsx` |
| 13 | 工具参数损坏与截断的单轮恢复 | 运行时 | 已实现（含无头回归测试） | `model_protocol.rs`、`loop_plugin.rs` |
| 14 | 供应商接入向导 | 设置 / 协议 | 已实现（两步、预览模型、手动勾选） | `src/settings/`、`preview_models` 命令 |
| 15 | 平台原生标题栏与移动端适配 | 界面 | 已实现（Windows/Linux/macOS/Android） | `src/components/Titlebar.tsx`、`src/App.css`、`gen/android/` |
| 16 | 聊天体验重构 | 界面 | 已实现（模型选择、toast、Markdown、流式交接） | `src/components/`、`src/toast.ts` |
| 17 | 内联 HTML 界面块 | 界面 + 提示词 + 状态 | 已实现（取代工作台视图） | `src/components/RichHtml.tsx`、`uiExpression.ts`、`InlineChart.tsx`、`assets/inline-ui-system-prompt.txt` |
| 18 | 动效与字体策略调整 | 界面 / 设计系统 | 已实现（2026-11-22） | `design/SPEC.md`、`src/App.css`、`src/index.tsx`、字体打包 |
| 19 | Matter 事项（取代隐式 Task） | 领域 | 设计中（未实现） | `adr/0012`、`40-runtime/multi-agent.md` §2 |
| 20 | Lead-Sidekick 线程化委派 | 运行时 + 界面 | 设计中（未实现） | `adr/0013`、`40-runtime/multi-agent.md` |
| 21 | 文本辅助模型角色（标题/记忆整理） | 模型 + 运行时 + 界面 | 设计中（未实现） | `adr/0014`、`40-runtime/auxiliary-models.md` |
| 22 | 结构化审批风险等级 | 安全 + 界面 | 已实现（2026-11-22） | `src-tauri/crates/phi-contracts`、`src/status.ts` |
| 23 | 决策模型（Jev 类型化判断） | 模型 + 运行时 + 安全 | 设计中（未实现） | `adr/0015`、`40-runtime/decision-model.md` |
| 24 | 生图独立配置（用户选模型） | 模型 + 界面 | 设计中（未实现） | `adr/0016`、`40-runtime/auxiliary-models.md` §6 |
| 22 | 结构化审批风险等级 | 协议 / 界面 | 已实现（2026-11-22） | `phi-contracts/src/ports.rs`、`phi-domain`、`phi-plugins`、`src/status.ts`、`src/types.ts` |

---

## 2. 逐条记录

### 2.1 Intelligent UI（已移除）

- **是什么**：让模型以 OpenUI 程序作答，桌面端用一个 React 岛屿渲染交互界面；对话消息之外多出一块模型生成的 UI。
- **为什么加**：探索「不只是聊天窗口」的交互方式。
- **为什么移除**：产品定位不是聊天应用；React 岛屿与 Solid 原生外壳的技术选择冲突；模型生成的 UI 代码带来额外的安全与依赖负担。用户明确要求彻底重做，而不是打补丁。
- **移除范围**：`src/OpenUIBlock.tsx`、`src/openui.d.ts`、`scripts/generate-openui-prompt.mjs`、`src-tauri/assets/openui-chat-system-prompt.txt` 全部删除；`package.json` / `bun.lock` 中的 OpenUI 与 React 依赖清零。
- **替代方案**：工作台视图（§2.2）。
- **文档影响**：README 中的 Intelligent UI 章节删除；`AGENT.md` 的对应章节改写为 Workbench view。
- **保留这条记录的原因**：它是一次真实发生的范围变更，后续设计不应无意中走回「模型生成 UI 代码」的老路。

### 2.2 工作台视图（Workbench view）

- **是什么**：主界面改为任务/工作台为中心。Agent 通过 `ui_update` 工具提交一棵声明式视图树，桌面外壳用 Solid 组件注册表渲染在对话旁边；按钮和表单只能把一条消息发回同一会话，或打开 http(s) 链接。
- **与原始设计的区别**：原始文档（ADR-0006 / ADR-0007、`30-extension/plugin-system.md`）里的声明式视图树是给**插件 UI**（D 档）用的；这里扩展为**Agent 驱动的主任务界面**，并补齐了版本化协议与严格校验。
- **新增内容**：
  - 视图树协议 v1：`phi-contracts/src/ui.rs`；节点种类白名单、属性校验、深度/节点数/字节上限、动作白名单。
  - `ui_update` 内置工具（仅 workbench 模式暴露），视图作为 `UiViewUpdated` 事件入库并投影回会话。
  - 按 Run 记录的 workbench 模式开关，以及只在该模式下追加的系统提示词（`assets/workbench-system-prompt.txt`）。
  - Solid 组件注册表（`src/workbench/`），未知组件降级为占位提示，永不执行模型生成的代码。
  - 桌面布局：会话与工作台并排，窄屏自动堆叠。
- **文档影响**：README 新增「A workbench, not a chat window」；`AGENT.md` 新增 Workbench view 章节；原始文档的插件视图树章节不变（两者共存，用途不同）。
- **2026-11-20 移除**：用户要求不再保留独立工作台，界面直接输出到正文流，并用 HTML 表达而不是固定的视图树语法。视图树协议（`phi-contracts/src/ui.rs`）、`ui_update` 工具、`RunView.workbenchMode`、`SessionView.uiView`、运行级 workbench 开关与 `src/workbench/` 全部删除；替代方案见 §2.17。`DomainEvent::UiViewUpdated` 仅保留为反序列化占位，让旧日志可以回放，投影会忽略其中的视图。

### 2.3 Provider 管理面

- **原始设计已有什么**：Provider / ModelSpec 抽象、每 Provider 独立密钥、密钥应走 OS Keychain 或等效加密（`40-runtime/model-gateway.md`）。
- **新增了什么**：
  - 多 Provider 档案的持久化（`providers.json`）：新增、更新、删除、切换当前 Provider。
  - 连接测试与模型列表拉取；自定义 headers、认证模式、定价和模型列表。
  - 凭据引用：`env:NAME`、`literal:VALUE`、`keychain:NAME`；读接口只返回脱敏提示。
  - 桌面命令（`api.rs`）与无头 CLI（`providers list/show/add/update/remove/use/test/models`）两套管理入口。
- **当前状态**：后端、无头 CLI 与桌面设置页（新增/编辑/删除、启用、测试连接、拉取模型、模型角色与自定义请求头）均已接通。
- **与设计约束的偏差**：`keychain:` 适配器尚未实现，使用时会如实报错；`literal:` 会把密钥写入本地档案，属于开发期方案，不是最终形态。
- **2026-11-15 增补**：供应商下的模型从纯字符串升级为结构化 `ModelSpec`（显示名、描述、标签、上下文窗口、输出上限、单价、温度、启用状态、来源）。`model_catalog.rs` 按模型名自动填充这些元数据（可手动覆盖），价格作为保守默认值参与当时的预算预留（该预算机制已于 2026-11-15 移除，见 §2.10）；模型描述与标签为后续模型路由 / 多 Agent 选择提供数据基础。旧 `providers.json` 中的字符串模型继续兼容读取。

### 2.4 多语言（i18n）

- **原始设计**：未提及界面语言。
- **新增内容**：English 语句作为消息 id，中文只存在于翻译目录；Rust 侧 `phi-i18n`（`t` / `t_args`），前端侧 `src/i18n.ts`；无头模式支持 `--lang`；桌面 UI 从 bootstrap 读取语言。
- **当前状态**：现有界面文案均有中英对照；桌面设置 → 通用提供语言切换，切换后节点与所有外壳同步。
- **约束**：源码、注释、标识符为英文；中文只能出现在翻译目录。

### 2.5 离线演示模型（已移除）

- **原始设计**：本地模型列入路线图，但未描述没有 Provider 时的体验。
- **是什么**：内置 mock 模型；未配置任何 Provider 时，它可以跑通模型调用、工具执行与审批流程，并产出示例工作台视图（含 `demo write` / `demo shell` 触发词）。
- **为什么加**：让「无 Key 也能离线体验」成为 README 承诺的一部分，同时保证无头测试不依赖网络。
- **为什么移除**（2026-11-15）：产品此时已有完整的真实 Provider 管理面（多协议、凭据、模型目录）。用固定的演示输出冒充模型结果，会让「未配置 Provider」这一状态对用户不可见，也违背「诚实」原则；测试依赖产品内的 mock 分支，等于没有验证真实协议路径。
- **移除范围**：`model_gateway.rs` 的 `mock_invoke` / `mock_workbench_view`、`ModelStatus` 的 `mock` 标识、mock 专用 i18n 文案与触发词全部删除。未配置 Provider 时，运行如实失败并提示到设置中添加。
- **替代方案**：真实 Provider（Anthropic Messages / OpenAI Chat / OpenAI Responses）；离线场景由本地 OpenAI 兼容运行时承担。无头测试改用测试进程内的脚本化 Anthropic HTTP provider，走真实请求/响应与流式解析路径。
- **文档影响**：README（中英）删除「内置演示模型」承诺；`AGENT.md` 无头示例注明需要 Provider；桌面空状态与设置页文案改为提示添加 Provider。

### 2.6 无头 CLI

- **原始设计**：有 headless 外壳/服务端节点的概念（`10-architecture/module-map.md`、`80-roadmap/evolution.md`），但没有具体命令面。
- **新增内容**：`run`、`sessions`、`status`、`approve`、`cancel`、`bootstrap`、`providers` 等命令；明确的退出码；stdout 上的 JSONL 事件流；跨进程控制文件；数据目录独占锁；只读命令不加锁。
- **为什么加**：作为参考外壳，保证每个共享能力都能脱离桌面 UI 被验证（`AGENT.md` 硬规则）。

### 2.7 文档增补规则

- **新增于**：2026-11-14。
- **内容**：凡是在原始设计之外新增的能力，都必须记录进 `docs/`；默认写入本文件，内容较大时另起新文件并在 `docs/README.md` 登记。README 与 `AGENT.md` 只保留指针，不堆放清单。
- **为什么加**：避免「代码里已经有的东西，文档里查不到」，也避免每次补充都往 README/AGENT.md 里堆。

### 2.8 视觉设计规格 v1

- **是什么**：桌面界面的唯一视觉权威（`docs/design/SPEC.md`）：design tokens、字体与数字排版、间距/圆角/阴影、三栏布局、风险分级审批、runbar 用量条（原为预算条，2026-11-15 改为步数/token/耗时读数，随后不再计价）、workbench 组件与动效语言。
- **新增于**：2026-11-14。
- **落地内容**：
  - 自绘无边框标题栏（窗口控制、拖拽区、节点状态 pill、设置入口）。
  - 风险分级的审批卡（low 单行 / medium 卡片 / high 危险边 + 红色倒计时 + 本次批准约束说明）。
  - 常驻 runbar：步数、token、时间三条读数（早期版本曾为预算 meter，2026-11-15 移除）。
  - 工作台视图新增 `steps` 节点（计划/步骤 stepper），与 `ui_update` 协议 v1 同步；`workbench-system-prompt.txt` 已记录该 kind。
  - 字体本地打包：Figtree、Noto Sans SC、Cascadia Code 经 npm 依赖随构建产物一起分发，不访问 CDN。
  - 设置独立成页：模型供应商管理（完整 model-gateway 面）、工作区与界面语言。
  - 工作台改为 Agent 决定是否使用，输入框不再提供用户开关。
- **与原始设计的区别**：原始文档只定义了多端外壳与工作台协议，没有视觉规格；本规格补齐「长什么样、怎么动」。
- **当前状态**：浅色主题已实现；深色 token 表、品牌 logo 正式图形仍为待办。

### 2.9 记忆 MVP 的落地增量

- **原始设计已有什么**：`20-domain/domain-model.md` §7 与 `40-runtime/context-and-memory.md` 已经定义了 MemoryRecord/Episode、准入、双时间、失效不删、版本链、`forget` 跨存储删除、写入侧脱敏、作用域与可见性分离、操作契约与 golden 集目标。
- **新增于**：2026-11-15（M3）。
- **新增了什么**：
  - **Agent 记忆工具**：`memory_search` 与 `memory_write`。设计只描述了「循环结束时把候选交给 `memory.add`」；MVP 不做额外的 LLM 抽取调用，而是让模型显式写入（来源信任固定为 `model`，同样过准入）。写入参数作为对话/工具记录的一部分存在于会话日志中，与普通对话内容同一保留边界。
  - **记忆设置页**（设置 → 记忆）：搜索、手动添加（user/project 作用域）、查看来源/信任/置信度/状态、永久删除。
  - **无头 CLI**：`memory list/search/show/add/invalidate/correct/supersede/forget/stats`，读命令不加锁，写命令走单写者。
  - **存储与检索实现**：SQLite 事实表 + 自建倒排索引 + BM25；分词器确定性处理拉丁词与中文二元组；Agent 作用域绑定项目、Run 作用域绑定运行。
  - **审计与删除边界**：会话日志只记录不含内容的 `Memory*` 元数据事件；`forget` 删除事实、索引与 Episode，审计事件只保留作用域、计数与原因。
  - **Golden 集**：56 条种子事实、64 条查询（含中英文、作用域隔离、时效与密钥脱敏用例），CI 以非阻塞基线方式运行，见 `phi-plugins/tests/memory_golden.rs`。
- **当前状态**：M3 范围内的 SQLite 事实表、BM25、准入、forget/脱敏与 golden 集均已接通；桌面与无头共用同一 `Node`。
- **与设计约束的偏差 / 待办**：
  - 检索目前只有 BM25 一路，向量通道与 RRF 融合留到下一阶段（设计允许「先无向量」）；跨语言查询因此只能命中同语言事实。
  - 记忆库为本地明文 SQLite；加密仍按 `module-map.md` 的推荐作为后续项。
  - 反思/整理（Reflection）未实现（触发条件与预算见设计 §12 待定）。
  - `forget` 覆盖记忆子系统完整链路；会话对话历史属于另一保留边界，不因记忆删除而改写。
  - 记忆内容随上下文发给已配置的模型供应商，暂未单独接入 `EgressGrant`（与普通对话内容的现状一致）。
- **文档影响**：`README`（中英）新增「可以检查的记忆」章节与路线图条目；`AGENT.md` 新增 Memory 章节；`docs/40-runtime/context-and-memory.md` 的目标设计不变。

---

### 2.10 移除 Run 级预算硬上限

- **原始设计**：`40-runtime/agent-loop.md` §3、`70-quality/quality-attributes.md` QAS-6 与 `20-domain/domain-model.md` 把步数/成本/墙钟三重硬上限写成发行基线的不变量，模型网关在调用前预留、调用后结算。
- **为什么移除**（2026-11-15）：默认 20 步 / $0.50 / 600 秒会中止正常任务——一次 Rust 冷编译或完整测试就可能超过 10 分钟；成本预留用字符/字节估算 token，英文场景高估约 4 倍，$0.50 往往几次调用就耗尽；失败调用也按整笔预留结算，一次限流即可触发中止。普通用户关心的是模型能否完成任务，而不是被默认阈值截断。
- **移除内容**：`RunLimits` 与 per-run 上限、`Bootstrap/RunStatus/RunView/RunScheduled` 的 max 字段、`BudgetReserved/BudgetSettled/BudgetExceeded` 事件、`Ledger` 预留/结算、`ModelPort::estimate_cost`、CLI 的 `--max-steps/--max-cost/--max-wall`、Runbar 的三条 meter 与 Composer 的预算提示。
- **保留内容**：每次调用的 `max_output_tokens`；`shell_exec` 的单命令 timeout；审批自身的 `PHI_APPROVAL_TIMEOUT_SECS`；用户取消；`ModelCallReturned` 的 token 记录，Runbar 改为「步数 / token / 耗时」读数（成本记录随后在 §2.12 移除）。
- **运行终止条件**：模型不再调用工具；连续三次被输出上限截断；用户取消；模型请求失败且不可重试。
- **设计记录**：预留/结算与三重上限的设计文本保留在 `agent-loop.md`、`model-gateway.md`、`quality-attributes.md`，标注为历史/可选目标态；如果未来重新引入，必须是显式 opt-in，而不是默认闸门。
- **文档影响**：README（中英）、SECURITY、AGENT.md 的 Runtime loop 一节、`design/SPEC.md` 的 Runbar 规范同步更新。

### 2.11 模型目录改为可替换数据

- **原状**：`model_catalog.rs` 内置一份按模型名匹配的名称/描述/标签/上下文/价格表，设置页 presets 也硬编码了模型 id。
- **为什么改**（2026-11-15）：这些条目已经过时，继续自动填充会误导用户；用户将提供最新的模型清单。
- **当前实现**：目录数据移到 `src-tauri/assets/model-catalog.json`，编译进二进制。目录只维护**当前**模型家族（GPT-6、Claude 5.x / Fable、GLM-5.x、DeepSeek-V4、Kimi-K3、Gemini 4）；未知模型保持 manual spec，只填 id；provider 的模型列表拉取仍是实时来源。匹配用家族 `patterns` 加变体 `requires`，`display` + `anchor` 从 id 解析版本，因此未记录的小版本（如 `gpt-6.5-luna`）显示为 `GPT-6.5 Luna`，Fable 5 / 5.1 / 5.2 共用一条记录。当前模型都支持思考强度，不再维护 `reasoning` 标记与旧模型推断列表。`scale` 为 1–5 的模型规模（Fable / Astra / Carbon 为 5）；目录没有通用回退条目，未记录的变体按 manual spec 处理。规模用于限制子代理调用比自己更大的模型。厂商发布新模型时同步更新本文件。
- **数据字段**：`patterns`（任一命中的大小写不敏感子串，匹配时去掉 vendor 前缀）、`requires`（必须同时命中的变体标记）、`name`（版本解析失败时的回退显示名）、`display`（`{version}` 模板）、`anchor`（版本数字起点）、`description`、`tags`、`contextWindow`、`maxOutputTokens`、`scale`（1–5，缺省 3；`pricing` 已在 §2.12 移除）。
- **文档影响**：`40-runtime/model-gateway.md` 增加目录数据说明；`scope-additions` 记录本变更。

### 2.12 只记 token，不再计价

- **原状**：Provider、`ModelSpec`、模型目录与 Runbar 都有每百万 token 价格；每次调用按价格快照计算 `cost_usd`，写入 `ModelCallReturned`、`RunView` 与 Runbar。
- **为什么改**（2026-11-15）：价格随供应商、缓存与促销频繁变化，内置价格容易过时并误导用户；用户明确要求「去掉定价，只记 token」。token 用量是客观事实，价格不是。
- **移除内容**：`Pricing` 类型及所有 `pricing` / `clearPricing` 字段、`Protocol::default_pricing`、网关的 `compute_cost`、`ModelResponse.cost_usd`、`RunStatus/RunView/ModelCallReturned.cost_usd`、无头 CLI 的 `--pricing-*` 参数与成本输出、设置页全部价格输入、模型目录的 `pricing` 字段。
- **保留内容**：输入/输出 token、缓存读取/写入 token、`estimated_usage`；Runbar 显示输入/输出 token。旧日志里的 `costUsd` 字段在反序列化时被忽略，历史会话仍可打开。
- **当前状态**：桌面与无头共用同一契约；无头 `run` 汇总输出 `tokens_in` / `tokens_out`。
- **文档影响**：README（中英）、SECURITY、AGENT.md、`40-runtime/model-gateway.md`、`70-quality/*`、`design/SPEC.md` 的 Runbar 规范同步更新。

### 2.13 工具参数损坏与截断的单轮恢复

- **原状**：模型流式返回的工具参数只要不是合法 JSON，整个模型响应解析失败，运行终止，已流式输出的助手文本也不会落盘；`length`/`max_tokens` 截断同样立即终止整轮。
- **为什么改**（2026-11-15）：真实模型会偶发输出损坏的 JSON 或被输出上限截断；一次工具参数解析失败不应吞掉已有输出，更不应结束整个会话。用户明确报告「工具参数不是合法 JSON」导致输出被吞。
- **当前实现**：
  - 参数解析永不使整个响应失败：先直接解析，失败后剥离 Markdown 代码围栏并提取最外层平衡的 `{...}`/`[...]`，仍失败则记录 `InvalidToolCall { id, name, rawArguments, error }`，工具块保留（`input` 为空对象）以维持协议历史有效。
  - 循环对非法调用不执行工具，写入带工具名、解析错误与截断原文的成对 `tool_result` 错误，模型可重新发起；同时发 `ToolInvokeFailed` 审计事件。同一运行累计 5 次非法参数后以明确错误终止，避免模型无限重试。
  - 模型调用失败并重试时，被放弃那次的流式文本会从界面移除，避免与重试输出重复；最终失败仍保留已生成的文本。
  - 截断恢复：截断的工具调用不执行，回写成对错误；纯文本截断通过一条**仅注入上下文、不写入会话**的续写指令继续生成。最多恢复 3 次，之后以指明输出上限的错误终止。
  - 模型调用最终失败但已流式输出文本时，把该部分文本作为助手消息持久化，避免输出被吞。
- **测试**：`phi-headless/tests/headless.rs` 覆盖非法参数不执行且恢复、截断工具调用、纯文本续写不出现为人类消息、恢复次数上限；`model_protocol.rs` 覆盖三协议解析与围栏恢复。
- **文档影响**：`AGENT.md` Runtime loop、`40-runtime/model-gateway.md` §6、`40-runtime/agent-loop.md` 的终止条件。

### 2.14 供应商接入向导

- **原状**：新建供应商先展示五张预设卡片；表单同时要求协议/认证/供应商默认值/模型角色/价格；拉取模型会立刻把所有模型写盘，用户无法先选择。
- **为什么改**（2026-11-15）：预设卡片与实际字段重复，价格与默认值过时，模型角色对普通用户是噪音；用户要求先填写连接信息，再自动拉取并手动勾选模型。
- **当前实现**：
  - 删除 `src/settings/presets.ts`、模型角色、供应商默认值与全部价格输入。
  - 两步向导：第一步名称、备注（单行）、官网、API Key、请求地址、请求格式（OpenAI Chat 默认）；认证由协议与密钥推导（空密钥 = 无认证，否则 Anthropic 用 x-api-key、OpenAI 用 Bearer），不再单独选择。
  - 第二步自动调用新增的 `preview_models`（只预览、不落盘），展示全部模型、大小写不敏感的去重与目录映射结果（`Mapped` / `Defaults`），支持搜索、全选/全不选、手动补充；只保存用户勾选的模型，主模型取第一个勾选项；不再有「启用哪个模型」的开关。
  - 编辑页保留连接字段、模型列表与「高级」折叠区（自定义请求头、清除密钥）；未完成的草稿在取消/切换时删除，避免残留。
- **当前状态**：后端新增 `preview_models` Tauri 命令；`fetch_models` 保留给 CLI/兼容用途（仍会落盘）。
- **文档影响**：README（中英）「自带模型」、`40-runtime/model-gateway.md` §5.1。

### 2.15 平台原生标题栏与移动端适配

- **原状**：所有平台都渲染 macOS 红绿灯；窗口最小宽度 760；没有移动端断点、安全区或抽屉侧栏；Android 主题是 DayNight，状态栏图标在浅色界面上不可读。
- **为什么改**（2026-11-15）：项目以 Windows 与 Android 优先，标题栏与交互必须符合平台预期；用户报告「没有移动端适配」。
- **当前实现**：
  - `Bootstrap.platform` 暴露宿主平台。Windows/Linux 使用右侧标准最小化/最大化/关闭按钮（46×32 命中区、关闭红色悬停）；macOS 保留左侧红绿灯；Android/iOS 使用无窗口控件的应用栏（抽屉按钮、标题、设置）。
  - 响应式：≤900px 侧栏变为带遮罩的抽屉，会话选择/新建后自动关闭，Esc 关闭；设置页导航在窄屏变为横向标签；输入区、Workbench、审批卡与工具栏重排；触摸设备命中区 ≥40px；`viewport-fit=cover` 与 `env(safe-area-inset-*)` 覆盖刘海/手势区域。
  - Android：edge-to-edge 透明状态栏/导航栏、强制浅色图标、`adjustResize` 避免键盘遮挡输入框；主题固定为浅色（v1 无深色模式）。
- **文档影响**：README 快速开始/体验描述；`design/SPEC.md` 的标题栏与响应式规范。

### 2.16 聊天体验重构

- **原状**：输入框没有模型选择；错误以卡片堆叠在消息流顶部；助手回复是纯文本；流式文本在 `done`/终止事件后被定时清空，而持久消息尚未刷新，造成闪烁/空白；工具失败只显示通用「工具错误」；Runbar 显示价格。
- **为什么改**（2026-11-15）：用户要求模型选择器、悬浮思考强度子菜单、Markdown 渲染、通知而不是卡片堆叠，并报告输出完成时空白闪烁、工具失败看不出是哪一个。
- **当前实现**：
  - Composer 左下角模型选择器：列出当前供应商启用的模型；桌面悬浮模型弹出思考强度子菜单（off/low/medium/high），触摸设备内联展开；选择按供应商写入 localStorage；发送时携带 `model` 与 `thinking`。当前模型都支持思考强度，因此不再维护「是否支持推理」标记，子菜单对所有模型可用。
  - 通知：`src/toast.ts` + `Toasts.tsx`，按消息去重、最多 4 条、6 秒自动消失；全局错误走 toast，表单错误留在原地，运行失败只保留一条可展开、可复制的紧凑提示。
  - Markdown：安全 JSX 渲染（标题、段落、列表、引用、表格、链接、行内代码、代码块与复制按钮），从不注入 HTML；流式输出同样渲染。
  - 流式交接：`done` 不再清空文本；仅在刷新后的持久助手消息与流式文本匹配时一次性交接，终止后 2 秒兜底刷新；无法匹配时保留文本并标记「Stopped」，不再出现空白。
  - 工具错误：从配对的 `tool_use` 解析工具名，显示「Tool failed: <name>」、可展开正文与复制按钮。
  - Runbar：步数 / token（输入↑ 输出↓）/ 耗时，不再显示成本。
- **文档影响**：README（中英）、`design/SPEC.md`、`AGENT.md`。

### 2.17 内联 HTML 界面块（HTML + 状态/表达式）

- **原状**：Agent 通过 `ui_update` 把声明式视图树写到对话旁的工作台面板；用户要求去掉工作台，界面直接出现在回复正文里。
- **参考**：本地参考项目 `references/open-intelligent-ui`（Open Intelligent UI，基于 OpenUI Lang；https://github.com/thesysdev/open-intelligent-ui）展示了足够强的目标：模型编排组件、输入绑定状态、页面内计算、图表/表单/动作。它本身是一门新语言，模型驾驭成本高；本实现保留它的**能力模型**（组件词汇、状态、表达式、动作），但把表面语言换成模型最熟悉的 HTML。
- **当前实现**：
  - 每次运行的模型调用都追加 `src-tauri/assets/inline-ui-system-prompt.txt`：引导模型在回复中用围栏代码块（语言标记 `html`）写 HTML，并只在需要时叠加 `data-ui-*` 与 `{{ }}`；没有按消息或按运行的用户开关。
  - 状态：`<script type="application/json" data-ui-state>` 数据岛，`JSON.parse` 读取，永不执行；每次渲染按块源码缓存，刷新会话时保留用户在块内的选择（进程内）。
  - 表达式：`src/components/uiExpression.ts` 使用固定文法（算术、比较、三元、成员/索引、白名单函数）求值，**不使用 `eval` / `new Function`**；`items.amount` 这类数组取字段、`filter/sort/toggle/sum` 等函数让页面内计算成为可能。
  - 交互：`data-ui-bind` 双向绑定 input/select/textarea；`data-ui-set` 修改状态；`data-ui-send` / `data-ui-copy` / 表单提交沿用会话输入 owner，一次只允许一个运行；`data-ui-show` 条件显示；`<template data-ui-each>` 重复标记（≤200 行）。
  - 组件：宿主 CSS 提供 `ui-grid`、`ui-card`、`ui-callout`、`ui-badge`、`ui-metrics`、`ui-steps`、`ui-kv`、`ui-chips`、`ui-num` 等词汇；`table.ui-chart` 由 `InlineChart.tsx` 绘制 bar/line/area，并提供无障碍表格；原生 `details`、`progress`、`meter`、表单控件与图片（仅对话中出现过的 https 地址）都可用；SVG 仅基础形状/文本。
  - 安全：`RichHtml.tsx` 用 inert `DOMParser` 解析后按标签、class、属性白名单重建；脚本（数据岛除外）、样式、iframe、对象、事件属性、`style` 全部丢弃；链接仅 `http(s)`/`mailto`；图片仅 `https`；表达式错误只显示原文或空值，不执行。
  - 流式：`inlineUi.ts` 把文本切成正文与 html 围栏，围栏内容边流边渲染；持久消息由 Markdown 渲染器统一处理。
- **能力差异**：地图、图片搜索、日历、可编辑表格、沙盒 HTML/JS（`HtmlView`）与 OpenUI 的完整组件集未实现；没有模型编写的 JavaScript。需要用户选择时用表单或 `data-ui-send` 按钮；数据用表格、指标或图表表达。安全承诺不变：永不执行或注入模型生成的代码，也永不注入原始 HTML 字符串。
- **文档影响**：README（中英）、`AGENT.md`、`design/SPEC.md`、`20-domain/domain-model.md`。

### 2.18 动效与字体策略调整（2026-11-22）

- **原状**：SPEC §7.4/§8 要求尊重 `prefers-reduced-motion`；字体按 Noto Sans SC 四个字重 × 101 个 `unicode-range` 子集打包，`dist/assets` 约 12MB、CSS 约 453KB。
- **为什么改**：用户判断动效的性能开销几乎可以忽略，保留动效可提升体验；字体包体积明显偏大，且中文微字号/字体回退存在不一致。
- **当前实现**：
  - 删除 `src/App.css` 的 `prefers-reduced-motion` 降级，动效在所有系统设置下保持启用；SPEC §7.4/§8 同步。
  - Figtree 使用 latin 400/500/600/700；Noto Sans SC 使用 `chinese-simplified` 400/500/600 单子集；Cascadia Code 使用 latin 400/600；`--font-mono` 增加 Noto Sans SC 作为中文回退。
  - 结果：`dist/assets` 约 3.7MB，字体约 3.4MB / 11 个 woff2（含 Figtree 两个斜体），CSS 约 61KB。
  - 恢复视觉规格中的天蓝强调色 token（`#5B9DF8` 系列）；为满足对比度，`--on-accent` 使用深色 `#0B1F33`、`--accent-ink` 使用 `#2565C7`，焦点环改用 accent-ink。
  - 设置页工作区增加目录选择器（`tauri-plugin-dialog`），不再要求手机用户手输绝对路径。
- **取舍**：放弃系统级减少动效适配；`chinese-simplified` 单子集不覆盖繁体/生僻字，回退系统 CJK 字体。
- **文档影响**：`design/SPEC.md` §2.1/§2.2/§7.4/§8；完整审查与后续项见 `docs/70-quality/frontend-review-2026-11-20.md`。

### 2.19 Matter 事项（取代隐式 Task）

- **原状**：`Task` 是 ADR-0008 引入的顶层目标，但实现只是在每个 Run 调度时惰性创建一个隐藏 Task；没有聚合状态、真实标题、任务树，前端只有 `taskId` 字段。效果上接近「一个会话 / 一轮 = 一个 Task」。
- **为什么改**（2026-11-22）：用户明确不希望 Task 绑定到单个 Session / Turn；接下来要做超级 Agent，需要一个跨会话、跨执行的长期目标单位。调研 Varin 后，采用「事项（matter）」语义：保存理解和关联，不保存实时状态，Run 结束不自动完成。
- **设计**：Matter 是可选聚合，跨 Session / Turn / Run；`Run.matterId` 可选；取消每个 Run 惰性创建 Task；`Task` / `TaskCreated` / `taskId` 直接删除，不保留兼容。详见 ADR-0012 与 `40-runtime/multi-agent.md` §2。
- **当前状态**：设计已定，未实现；Matter 看板、日程、主动跟进属于超级 Agent 阶段。
- **文档影响**：`20-domain/domain-model.md` §3.0（改写为 Matter）、`00-overview/glossary.md`、ADR-0012、ADR-0008 状态。

### 2.20 Lead-Sidekick 线程化委派

- **原状**：单 Agent 运行时；一个 Session 同时只能有一个 Run；没有子 Agent、委派或 Agent 间通信。
- **为什么加**（2026-11-22）：用户偏好 Fusion 的 Lead-Sidekick：规划与执行分离，Lead 是唯一用户接口，Sidekick 用更便宜的模型执行；多个 Sidekick 可并行，但不允许嵌套，Sidekick 之间可以定向交流。相比图编排，这种形态更符合当前单写者、审批绑定、诚实取消的约束。
- **设计**：
  - 同一个 Session 内，根 Run（Lead）+ 子 Run（Sidekick），子 Run 带 `parentRunId`，共享 Turn 与 Matter，不新建用户轮次。
  - 消息按线程隔离：`main` 与 `agent:<id>`，Lead 不读 Sidekick 轨迹，只收 brief / 报告。
  - 每次委派创建持久 `AgentInstance`，Lead 在 `delegate` 时选择模型（scale 不得越级，pinned，不静默回退）。
  - 多 Sidekick、扇出上限、Sidekick 之间 `send_message` 定向通信；Sidekick 没有 `delegate`。
  - 人仍是唯一审批者；`shared` / `read_only` 工作模式，同一时刻只有一个 shared writer；`isolated_write` 后续再做。
  - 报告区分 completed / delivered / read，失败 / 取消 / 部分结果同样形成报告。
- **当前状态**：设计已定，未实现；按 `40-runtime/multi-agent.md` §10 分期落地，headless 先行。
- **文档影响**：`40-runtime/multi-agent.md`、ADR-0013、`40-runtime/agent-loop.md` §7、`00-overview/glossary.md`；后续实现时更新 `AGENT.md`。

### 2.21 文本辅助模型角色（标题 / 记忆整理）

- **原状**：`40-runtime/model-gateway.md` §8 把辅助模型定义为审批 / 分类 / 抽取的受限调用，但代码没有角色绑定；会话标题固定为 `New session`；记忆没有后台整理。
- **为什么加**（2026-11-22）：用户要求支持会话标题生成、记忆后台整理等辅助模型用途。如果每个功能各自读配置、各自打 provider API，设置、外发授权、用量和审计会散落；如果做成 Agent / Run，又会污染 Turn / Run 语义并引出审批递归。
- **设计**：
  - 文本辅助角色按应用级绑定：`session_title`、`memory_maintenance`、`approval_judge`（后续）、`summarizer` / `embedding`（后续）。
  - 辅助调用是**服务调用 / 后台任务**，不创建 Turn / Run / Step / AgentInstance；有独立审计与用量事件。
  - 未绑定、能力不匹配或缺少外发授权时功能明确禁用，不回退到其他模型。
  - 标题在首条助手回复后生成，只覆盖默认标题，用户改名后不再自动覆盖。
  - 记忆整理按写入阈值 / 空闲 / 显式命令触发，提案过后台准入屏障；安全操作可自动应用，纠正 / 失效待确认，`forget` 永不自动。
- **当前状态**：设计已定，未实现；先做角色配置、标题与记忆整理。
- **文档影响**：`40-runtime/auxiliary-models.md`、ADR-0014、`40-runtime/model-gateway.md` §8、`00-overview/glossary.md`；后续实现时更新 `AGENT.md` 与设置页文档。

### 2.22 结构化审批风险等级（2026-11-22）

- **原状**：审批的视觉等级由前端从 `toolId` 和本地化风险句推断；`src/status.ts` 里还包含中文 `"不可逆"` / `"可逆"` 字面量，违反英文源码规则，也无法适配其他语言。
- **为什么改**：风险等级是安全语义，权威应在内核；前端只应渲染。
- **当前实现**：
  - `ApprovalSpec`、`ApprovalRequested`（`#[serde(default)]`，兼容旧 journal）和 `PendingApprovalView` 增加 `risk_level`。
  - `loop_plugin` 对 `shell_exec` 写 `"high"`，其余审批请求写 `"medium"`；本地化 `risk` 句保持不变。
  - 前端 `PendingApproval.riskLevel` 优先使用该字段；旧 journal 缺失时回退到 tool id 与英文风险句，不再包含中文 literal。
- **文档影响**：`40-runtime/policy-and-approval.md`（如描述审批 DTO/事件）与 `src/types.ts` 注释。

### 2.23 决策模型（Jev 类型化判断）

- **原状**：审批弹窗全部由人处理；长工具输出直接截断或整段塞进上下文；没有可校准的封闭式判断层。
- **为什么加**（2026-11-22）：用户要求接入 Jev 决策模型（BYOK，端点自己填写），用于长工具调用精简、审批弹窗减负等判断点。Jev 不生成文本，而是接收状态和类型化问题（`noul` / `choice` / `score`），返回带校准概率的结构化判定。
- **设计**：
  - 独立 `decision.evaluate` 端口与 `jev_decisions` 协议；`decision.json` 配置端点、Key、模型、超时。
  - 决策点单独配置阈值：`approval_triage`（审批分流）、`tool_output_compaction`（输出压缩策略）；后续可加循环终止 / 路由。
  - 审批分流只在用户预授权的低风险类别内自动放行；高风险 / 不可逆 / 未知风险永远人工；失败 / 低置信度回到人。
  - 长工具输出由决策模型选择策略，真正的摘要由文本 `summarizer` 或确定性截断完成；完整输出保留在 Blob。
  - 事件、审计、撤销、外发授权与 headless 入口齐全。
- **当前状态**：设计已定，未实现；先做审批分流与输出压缩两个点。
- **文档影响**：`40-runtime/decision-model.md`、ADR-0015、`40-runtime/policy-and-approval.md`、`40-runtime/model-gateway.md` §8、`00-overview/glossary.md`。

### 2.24 生图独立配置（用户选模型）

- **原状**：没有生图能力；聊天 provider 配置只覆盖文本协议。
- **为什么加**（2026-11-22）：用户明确要求生图单独配置，模型由用户选择而非 Agent 选择，并且由 Agent 主动调用工具。
- **设计**：
  - 独立 `image.json`（provider、baseUrl、Key、用户选择的模型、默认参数、上限、外发授权），不复用聊天 provider 或 `auxiliary.json`。
  - 可选 `image.generate` 端口；`image_generate` 工具没有 `model` 参数，运行时注入用户配置的模型；Agent 只决定何时生成与提示词。
  - Agent 主动调用，不是后台任务；默认只给 Lead，是否给 Sidekick 由配置决定。
  - 本地内容寻址资产库；PNG / JPEG / WebP / GIF 可渲染，SVG 只下载不内联；校验 MIME / 尺寸 / 文件大小；提示词外发需 `EgressGrant`。
  - 独立图片模型目录记录能力（4K、蒙版编辑、参考图、GIF / SVG、速度档位），不硬编码未公开分辨率。
- **当前状态**：设计已定，未实现。
- **文档影响**：`40-runtime/auxiliary-models.md` §6、ADR-0016、`00-overview/glossary.md`；后续实现时更新 `AGENT.md` 与设置页文档。

---

## 3. 维护约定

- 新增原设计之外的能力：先在 §1 加一行，再在 §2 加一节。
- 能力被移除或替换：更新状态并保留记录，写明原因和替代方案。
- 本文件记录的是**增量**；当前实现状态与缺口以代码和测试为准，需要时另行成文。
