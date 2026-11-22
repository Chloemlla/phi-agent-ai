# phi-agent 视觉设计规格

> 版本 v1 · 2026-11-14 · 配套原型 [`visual-anchor.html`](./visual-anchor.html)
>
> 本文档是桌面 UI 的唯一视觉权威来源。它定义 design tokens、组件、
> 状态语义、安全分级与动效语言。**所有人类可见文案仍走 i18n**
> （见 `AGENT.md`），本文只规定"长什么样、怎么动"，不规定文案内容。

---

## 0. 设计原则（决策的仲裁尺）

| 原则 | 含义 | 落到视觉上 |
|---|---|---|
| **通透** | 界面不该有压迫感 | 浅色为主、低对比 hairline 边框、几乎不可察觉的阴影、充足留白 |
| **秩序** | 信息有清晰的层级与对齐 | 8px 间距栅格、统一圆角、3~4 级字体层级、等宽数字承载一切数据 |
| **克制** | 颜色与动效都是稀缺资源 | 单一强调色；语义色只在状态上出现；动效只在"状态改变"时发生 |
| **安全可见** | 危险必须在视觉上更重 | 审批按风险分级，视觉重量与风险成正比（见 §5） |
| **诚实** | UI 永远反映真实状态 | 不假装成功；"outcome unknown" 就是 unknown；动效不掩盖延迟 |

> 当两个方案冲突时，用这张表仲裁：**通透与诚实优先于美观**。

---

## 1. 色彩

### 1.1 中性色（浅色系 · 冷白纸感）

| Token | 值 | 用途 |
|---|---|---|
| `--bg-app` | `#F7F9FB` | 应用画布 |
| `--bg-surface` | `#FFFFFF` | 卡片、面板、composer |
| `--bg-sidebar` | `#FBFCFE` | 侧栏（与画布微区分） |
| `--bg-subtle` | `#F2F5F8` | hover / 内嵌 |
| `--bg-inset` | `#EDF1F6` | 代码块、命令预览 |

### 1.2 边框（hairline，低对比）

| Token | 值 | 用途 |
|---|---|---|
| `--border-softer` | `#EFF2F6` | 最弱分隔（卡片内、列表间） |
| `--border-soft` | `#E6EBF0` | 常规边框 |
| `--border-strong` | `#D6DEE6` | 输入框、hover 加强 |

> 规则：**优先用留白分隔，其次 hairline，最后才是色块。** 禁止粗黑分割线。

### 1.3 文本

| Token | 值 | 用途 |
|---|---|---|
| `--text-primary` | `#1B2532` | 正文、标题 |
| `--text-secondary` | `#54627A` | 次要信息 |
| `--text-muted` | `#8A97A8` | 辅助、时间戳 |
| `--text-faint` | `#AEB9C7` | placeholder、最弱提示 |

### 1.4 强调色 —— 标志浅蓝

> 性格：干净的天空蓝，**不偏绿、不深**。它是界面里唯一"高饱和"的颜色，因此极具指向性。

| Token | 值 | 用途 |
|---|---|---|
| `--accent` | `#5B9DF8` | 主按钮、激活态、链接 |
| `--accent-hover` | `#4A90F5` | hover |
| `--accent-active` | `#3D84EF` | pressed |
| `--accent-soft` | `#EEF4FE` | 着色底（tag、选中） |
| `--accent-softer` | `#F6FAFF` | 最弱着色（空状态图标底） |
| `--accent-border` | `#CFE2FC` | 强调边框 |
| `--accent-ink` | `#2565C7` | 浅底上的强调文字、焦点环 |
| `--on-accent` | `#0B1F33` | accent 填充上的文字（深色，保证对比度） |

> 规则：**一屏只让一个元素"最亮"。** 强调色用于"当前最重要的那一个动作/状态"，不要大面积平铺。天蓝色上不用白字：白字对比度只有 2.75:1，主按钮与强调填充统一使用 `--on-accent` 深色文字。

### 1.5 语义状态色（克制、去饱和）

状态色比强调色更淡，只在"状态"上出现，不用于装饰。

