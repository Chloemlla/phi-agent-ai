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
| **安全可见** | 危险必须在视觉上更重 | 审批/预算按风险分级，视觉重量与风险成正比（见 §5） |
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
| `--accent-ink` | `#2B6FD4` | 浅底上的强调文字 |

> 规则：**一屏只让一个元素"最亮"。** 强调色用于"当前最重要的那一个动作/状态"，不要大面积平铺。

### 1.5 语义状态色（克制、去饱和）

状态色比强调色更淡，只在"状态"上出现，不用于装饰。

| 语义 | 主色 | soft 底 | border | 用于 |
|---|---|---|---|---|
| ok | `#2FA96C` | `#EBF6F0` | `#C6E8D5` | 成功、Normal、已完成步骤 |
| warn | `#CE911F` | `#FBF4E4` | `#EFD CAB`（见注） | 需复核、中等风险、预算过半 |
| danger | `#DA5B4E` | `#FCEFED` | `#F4CCC7` | 失败、高危、取消、超预算 |
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
| `--font-mono` | `"Cascadia Code", "JetBrains Mono", Consolas, monospace` | **一切数据**：数字、命令、digest、token 数、路径、代码（微软 **Cascadia Code**） |

> 匹配逻辑：Figtree 无中文字形，中文自动落到 Noto Sans SC；西文与中文混排时
> 各自使用自己的字体，字号一致、视觉重量对齐（二者 x-height 接近，混排不跳）。

### 2.2 字体打包（local-first 的硬约束）

phi-agent 离线可用，**三个字体都必须作为 webfont 打包进应用资产**，禁止运行时走 CDN，也禁止假设系统已预装（Cascadia Code 仅在装有 Windows Terminal / VS 的机器上存在）。

