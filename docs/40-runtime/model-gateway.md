# 模型网关

> 状态：🌿 · 最后更新：2026-11-22
> 上游：`20-domain/domain-model.md`（ModelCall 事件）、`30-extension/plugin-system.md`（provider.model / strategy.modelRouter）
> 关联：`40-runtime/policy-and-approval.md`（辅助模型）、`70-quality/`（评测）

---

## 0. 一句话

**模型网关是节点与「模型提供方」之间的唯一入口 (`ModelPort`)：它把「调用一个模型」抽象成统一接口，并集中承担路由、缓存、用量计量、（可选）预算预留、失败降级、外发授权与密钥管理。** 领域逻辑只见「模型」这个抽象，不认任何厂商。

> 它是**发行基线的必需槽位** `model.invoke`（ADR-0009），不是一个内核部件——`ModelPort` 定义在模型上下文的契约包里。

---

## 1. 端口抽象（依赖倒置）

```
  循环/上下文/审批者 ──▶ ModelPort（接口） ◀── 适配器 provider.model
                            │                    ├─ Anthropic
                            │                    ├─ OpenAI
                            │                    ├─ 本地模型（Ollama/…）
                            │                    └─ 第三方插件提供的 provider
                            ▼
                      ModelRequest → ModelResponse（含用量、延迟、结束原因）
```

**为什么集中在一处**：路由、缓存、计量、降级、密钥都是**横切关注点**，散落在各调用点会失控。网关是它们的唯一收口。

## 2. Provider 与 ModelSpec

| 概念 | 说明 |
|---|---|
| **Provider** | 一个模型来源（`provider.model` 扩展点实现）：鉴权、协议、错误码 |
| **ModelSpec** | 一个具体模型：能力（工具调用/视觉/流式）、上下文窗口、**定价快照**、限速 |
| **ModelCall** | 一次调用 + 用量（input/output/cache tokens、延迟）+ 结束原因，产 `ModelCallIssued/Returned/Failed` 事件 |

**能力协商**：调用前校验「所需能力 ⊆ 模型能力」——如策略要求工具调用、但选中的模型不支持 → 换模型或失败，不静默降级（P9）。

---

## 3. 路由（`strategy.modelRouter`）

**默认策略（可插拔）**：按「任务难度 / 类型」把调用分派到不同模型。

```
   调用请求 ──▶ 路由判定
                │
   ┌────────────┼─────────────────────────┐
   │            │                         │
 分类/抽取/    普通推理                  高难推理/权衡
 审批（辅助）   → 中档模型                → 强模型
   → 小模型
```

- **小模型分流**：分类、抽取、**权限审批**（见 `policy-and-approval.md`）等判断类任务用**辅助模型**（小、快、便宜）。
- **成本 vs 质量**：路由是成本控制的第一道闸——把便宜任务拦在贵模型之外。
- **路由可被插件替换**：`strategy.modelRouter` 是扩展点，用户/插件可定义自己的路由规则。

> 与 `awesome-architecture` 的一致：**成本随步数线性涨**，路由 + 缓存 + 用量可见（硬上限为可选目标态）是三条破解。

---

## 4. 缓存

| 类型 | 说明 |
|---|---|
| **提示缓存（Prompt Cache）** | 复用不变前缀（系统提示/长文档），显著降本（对齐厂商 prompt caching） |
| **响应缓存（Response Cache）** | 相同输入（含确定性参数）命中即复用；仅对**幂等、无副作用**的调用安全 |
| **嵌入/检索缓存** | 记忆检索的中间结果复用 |

**约束**：响应缓存只对**确定性**调用启用；带随机性/工具调用的不缓存（否则行为不可复现）。

### 4.1 调用类别：内部调用走受限路径 ⭐（回应「审批递归」）

`ModelCallIssued` 携带 `callClass`，网关据此走不同路径：