| 语义 | 主色 | soft 底 | border | 用于 |
|---|---|---|---|---|
| ok | `#2FA96C` | `#EBF6F0` | `#C6E8D5` | 成功、Normal、已完成步骤 |
| warn | `#CE911F` | `#FBF4E4` | `#EFD CAB`（见注） | 需复核、中等风险 |
| danger | `#DA5B4E` | `#FCEFED` | `#F4CCC7` | 失败、高危、取消 |
| info | 复用 accent | — | — | 信息提示 |

> 注：warn border 实际值 `--warn-border: #EED9A8`。
> 规则：**状态色永远成对出现**（主色 + soft 底 + border），单独一个色块太硬。

### 1.6 风险分级色（专属于审批）

| 等级 | 色 | 语义 |
|---|---|---|
| low | `--risk-low: #54627A` | 只读、可逆 → 接近中性 |
| medium | `--risk-medium: #CE911F` | 写入、可逆 → 琥珀 |
| high | `--risk-high: #DA5B4E` | 执行/不可逆/出边界 → 红 |

### 1.7 深色模式（v1 不做，但预留）

- v1 **只交付浅色**，把通透做到极致。
- 所有颜色必须走 token，**禁止硬编码 hex**，这样深色模式 = 替换 token 表。
- 深色方向预留：底 `#15181D`、表面 `#1C2026`、强调色提亮到 `#7AB3FF`。

---

## 2. 字体与排版

### 2.1 字族

| Token | 字族栈 | 用途 |
|---|---|---|
| `--font-ui` | `"Figtree", "Noto Sans SC", system-ui, sans-serif` | 界面：英文走 **Figtree**，中文走**思源黑体（Noto Sans SC）** |
| `--font-mono` | `"Cascadia Code", "Noto Sans SC", "JetBrains Mono", Consolas, monospace` | **一切数据**：数字、命令、digest、token 数、路径、代码（微软 **Cascadia Code**；中文回退思源黑体，避免落进系统宋体） |

> 匹配逻辑：Figtree 无中文字形，中文自动落到 Noto Sans SC；西文与中文混排时
> 各自使用自己的字体，字号一致、视觉重量对齐（二者 x-height 接近，混排不跳）。

### 2.2 字体打包（local-first 的硬约束）

phi-agent 离线可用，**三个字体都必须作为 webfont 打包进应用资产**，禁止运行时走 CDN，也禁止假设系统已预装（Cascadia Code 仅在装有 Windows Terminal / VS 的机器上存在）。

- 目录：`src/assets/fonts/`（Tauri 打包进 resources，与现有 `src/assets/` 同级约定）。
- 格式：**woff2**。思源黑体使用 `chinese-simplified` 单子集文件，避免每个字重打包 101 个 `unicode-range` 分片造成的体积膨胀。
- 字重：Figtree 400 / 500 / 600 / 700（含 400 / 600 italic）；Noto Sans SC 400 / 500 / 600；Cascadia Code 400 / 600。中文强调使用着重号（text-emphasis），不使用合成斜体。
- 用 `@font-face` 声明，`font-display: swap`，`font-family` 名与 token 表一致。
- 许可：三者均为 OFL / 宽松许可，可随 GPL-3.0 项目分发（保留 LICENSE 文件）。

```css
/* 示例 */
@font-face {
  font-family: "Figtree";
  src: url("assets/fonts/figtree-500.woff2") format("woff2");
  font-weight: 500; font-style: normal; font-display: swap;
}
```

### 2.3 字级（仅 5 级，不多设）

| 名称 | 字号 / 行高 / 字重 | 用途 |
|---|---|---|
| Display | 15 / 1.4 / 600 | 面板标题、会话标题（少用） |
| Body | 14 / 1.6~1.7 / 400 | 对话正文 |
| Label | 13 / 1.45 / 500 | 按钮、表单、卡片标题 |
| Meta | 12 / 1.5 / 400~500 | 时间戳、辅助信息 |
| Micro | 11 / 1.4 / 600 · 字距 0.05em · 大写 | 分区标签、风险标签 |

### 2.4 数字排版（秩序感的核心）

- **所有数字、金额、步数、token、digest、时间一律用 `--font-mono`（Cascadia Code）**。
- 金额格式 `$0.012`；token 用千分位 `4,812`。
- 倒计时用等宽 `4:32`，禁止跳动时宽度变化（Cascadia Code 数字等宽，保证对齐）。