- 目录：`src/assets/fonts/`（Tauri 打包进 resources，与现有 `src/assets/` 同级约定）。
- 格式：**woff2**（子集化；思源黑体按常用汉字子集 + 动态 `unicode-range` 分片，控制体积）。
- 字重：Figtree 400 / 500 / 600 / 700；Noto Sans SC 400 / 500 / 700；Cascadia Code 400 / 600。
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
- 金额格式 `$0.012 / $0.50`；token 用千分位 `4,812`。
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
├──────────┬─────────────────────┬───────────────┤
│          │                     │               │
│ Sidebar  │   Conversation      │  Workbench    │
│ 264px    │   (flex, max 720px) │  420px(可选)  │
│ 可收起    │                     │               │
│          │─────────────────────│               │
│          │ Runbar（状态/预算）  │               │
│          │ Composer（输入）     │               │
└──────────┴─────────────────────┴───────────────┘
```

### 4.1 Titlebar（自绘，不用系统默认）

- 高 `44px`，`-webkit-app-region: drag`；交互元素 `no-drag`。
- 左：窗口控制点（12px 圆点，hover 才显色）+ brand（logo + 字标）。
- 中：当前会话标题 + 工作区路径（mono，muted）。
- 右：节点状态 pill + 设置 + 侧栏开关。
- 底边 `1px --border-softer`。

### 4.2 Sidebar（可收起）

- 宽 `264px`，收起 = `margin-left: -264px` + `--ease` 缓动。
- 内容：新建会话按钮（primary block）→ 分区标签 → 会话列表 → 底部 node 信息。
- 会话项：标题 + mono 元信息（状态点 + 状态 + 相对时间）。
- **激活态**：白底卡片 + 左侧 `3px` 强调色指示条（不是整行变蓝，保持通透）。

### 4.3 Conversation

- 内容 `max-width: 720px`，保证长文可读。
- 消息间 `--sp-5`，角色行（avatar + 名字 + 时间）+ 正文。
- user / agent 用 avatar 颜色区分，**不用左右气泡对齐**（保持文档式流畅）。

### 4.4 Workbench（按需出现）

- 宽 `420px`（窄屏 360px），左 `1px` 分隔 + 极轻左阴影。
- **是否出现由 Agent 决定**：每次运行都向 Agent 提供 `ui_update` 工具，由它在
  可视化布局有帮助时提交视图；面板在视图出现时自动展开，**不设用户开关**。
- **身份建立靠三件小事**：标题 + `已更新` 脉冲指示 + 版本徽章（`v3`）。
- 内容 = 白名单组件（plan / table / form / actions），见 §6.4。

---

## 5. 安全分级（产品的灵魂，单独成章）

> 核心主张：**危险的视觉重量与风险成正比。** 用户应该"看一眼就知道这件事多重"。

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

- **批准** = primary（accent + `--shadow-pop`），文案视风险而定（low「批准」/ high「批准并运行」）。
- **拒绝** = 中性按钮，hover 才转 danger（避免误触，也避免过度恐吓）。
- 高危需在按钮旁给一句**约束说明**（如"批准仅对这条命令、这套参数生效"）。

### 5.3 预算 / Runbar

- 位置：composer 正上方，与 composer 连成一体（圆角相接）。
- 三项硬上限**永远可见**：步数、成本、时间，各配一条 meter。
- **meter 颜色随消耗渐变**：
  - `< 50%` → accent
  - `50–80%` → warn
  - `> 80%` → danger
- 运行中：状态文字 accent + 旋转 ring；可取消（danger 软底按钮）。

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
- 底部行：上限提示（mono faint）+ 发送按钮。工作台是否启用由 Agent 决定，
  输入框不提供用户开关。

### 6.3 工具调用 / 结果（tool block）

- 折叠卡片，白底 + hairline + `xs` 阴影。
- summary：左侧 glyph（成功 accent-soft / 失败 danger-soft）+ 名称（mono 加粗）+ 右侧 chevron。
- 展开体：`--bg-inset` 底 + mono 12px。错误内容用 danger 文字。
- 默认**折叠**，失败的可默认展开。

### 6.4 Workbench 组件（白名单）

> 全部走"声明式 JSON → 受控渲染"，**前端永不执行模型代码**。

| 组件 | 视觉 |
|---|---|
| Plan / Steps | 垂直 stepper：圆圈编号（done=ok 底+✓ / now=accent 实底+pop 影 / 待办=灰）|
| KV Table | 无边框，行间 hairline，左列 muted、右列 mono |
| Form | 与全局输入一致；提交按钮 primary |
| Actions | 按钮组，主动作 primary，次动作 default |
| Chart | 遵循 dataviz 规范（中性底 + 单强调色系） |

- **更新提示**：agent `ui_update` 后，wb-head 的 `已更新` 标签做**一次**脉冲（不循环），wb-body 做一次 `wbIn` 淡入上移。

### 6.5 空状态

- 居中，max-width `380px`。
- 大图标（`--accent-softer` 底 + `--accent-border` 边）+ 标题 + 提示（可含 mono `code` 片段）。

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
| **hover** | 颜色/背景过渡 | fast | 所有可点元素 |
| **按钮按下** | `translateY(0.5px)` | fast | 物理反馈 |
| **审批卡出现** | `rise`：opacity 0→1 + `translateY(6px)→0` | med | 新审批"浮上来" |
| **流式文本** | 尾部 `▍` 光标 `blink`（1s steps） | — | 表示"正在生成" |
| **运行状态点** | `pulse`：box-shadow 扩散 | 2s loop | live/运行中 |
| **Workbench 更新** | `已更新` 脉冲一次 + body `wbIn`（淡入+上移 4px） | slow | "agent 刚调整过" |
| **运行中 ring** | `spin` 旋转 | 0.9s loop | 仅运行态 |
| **侧栏收起** | `margin-left` 过渡 | slow + ease | 布局级 |
| **meter 变化** | `width` + 颜色过渡 | slow | 预算消耗平滑 |
| **toggle 开关** | 圆点 `translateX` + 底色过渡 | fast | |
| **折叠展开** | chevron `rotate(90deg)` + 高度过渡 | fast | |

### 7.3 加载与"诚实的等待"

- **运行中**：ring 旋转 + 状态文字 accent，**不用骨架屏假装内容已到**。
- **流式输出**：用闪烁光标表达"正在进行"，文本逐字出现，**不用假进度条**。
- **无假乐观**：操作未确认前不显示成功态。取消后若"outcome unknown"，就明确标 unknown，不用对勾糊弄。

### 7.4 动效红线

- ❌ 不为装饰加动画（漂浮、视差、无意义的入场）。
- ❌ 不让高危元素"闪烁/抖动"吓人（高危靠结构与色，不靠动）。
- ❌ 不用超过 320ms 的单元素动画。
- ❌ 不用弹性过大的缓动（如强 back/bounce），保持干净专业。
- ✅ 尊重 `prefers-reduced-motion`：关闭 pulse/spin/rise，仅保留 opacity 过渡。

### 7.5 性能

- 只动 `transform` 和 `opacity`（GPU 友好），避免动 `width/height/top/left`（meter 的 `width` 是唯一例外，量小）。
- 列表项动画用 `content-visibility` / 按需，避免长列表抖动。

---

## 8. 可访问性

- 正文对比度 ≥ 4.5:1；辅助文字 ≥ 3:1（`--text-muted` 在最浅底上需校验）。
- 所有交互元素有 `:focus-visible` 环：`2px --accent` + `2px` offset。
- 状态不只靠颜色：配文字标签或图标（如状态点旁永远有文字）。
- 支持 `prefers-reduced-motion`（见 §7.5）。
- 点击目标 ≥ 24px（icon 按钮 30px）。

---

## 9. 图标系统

- 风格：**线性图标**（stroke `1.8~2`，圆角线帽），统一 24 viewBox。
- 尺寸：工具栏 16 / 卡片头 14 / 内联 11~13。
- 颜色：继承 `currentColor`，默认 `--text-secondary`，hover 转 primary。
- 品牌 logo：圆角方块 + 渐变（`--accent` → `#7AB3FF`）+ 字标 `φ`。

---

## 10. 落地映射（Solid）

| 规格 | 代码位置 |
|---|---|
| Design tokens | `src/App.css` `:root`（全部 CSS 变量，禁硬编码） |
| 字体资产 | `src/assets/fonts/`（Figtree / Noto Sans SC / Cascadia Code 的 woff2 + `@font-face`） |
| 组件 | `src/App.tsx`（消息/审批/runbar/composer）、`src/workbench/ViewRenderer.tsx`（workbench 白名单） |
| 动效 | `src/App.css` keyframes + 工具类 |
| 自绘标题栏 | `src-tauri/tauri.conf.json` frameless + `src/` 顶部组件，drag region |
| 深色模式 | 未来替换 `:root` token 表为 `[data-theme="dark"]` |

**实现顺序建议**：tokens → 布局骨架（titlebar/sidebar/main）→ 对话与审批分级 → runbar/composer → workbench → 微动画打磨。

---

## 11. 待办 / 开放问题

- [ ] 获取并打包字体：Figtree、Noto Sans SC（子集化）、Cascadia Code 的 woff2 及 LICENSE，放入 `src/assets/fonts/`。
- [ ] 浅色版配色需在真机上校验对比度（尤其 `--text-muted` 与 warn/danger soft 底）。
- [ ] 深色模式 token 表（v2）。
- [ ] Chart / 数据可视化的专用色板（接 dataviz 规范）。
- [ ] 空会话首屏、模型未配置、崩溃待复核等**异常态**的高保真设计。
- [ ] 品牌 logo 的正式图形（当前为占位 `φ`）。