| 类别 | 谁发起 | 上下文组装 | 是否过审批 |
|---|---|---|---|
| `main` | 行动循环 | **完整** `context.contributor` 链 | 模型调用本身不是「改变世界」的动作，**不过审批**（`policy-and-approval.md` §0） |
| `auxiliary` | 审批者 / 准入判定 | **固定短提示**，不检索、不注入完整历史 | **绝不**——否则「审批要调模型、模型要审批」递归 |
| `summarize` | 上下文压缩 | 只给被压缩的片段 | 否 |
| `embed` | 检索 | 只给待嵌入文本 | 否 |

**铁律**：`auxiliary / summarize / embed` **不进审批链、不触发完整上下文组装**。这不是优化，是**防递归的结构保证**——否则辅助模型审批会要求先审批那次审批调用。

（对齐 `model-gateway.md` §8「辅助模型不是 Agent」：它不跑行动循环、不调工具、无记忆。）

---

## 5. 用量与预算：调用前预留，调用后结算

> **2026-11-15 变更**：v1 已移除 Run 级预算硬上限，并进一步移除了价格计算。当前行为是**调用后按实际用量记录** `ModelCallReturned { usage }`，Runbar 显示步数/token/耗时；没有预留、结算、成本或超限中止。下面的「调用前预留」保留为**可选目标态**的设计记录（见 `80-roadmap/scope-additions.md` §2.10 与 §2.12）。

> 「调用结束后扣费」保证不了「不超过成本预算」——一次调用本身就可能超掉剩余额度。硬上限必须在**发起之前**成立。

- **预留（reserve）**：发起前先预留额度（`BudgetReserved`），可用额度不足 → **拒绝发起**。
- **输出上限**：每次调用必须带 `maxOutputTokens`，预留额度按它计算——预留才有上界。
- **并发统一记账**：同一 Run 及其全部子 Run 走**同一个账本**；并发子 Run 各自预留，父的可用额度受所有子预留约束。
- **结算（settle）**：返回后按实际用量结算（`BudgetSettled`），多余的预留释放。**失败也要结算**——请求失败但已产生费用照样记账。
- **用量未知**：provider 未返回用量时，按**预留值**记账（宁可高估），并在事件里标注「估算」。
- **计量口径**：tokens + 金额双记账；金额用调用时的 `ModelSpec` **定价快照**，避免厂商调价后账单漂移。
- 事件：`ModelCallReturned { usage }` 与 `BudgetReserved / BudgetSettled / BudgetExceeded`。

> 这条是从「愿望式的硬上限」变成「结构上的硬上限」的关键：**没有预留，就没有硬上限。**（可选目标态；v1 不启用。）

### 5.1 模型目录数据（可选）

`src-tauri/assets/model-catalog.json` 是编译进二进制的可选目录，用于在设置页为模型 id 自动填充元数据（名称、上下文、输出上限、规模）。目录只维护**当前**模型家族：GPT-6、Claude 5.x / Fable、GLM-5.x、DeepSeek-V4、Kimi-K3、Gemini 4。未知模型保持 manual spec，只填 id；provider 的模型列表拉取仍是实时来源。所有当前模型都接受思考强度，不再维护「是否支持推理」标记。

`patterns` 与 `requires` 允许略微模糊：`patterns` 匹配家族前缀（如 `gpt-6`、`claude-sonnet-5`），`requires` 再要求变体标记（如 `luna`、`flash`、`pro`），更具体（匹配串更长）的条目优先。`display` 是带 `{version}` 的显示模板，版本从 `anchor` 之后的数字读取（`.` 与 `-` 等价，最多两段），因此未记录的小版本 `gpt-6.5-luna` 仍显示为 `GPT-6.5 Luna`，带日期快照的 id 也不会把日期当成版本。厂商发布新模型时应同步更新本文件。