---

## 3. 间距、圆角、阴影

### 3.1 间距（8px 基准，呼吸感来自"偏大"）

| Token | 值 | 典型用途 |
|---|---|---|
| `--sp-1` | 4 | 图标与文字间 |
| `--sp-2` | 8 | 紧凑元素间 |
| `--sp-3` | 12 | 卡片内边距（小） |
| `--sp-4` | 16 | 卡片内边距、组件间 |
| `--sp-5` | 24 | 区块间、页边距 |
| `--sp-6` | 32 | 大区块间 |
| `--sp-7` | 48 | 屏级留白 |

> 规则：**宁大勿小。** 拥挤是通透的最大敌人。

### 3.2 圆角（统一，友好但不锐利）

| Token | 值 | 用途 |
|---|---|---|
| `--r-sm` | 6 | 小元素、tag |
| `--r-md` | 8 | 按钮、输入框、小卡片 |
| `--r-lg` | 12 | 卡片、审批卡、composer |
| `--r-xl` | 16 | 大型容器 |
| 99px | 全圆 | pill、头像、状态点 |

> 规则：**同一层级的元素用同一圆角。** 嵌套时外层圆角 > 内层。

### 3.3 阴影（几乎不可察觉，分层柔软）

| Token | 值 | 用途 |
|---|---|---|
| `--shadow-xs` | `0 1px 2px rgba(27,37,50,.04)` | 静态卡片 |
| `--shadow-sm` | `0 1px 3px .05, 0 1px 2px .03` | 浮起卡片 |
| `--shadow-md` | `0 4px 12px .06, 0 2px 4px .04` | composer、弹层 |
| `--shadow-lg` | `0 12px 32px .10, 0 4px 8px .05` | 模态、popover |
| `--shadow-pop` | `0 8px 24px rgba(91,157,248,.10), ...` | **主按钮专属**（带蓝光） |

> 规则：浅色主题里阴影要"感觉有，看不见"。**主按钮用 `--shadow-pop`（带强调色光晕），是它"最重要"的视觉凭证。**

---

## 4. 布局结构

```
┌────────────────────────────────────────────────┐
│ Titlebar（自绘，frameless，44px）               │
├──────────┬─────────────────────────────────────┤
│          │                                     │
│ Sidebar  │   Conversation (flex, max 720px)    │
│ 264px    │                                     │
│ 可收起    │─────────────────────────────────────│
│          │ Composer（输入 + 上下文圆环 + 停止） │
└──────────┴─────────────────────────────────────┘
```

### 4.1 Titlebar（自绘，不用系统默认）

- 高 `44px`，`-webkit-app-region: drag`；交互元素 `no-drag`。
- 左：窗口控制点（12px 圆点，hover 才显色）+ brand（logo + 字标）。
- 中：当前会话标题 + 工作区路径（mono，muted）。
- 右：节点状态 pill + 设置 + 侧栏开关。
- 底边 `1px --border-softer`。

### 4.2 Sidebar（可收起）

- 宽 `264px`，收起 = `margin-left: -264px` + `--ease` 缓动。
- 内容：分区标签 + 新建按钮（`+`）→ 会话列表 → 底部 node 信息。
- 会话项：**单行标题** + 悬停/键盘聚焦才出现的删除按钮；状态、消息数、最后活动时间放进悬停的 Alt 气泡。
- 悬停：透明浅灰底（`--bg-subtle`），**不加边框**。
- **激活态**：`--accent-soft` 浅蓝底，不再使用左侧 3px 指示条。
- 删除：两步内联确认（第一次点击变红，第二次执行）；运行中的会话禁止删除。
- 滚动：自绘 3px 方形 thumb，原生滚动条隐藏，避免挤占布局宽度。

### 4.3 Conversation

- 内容 `max-width: 720px`，保证长文可读。
- 用户消息右对齐浅灰气泡（`--bg-subtle`，右下角小圆角）；Agent 消息为左侧 7px 状态点 + 全宽文档流；不再有头像、名字、时间行。
- 工具与思考是紧凑活动行：状态点 + 标题 + chevron；连续低噪音调用压缩为一句摘要；整组活动左侧有 1px 竖轨，首尾停在状态点。
- 消息圆点悬停显示完整时间（Alt 气泡）。

### 4.4 内联界面块（正文内）

- 不再有独立的第三栏：模型生成的界面出现在回复正文中，随消息流滚动，宽度受正文 `720px` 约束。
- **是否出现由 Agent 决定**：系统提示词引导模型在布局或交互比文字更清楚时用围栏代码块（语言标记 `html`）；不设用户开关。
- **表达方式**：普通 HTML 为主体，叠加极小的 `data-ui-*` 属性与 `{{ }}` 表达式；状态用 JSON 数据岛声明，表达式由固定语法解析，**不执行任何模型编写的 JavaScript**。
- **交互**：输入绑定、清单勾选、步进器、筛选、表单提交和动作按钮都在正文内完成；状态留在本机，只有用户点击发送/提交时才进入会话。
- **安全**：先解析再按白名单重建；脚本/样式/iframe/对象/事件属性全部丢弃；链接只允许 `http(s)`/`mailto`，图片只允许对话中出现过的 `https` 地址。
- **风格**：宿主 CSS 决定排版，模型只使用宿主定义的组件类（卡片、Callout、指标、步骤、图表等），见 §6.4。

---

## 5. 安全分级（产品的灵魂，单独成章）

> 核心主张：**危险的视觉重量与风险成正比。** 用户应该"看一眼就知道这件事多重"。
>
> 默认策略：工作区写入（旧内容自动备份）、只读命令与项目验证命令自动放行；其余命令进入审批。下面的三级审批卡只描述**进入审批的动作**；自动放行的动作不弹卡，但仍在会话中以活动行留痕。

### 5.1 审批（Approval）三级

| | **Low** | **Medium** | **High** |
|---|---|---|---|
| 典型 | 只读、可逆 | 写入工作区内文件 | 执行命令 / 不可逆 / 越界 |
| 形态 | **单行**，安静 | **卡片** + 参数 | **卡片** + 红色左边条 |
| 边框 | `--border-soft` | `--warn-border` | `--danger-border` + 左 `3px danger` |
| 风险标签 | 灰底 | 琥珀底 | 红底 |
| 命令/参数预览 | 隐藏 | 显示 | 显示 + 高亮 |
| action digest | 隐藏 | 显示 | 显示 + 强调 |
| 倒计时 | 隐藏 | 显示 | 显示 + **数字变红** |
| 按钮 | 内联小按钮 | 标准按钮 | 标准按钮 + 说明文案 |
| 阴影 | `xs` | `sm` | `md` |

> 规则：**升级的是"信息密度 + 色彩重量"，不是"闪烁"。** 高危不靠动画吓人，靠结构和颜色沉下来。

### 5.2 审批按钮语义

- **批准** = primary（accent 填充 + `--on-accent` 深色文字），文案视风险而定（low「批准」/ high「批准并运行」）。
- **拒绝** = 中性按钮，hover 才转 danger（避免误触，也避免过度恐吓）。
- 高危需在按钮旁给一句**约束说明**（如"批准仅对这条命令、这套参数生效"）。

### 5.3 上下文与用量

- composer 右侧有一个**上下文圆环**，显示最近一次 prompt 占模型窗口的比例；点击/悬停打开 Alt 面板。
- 面板内容：真实已用/窗口 token、组成（消息 / 系统工具 / 系统提示 / 剩余，按字符比例估算并标注）、轮次/步数、累计 token、平均速度、缓存命中率。
- 圆环颜色：正常 accent；≥80% warn；≥95% danger。
- 不再有常驻 Runbar：运行中发送按钮变为「停止」，状态与用量按需查看。

### 5.4 状态语义色映射（Run / Node）

| 状态 | 色 | 动效 |
|---|---|---|
| Scheduled / Running | accent | ring 旋转 / 状态点脉冲 |
| WaitingApproval | warn | 状态点脉冲（慢） |
| Succeeded | ok | 无 |
| Failed | danger | 无 |
| Aborted | danger | 无 |
| AwaitingReconciliation | warn | 状态点脉冲 |
| Normal / Degraded / Maintenance | ok / warn / danger | 见 §4.1 pill |