| 字段 | 类型 | 说明 |
|---|---|---|
| `patterns` | string[] | 小写子串，任一命中即可；匹配时先去掉 vendor 前缀（如 `anthropic/`） |
| `requires` | string[]? | 必须同时命中的小写子串，用于区分变体 |
| `name` | string | 显示名；无法解析版本时的回退值 |
| `display` | string? | 显示模板，`{version}` 替换为从 id 解析出的版本 |
| `anchor` | string? | 版本数字在 id 中的起始标记，如 `gpt-`、`claude-sonnet-` |
| `description` | string | 一句话描述 |
| `tags` | string[] | 如 `coding`、`long-context`、`fast`、`agentic` |
| `contextWindow` | number? | 上下文窗口（tokens） |
| `maxOutputTokens` | number? | 单次输出上限 |
| `scale` | number | 模型规模 1–5（1 轻量、5 前沿）；缺省按 3 处理，用于限制子代理调用比自己更大的模型 |

示例：

```json
[
  {
    "patterns": ["gpt-6"],
    "requires": ["luna"],
    "name": "GPT-6 Luna",
    "display": "GPT-{version} Luna",
    "anchor": "gpt-",
    "description": "Small, fast GPT-6 variant built for high-volume extraction, classification, structured summaries and routine interactions.",
    "tags": ["fast", "high-volume", "extraction", "economic"],
    "contextWindow": 1050000,
    "maxOutputTokens": 128000,
    "scale": 2
  }
]
```

目录只填充用户留空的字段，显式值优先；未知 id 不会失败。目录不含价格：节点只记录 token 用量，不做计价。

缓存用量：Anthropic 请求在 system 块上带 `cache_control: { type: "ephemeral" }`，usage 读取 `cache_creation_input_tokens` / `cache_read_input_tokens`；OpenAI / DeepSeek 的 `cached_tokens` / `prompt_cache_hit_tokens` 从普通输入中扣除。`ModelCallReturned` 记录 `cache_read_tokens` 与 `cache_write_tokens`，不计算金额。

### 5.2 推理强度（thinking effort）

输入框的模型选择器允许为下一条消息指定模型与思考强度（`off` / `low` / `medium` / `high`）。网关按协议翻译：

| 协议 | 映射 |
|---|---|
| Anthropic Messages | `thinking: { type: "adaptive", display: "summarized" }` + `output_config: { effort }`；不发送 `temperature` |
| OpenAI Chat Completions | 设置思考强度时发送 `reasoning_effort: low\|medium\|high` |
| OpenAI Responses | 设置思考强度时发送 `reasoning: { effort: low\|medium\|high }` |

`off` 或未选择时不发送任何思考字段。当前维护的模型都接受思考强度；不识别该字段的兼容供应商会忽略它，不需要额外的模型能力标记。Anthropic 返回的 `thinking` / `redacted_thinking` 块会原样保存在助手消息中；重放请求只保留最近一条助手消息的思考块（更早的块对工具续写无用，且会随历史裁剪失效），签名缺失的块不会回传。旧版 `budget_tokens` 形态不再支持。

---

## 6. 失败、重试与降级

| 情况 | 处理 |
|---|---|
| 限速 / 临时错误 | 指数退避重试（有次数上限；消耗 token，预算硬上限为目标态） |
| Provider 不可用 | 按策略**降级**到备用 provider/model（若配置） |
| 上下文超长 | 触发压缩/裁剪（见 `context-and-memory.md`），而非直接失败 |
| 输出不合法（工具参数坏） | 不执行该工具；把助手文本与带工具名的 `tool_result` 错误写回历史，循环继续，由模型重新发起调用 |
| 全部失败 | Run `Failed`，带可诊断错误 |

**不静默降级**：从强模型掉到弱模型、从有工具能力掉到无——必须**显式**（事件留痕），否则质量悄悄退化无法察觉（对齐评测驱动的初衷）。

---

## 7. 密钥管理与外发授权

### 7.1 密钥

- 密钥存 **OS Keychain / 等效加密**，**明令禁止**「AES 文件 + 同目录明文密钥」的反面做法（`agent-research` 对 pi-desktop 的教训）。
- 密钥**不进日志、不进事件、不进上下文**。
- 每 provider 独立管理；插件 provider 的密钥作用域受限。

### 7.2 外发授权（EgressGrant）——「默认不上传」必须落到链路上 ⭐