---

## 6. 组件规范

### 6.1 按钮

| 类型 | 底 / 边 / 字 | 阴影 | 用途 |
|---|---|---|---|
| primary | accent / accent / 白 | `--shadow-pop` | 每屏一个主动作 |
| default | 白 / border-soft / primary | `xs` | 次动作 |
| danger-soft | danger-soft / danger-border / danger | 无 | 取消、拒绝（hover 加深） |
| icon | 透明，hover `--bg-subtle` | 无 | 工具栏、icon-only |

- 高 `34px`（小按钮 26~28，发送 32），圆角 `--r-md`，字号 Label。
- `:active` 下沉 `translateY(0.5px)`。

### 6.2 输入 / Composer

- Composer 是**主角**：白底 + `--border-soft` + `--shadow-md`，`:focus-within` 时边框转 `--accent-border`、阴影转 `--shadow-pop`。
- textarea 无边框，placeholder 用 `--text-faint`。
- 底部行：上限提示（mono faint）+ 发送按钮。内联界面由 Agent 决定是否使用，
  输入框不提供用户开关。

### 6.3 工具调用 / 结果（activity row）

- 工具调用与结果合并为一行：状态点（成功圆点 / 失败红色方点 / 运行中 spinner）+ 工具标题 + chevron；不再使用带边框的折叠卡片。
- 展开体：左缩进 + hairline 左边线；输入与输出各一段 mono 预格式块；复制是次级操作，hover/focus 才出现。
- 连续 ≥2 个低噪音活动（思考 + read/search/list）压缩成摘要行，例如「Thought for 49s, searched for 3 patterns, read 6 files, and listed 2 directories」。写操作、失败与未知命令不压缩，保持独立行。
- 摘要按当前语言做单复数与时态（前端 `tPlural` + `Intl.ListFormat`）；数字用等宽字体。
- 整组活动左侧有 1px 竖轨，首行与末行的线停在状态点中心；失败用方形点，不只靠颜色。

### 6.4 内联界面块（HTML + 受控组件）

> 模型写 HTML 与少量 `data-ui-*` 属性；宿主解析后按白名单重建为 Solid 元素，表达式由固定语法求值。**宿主文档永不注入模型 HTML，也永不执行模型代码；富视觉块在禁用脚本的沙盒 iframe 中渲染**。

| 组件 | 视觉 |
|---|---|
| 容器 / 卡片 / 网格 | 正文内不再套统一卡片壳；`ui-col` / `ui-row` / `ui-box` 组成版式，`ui-grid` 支持 `data-cols`（窄屏堆叠），间距/内边距/对齐由 token 化 `data-*` 控制 |
| 卡片 / Callout | `ui-card` 为浅底面板；`data-bg` / `data-border` / `data-radius` 可调；`ui-callout` 按 `ui-info/ok/warn/danger` 使用语义底色 |
| 指标 | `ui-metrics` 自适应网格 + `ui-metric`（大号数值 + 小标签，tabular numbers） |
| 徽标 / 步骤 | `ui-badge` 胶囊标签；`ui-icon` 宿主图标（固定名称表）；`ui-steps` 编号时间线，`data-state` 区分 done/current/todo |
| 表格 / 键值 | 行间 hairline，表头 10px 大写 muted；`ui-num` 数字右对齐；`ui-kv` 两列 |
| 图表 | `table.ui-chart` + `data-type="bar|line|area"`，由表格行绘制；caption 作为说明；带无障碍表格 |
| 表单 / 控件 | 原生 input/select/textarea/range/checkbox/radio，输入框与全局风格一致；提交按钮 primary |
| 图片 / SVG | `ui-figure` + 圆角裁切图片 + `ui-caption`；`ui-carousel` 横向滚动图片/卡片；图片限 https；SVG 基础形状/文本支持 `{{ }}` 属性表达式与宿主颜色 token，用于示意图 |
| 富视觉块 | `<template data-ui-rich>` 内的自定义 HTML/CSS 在禁用脚本的 same-origin iframe 中渲染；注入宿主 token；无表单、外链样式、嵌套帧或脚本 |
| 状态反馈 | 交互即时更新，不弹“已保存”类提示；复制成功走 toast |

- 样式完全由 `.inline-ui` 与组件类定义；`class` 只允许宿主清单内的 token，布局只允许白名单内、取值受校验的 `data-*` token，普通块的 `style` / 事件属性在清洗时丢弃；富视觉块内的样式只存在于沙盒文档。
- 界面块出现在正文流中，不需要额外的“已更新”提示或版本徽章。

### 6.5 空状态

- 居中，max-width `380px`。
- 大图标（`--accent-softer` 底 + `--accent-border` 边）+ 标题 + 提示（可含 mono `code` 片段）。
- 侧栏等紧凑面板使用 `EmptyState`：小图标 + 一句标题 + 一句说明，不占满整屏。

### 6.6 悬浮气泡与菜单

- **Alt 气泡**：Portal + 固定定位；hover 时跟随指针，贴边自动翻转；半透明表面 + 14px 模糊 + 分层阴影；`prefers-reduced-transparency` 时退化为不透明表面。
- 使用位置：会话行悬停显示完整标题/状态/消息数/时间/会话 id；消息圆点悬停显示完整时间；上下文圆环点击固定显示面板。
- **下拉菜单**：4px 内边距、细边框、`--shadow-lg`、80ms 上滑；触发器无边框，hover 纯灰底；选中项加粗 + `--bg-subtle`，不用整行强调色。
- **可折叠区段**：标题行 + chevron 旋转 + 200ms 高度过渡；上下文面板的「上下文窗口」与工具/思考详情共用这套语言。

---

## 7. 动效语言（微动画是一等公民）

> 原则：**动效只为"状态改变"服务，永不为了装饰。** 它要快到不被察觉为"动画"，却又足够顺滑让人感到精致。

### 7.1 时长与缓动

| Token | 值 | 用途 |
|---|---|---|
| `--dur-fast` | 120ms | hover、颜色、opacity |
| `--dur-med` | 200ms | 卡片出现、展开 |
| `--dur-slow` | 320ms | 面板滑入滑出、布局变化 |
| `--ease` | `cubic-bezier(0.32, 0.72, 0.35, 1)` | 标准（回弹感极小，干净） |

> 规则：**单元素动画 ≤ 320ms。** 超过会显得拖沓，破坏"轻盈"。

### 7.2 核心微动画清单

| 场景 | 动画 | 时长 | 说明 |
|---|---|---|---|
| **hover** | 纯灰底 / 颜色过渡，不加边框 | fast | 所有可点元素与列表行 |
| **按钮按下** | `translateY(0.5px)` | fast | 物理反馈 |
| **审批卡出现** | `rise`：opacity 0→1 + `translateY(6px)→0` | med | 新审批"浮上来" |
| **流式文本** | 尾部 `▍` 光标 `blink`（1s steps） | — | 表示"正在生成" |
| **运行状态点** | `pulse`：box-shadow 扩散 | 2s loop | live/运行中 |
| **运行中 ring** | `spin` 旋转 | 0.9s loop | 仅运行态 |
| **侧栏收起** | `margin-left` 过渡 | slow + ease | 布局级 |
| **toggle 开关** | 圆点 `translateX` + 底色过渡 | fast | |
| **下拉菜单** | `select-list-in`：opacity + `translateY(2px)→0` | 80ms | 浮层出现 |
| **折叠展开** | chevron `rotate(90deg)` + 高度过渡 | 200ms | 上下文面板、工具/思考详情 |

### 7.3 加载与"诚实的等待"

- **运行中**：ring 旋转 + 状态文字 accent，**不用骨架屏假装内容已到**。
- **流式输出**：用闪烁光标表达"正在进行"，文本逐字出现，**不用假进度条**。
- **无假乐观**：操作未确认前不显示成功态。取消后若"outcome unknown"，就明确标 unknown，不用对勾糊弄。

### 7.4 动效红线

- ❌ 不为装饰加动画（漂浮、视差、无意义的入场）。
- ❌ 不让高危元素"闪烁/抖动"吓人（高危靠结构与色，不靠动）。
- ❌ 不用超过 320ms 的单元素动画。
- ❌ 不用弹性过大的缓动（如强 back/bounce），保持干净专业。
- 不提供 `prefers-reduced-motion` 降级；动效在所有系统设置下保持启用（2026-11-22 决定）。