`local-first.md` 承诺「默认不上传会话、记忆」——但主模型、辅助审批、嵌入、摘要**都可能把内容发出去**，降级还可能**改变数据接收方**。所以「默认不上传」要具体化为**外发授权**：

| 维度 | 说明 |
|---|---|
| **接收方** | 哪个服务（不同 provider 分别授权） |
| **用途** | 推理 / 审批 / 嵌入 / 摘要（**分别授权**，不因为填过一个 API Key 就默认全部） |
| **内容类别** | 会话内容 / 记忆 / 项目文件 / 工具结果 |

- 用户首次为项目选择远端模型时，**明确允许哪些内容用于该服务的推理**；授权后正常调用不反复弹窗，只发送**本次需要的内容**。
- **降级只能在已授权的服务集合内切换**（`#29`）——备用 provider 不在授权集里就不能降级，宁可失败。
- **嵌入与反思要单独授权**：它们最容易成为绕过主模型授权的后门。
- 提供明确的 **「仅本地」模式**：路由与失败降级**都必须遵守它**。
- 事件：`EgressGranted / EgressRevoked`。

---

## 8. 辅助模型（小模型）的定位

**辅助模型不是 Agent**：它不跑行动循环、不调工具、无记忆，是一个**无状态判定/抽取器**。网关为它提供：

- 一个**受约束的调用**（固定的短提示、低温度、严格输出 schema）。
- 与主模型**共享** token 用量计量（可选预算为其目标态）。
- 可被 `strategy.modelRouter` 单独指向「便宜快」的模型。

它的用途：**权限审批**（`judge.approver`）、分类、抽取、轻量校验。

> **2026-11-22 扩展**：文本辅助角色扩展为按角色绑定：`session_title`（会话标题）、`memory_maintenance`（记忆后台整理）、`approval_judge`（后续）、`summarizer` / `embedding`（后续）。它们仍然是**服务调用 / 后台任务**，不进入 Turn / Run / 执行树；角色绑定、外发授权、事件与安全见 `auxiliary-models.md` 与 ADR-0014。生图使用独立的 `image.generate` 端口与用户选择的模型（ADR-0016）；决策模型使用独立的 `decision.evaluate` 端口与 Jev 类型化协议（ADR-0015、`decision-model.md`），都不走聊天网关。

---

## 9. 反模式

- ❌ **各调用点直接打厂商 API** → ✅ 一律走 `ModelPort`（否则路由/缓存/计量失控）。
- ❌ **密钥散落、入库、进日志** → ✅ Keychain + 永不落日志/事件。
- ❌ **静默降级模型** → ✅ 显式降级 + 事件留痕 + **只在已授权服务集内**。
- ❌ **对非确定性调用做响应缓存** → ✅ 只缓存确定性调用。
- ❌ **把辅助模型当 Agent 用** → ✅ 它是判定器，不是循环。
- ❌ **调用后才扣预算** → ✅ 调用前预留 + 输出上限 + 并发统一记账（可选目标态；v1 用逐次用量记录与用户取消）。
- ❌ **辅助/摘要/嵌入调用也走审批与完整上下文** → ✅ `callClass` 受限路径，防递归。
- ❌ **填了 API Key 就默认所有内容可外发** → ✅ 按接收方/用途/内容类别分别授权。

## 10. 待定

| 编号 | 问题 | 影响 |
|---|---|---|
| Q12 | 首批支持哪些 provider？是否含本地模型？ | `provider.model` 初始集；**离线承诺依赖本地模型**（见 `#33`） |
| — | 路由策略的默认规则表 | `strategy.modelRouter` 默认实现 |
| — | 降级链的默认配置 | 可用性 |
| — | 定价快照的更新机制（手动/拉取目录） | 计量准确性 |
| — | `EgressGrant` 的 UI 与默认粒度（按项目/按会话） | 隐私承诺的可感知性 |

## 11. 参考

- `agent-research/infra-tools/12-litellm-openrouter.md` — 多 provider 抽象与路由
- `agent-research/infra-memory/*`（嵌入/检索缓存）