### 7.5 性能

- 只动 `transform` 和 `opacity`（GPU 友好），避免动 `width/height/top/left`（meter 的 `width` 是唯一例外，量小）。
- 列表项动画用 `content-visibility` / 按需，避免长列表抖动。

---

## 8. 可访问性

- 正文对比度 ≥ 4.5:1；辅助文字 ≥ 3:1（`--text-muted` 在最浅底上需校验）。
- 所有交互元素有 `:focus-visible` 环：`2px --accent-ink` + `2px` offset。
- 状态不只靠颜色：配文字标签或图标（如状态点旁永远有文字）。
- 点击目标桌面 ≥ 24px（icon 按钮 30px）；触摸设备 ≥ 44px。

---

## 9. 图标系统

- 风格：**线性图标**（stroke `1.8~2`，圆角线帽），统一 24 viewBox。
- 尺寸：工具栏 16 / 卡片头 14 / 内联 11~13。
- 颜色：继承 `currentColor`，默认 `--text-secondary`，hover 转 primary。
- 品牌 logo：圆角方块 + 纯色 `--accent` + 字标 `φ`（不使用渐变）。

---

## 10. 落地映射（Solid）

| 规格 | 代码位置 |
|---|---|
| Design tokens | `src/styles/app/tokens.css`（全部 CSS 变量，禁硬编码） |
| 字体资产 | `src/index.tsx` 的 `@fontsource` 导入（Figtree / Noto Sans SC / Cascadia Code） |
| 组件 | `src/components/Conversation.tsx`（正文流/活动行）、`Sidebar.tsx`（会话列表）、`Composer.tsx`（输入与上下文圆环）、`Alt.tsx`（悬浮气泡）、`SideScrollbar.tsx`（自绘滚动条）、`RichHtml.tsx`（内联界面白名单与交互）、`uiExpression.ts`（表达式） |
| 工具活动分类 | `src/components/toolActivity.ts`（read/search/list 的展示分类，不是安全边界） |
| 上下文面板 | `src/components/ContextMeter.tsx` + `src/styles/app/context-meter.css` |
| 图表 | `src/components/InlineChart.tsx`（表格 → SVG，bar/line/area） |
| 动效 | `src/styles/app/motion.css` keyframes + 工具类 |
| 自绘标题栏 | `src-tauri/tauri.conf.json` frameless + `src/components/Titlebar.tsx`；平台决定 Windows/Linux 右侧 caption 按钮、macOS 左侧红绿灯、Android/iOS 应用栏 |
| 模型 / 思考选择 | `src/components/ModelPicker.tsx`（悬浮子菜单，触摸内联展开） |
| 通知 | `src/toast.ts` + `src/components/Toasts.tsx`（去重、限流、自动消失） |
| Markdown | `src/components/Markdown.tsx`（安全 JSX，不注入 HTML；`html` 围栏交给 `RichHtml`） |
| 响应式 / 安全区 | `src/styles/app/responsive.css` 断点 1100/900/760/520 + `env(safe-area-inset-*)` |
| 深色模式 | 未来替换 `:root` token 表为 `[data-theme="dark"]` |

**实现顺序建议**：tokens → 布局骨架（titlebar/sidebar/main）→ 对话与活动行 → 上下文面板/composer → 内联界面块 → 微动画打磨。

---

## 11. 待办 / 开放问题

- [ ] 获取并打包字体：Figtree、Noto Sans SC（子集化）、Cascadia Code 的 woff2 及 LICENSE，放入 `src/assets/fonts/`。
- [ ] 浅色版配色需在真机上校验对比度（尤其 `--text-muted` 与 warn/danger soft 底）。
- [ ] 深色模式 token 表（v2）。
- [ ] Chart / 数据可视化的专用色板（接 dataviz 规范）。
- [ ] 空会话首屏、模型未配置、崩溃待复核等**异常态**的高保真设计。
- [ ] 品牌 logo 的正式图形（当前为占位 `φ`）。
