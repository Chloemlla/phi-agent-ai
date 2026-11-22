# 决策模型：Jev 类型化判断层

> 状态：🌿 评审中 · 最后更新：2026-11-22
> 上游：ADR-0015、`40-runtime/model-gateway.md` §8、`40-runtime/policy-and-approval.md`
> 关联：`40-runtime/auxiliary-models.md`、`40-runtime/context-and-memory.md`

---

## 0. 一句话

决策模型不是聊天模型，也不是 Agent：它接收一段状态和若干**封闭式、带类型的问题**，返回校准过的概率 / 置信度 / 得分，供代码做 `if/else`。它只负责判断，不生成文本、不调工具、不拥有权限。

---

## 1. 定位与边界

**做**：

- 审批分流：判断某个 `Ask` 动作是否属于用户已授权的低风险类别。
- 长工具输出：判断该保留全文、摘要、截断还是只留引用。
- 后续：循环是否完成、用户消息路由到哪个工具 / 子 Agent。

**不做**：

- 不生成回复、不写摘要、不做多步推理、不跑行动循环、不调工具。
- 不替代编排器，也不替代策略引擎；策略仍拥有最终权威。
- 不接收开放式问题；所有问题必须是 `noul` / `choice` / `score`。
- 不自动放行高风险 / 不可逆 / 未知风险的动作。

> **关键区分**：用户说的“长工具调用精简”里，Jev 只能决定“要不要精简、用哪种策略”；真正的文字精简由文本 `summarizer` 角色或确定性截断完成。Jev 的输出里没有文本。

---

## 2. 配置

独立配置 `<data-dir>/decision.json`，不复用聊天 provider：

```json
{
  "provider": "jev",
  "baseUrl": "https://openrouter.ai/api/v1",
  "apiKey": "env:OPENROUTER_API_KEY",
  "model": "typesafe/jev-latest",
  "timeoutMs": 8000,
  "points": {
    "approval_triage": {
      "enabled": true,
      "threshold": 0.92,
      "tools": ["fs_write", "memory_write"],
      "commandPatterns": []
    },
    "tool_output_compaction": {
      "enabled": true,
      "threshold": 0.8,
      "minChars": 8000
    },
    "loop_completion": { "enabled": false },
    "route": { "enabled": false }
  },
  "egressGrant": "decision-state"
}
```

- `baseUrl` 由用户填写：OpenRouter（`typesafe/jev-latest`）或 TypeSafe 直连（`jev-latest` / 固定版本）。
- 凭据只存引用（`env:` / `keychain:` / `literal:`），读接口只返回脱敏提示。
- 模型 id 可固定版本以保证可复现，也可用 `jev-latest` 跟踪。
- 每个决策点单独开关和阈值；未配置的角色 / 决策点保持禁用，不回退到聊天模型。
- 设置页提供「测试连接」和一条示例决策，显示概率与延迟，不显示密钥。

---

## 3. 协议

### 3.1 请求

```rust
pub struct DecisionRequest {
    pub state: String,
    pub questions: BTreeMap<String, DecisionQuestion>,
    pub timeout_ms: u64,
}

pub enum DecisionQuestion {
    Noul { instructions: String },
    Choice { instructions: String, criteria: BTreeMap<String, String> },
    Score { instructions: String, criteria: Vec<String> },
}
```

- `state` 是调用方拼好的、脱敏后的状态文本；总长度受 provider 限制（约 3.2 万 token），超限时先裁剪。
- 问题 id 稳定（如 `approve` / `compact`），便于审计和阈值配置。
- `choice` 的选项是封闭集合；`score` 的等级是有序列表。
- 不支持多轮；每次判断独立。

### 3.2 响应

```rust
pub struct DecisionResponse {
    pub answers: BTreeMap<String, DecisionAnswer>,
    pub usage: Option<DecisionUsage>, // input tokens; output 通常为 0
}

pub enum DecisionAnswer {
    Noul { probability: f32 },                       // 0..1
    Choice { choice: String, probabilities: BTreeMap<String, f32>, confidence: f32 },
    Score { score: f32, distribution: Vec<f32>, confidence: f32 },
}
```

- 概率 / 置信度都是 0..1；调用方必须校验范围、选项存在、schema 完整。
- 错误处理：`401` 检查 Key，`422` 检查 schema，`429` 指数退避，`529` 过载退避；超过重试次数按失败处理。
- 响应不合法（字段缺失、选项不在 criteria 内、概率越界）视为失败，不猜测。

### 3.3 端口与槽位

```rust
#[async_trait]
pub trait DecisionPort: Send + Sync {
    async fn evaluate(
        &self,
        request: DecisionRequest,
        cancel: CancellationToken,
    ) -> Result<DecisionResponse, DecisionError>;
}
```

- 可选槽位 `decision.evaluate`；适配器 `jev_decisions`。
- 未安装 / 未配置时，`Services` 返回 `None`；调用方走各自的失败安全路径。

---

## 4. 决策点

### 4.1 审批分流（`approval_triage`）

现有策略返回 `Ask` 时：

1. 宿主先做硬规则：高风险 / 不可逆 / `OutcomeUnknown` / 策略 `Deny` → **直接走人**，不询问决策模型。
2. 只有动作命中用户显式配置的 `tools` 或 `commandPatterns`，且宿主评估为低风险时，才调用决策模型。
3. 问题：

```json
{
  "approve": {
    "type": "noul",
    "instructions": "这个动作是否属于用户已授权的低风险类别，且参数没有异常？"
  },
  "risk": {
    "type": "choice",
    "instructions": "这个动作的风险等级？",
    "criteria": {
      "low": "只影响工作区内的可逆文件或记忆",
      "medium": "有外部副作用或较大范围修改",
      "high": "不可逆、执行代码或涉及凭据"
    }
  }
}
```

4. 自动放行条件（全部满足）：
   - 策略结果是 `Ask`（不是 `Deny`）；
   - 动作在用户授权的工具 / 命令白名单内；
   - 宿主风险等级为 `low`（不采信模型自报的高/低作为唯一依据）；
   - `approve.probability >= threshold` 且 `risk.choice == "low"`、`risk.confidence >= threshold`；
   - 决策响应完整、未超时、动作摘要未变化。
5. 否则：显示审批卡，由人决定。默认**不**自动放行 `shell_exec`；只有用户显式加入 `commandPatterns`（精确 command 或受限模式）才进入决策；不可逆命令永远人工。
6. 自动放行时：
   - 发布 `DecisionAutoApproved`，记录 `callId`、模型、问题、概率、阈值、工具、动作摘要、风险等级；
   - 审批记录仍写入 `ToolInvokeDecided { decision: "allow", rationale: "decision-model" }`；
   - 用户在「审批历史」里可以查看并撤销该授权类别；撤销立即生效。
7. 决策模型不可用 / 低置信度 / schema 不合法 → **询问人**，绝不默认放行。

> 安全边界：决策模型只能在用户预先授予的低风险类别内**减少打扰**；它不能扩大权限、不能覆盖 `Deny`、不能批准不可逆动作。策略引擎仍是权威。

### 4.2 长工具输出（`tool_output_compaction`）

工具返回后，如果结果超过 `minChars`（默认 8,000）：

1. 先保留完整结果到 Blob（永远不丢证据）。
2. 调用决策模型，状态包含：工具 id、输出字符数、前 2,000 字符预览、当前任务目标（截断）、是否错误。
3. 问题：

```json
{
  "strategy": {
    "type": "choice",
    "instructions": "这份工具输出应如何进入模型上下文？",
    "criteria": {
      "keep_full": "完整输出对当前任务必要",
      "summarize": "可以用摘要替代，细节留在 Blob",
      "head_tail": "只保留开头和结尾",
      "reference_only": "只需要知道结果存在，引用 Blob"
    }
  }
}
```

4. 执行：
   - `keep_full`：原样进入上下文；
   - `summarize`：调用文本 `summarizer` 角色（若已绑定）；未绑定则退化为 `head_tail`；
   - `head_tail`：确定性保留首尾各 N 字符，中间标记省略与 Blob 引用；
   - `reference_only`：只给一行「结果已保存，引用 <blob>」，由模型决定是否再读。
5. 发布 `ToolOutputCompacted { invocationId, strategy, beforeChars, afterChars, blobRef }`；完整输出仍可通过工具 / 文件读取。
6. 失败安全：超时 / 低置信度 / 不合法 → `head_tail`（确定性），不阻塞工具结果。
7. 已知风险：压缩可能丢掉关键证据；因此不改变原始 Blob，事件可审计，模型可以重新读取完整结果。

### 4.3 后续决策点

- `loop_completion`：判断目标是否完成（`noul`）；低置信度继续循环或交给人。
- `route`：用户消息路由到哪个工具 / 子 Agent（`choice`）。
- 这些在第一版不启用；先验证审批与压缩两个点的收益和阈值。

---

## 5. 阈值与校准

- **不要抄 0.8 / 0.9**：每个决策点、每个动作类别的概率分布不同。
- 上线前用本地样例跑一遍：记录每个样本的概率与人工判断，选择满足误放行预算的阈值。
- 配置按决策点保存；修改阈值只影响后续调用，不改变已提交的决策。
- 低置信度 / 分布接近均匀 → 视为「不知道」，走人工或确定性回退。
- 需要时可提供 `phi-headless decision calibrate --samples <file>` 输出建议阈值；第一版先手工校准。

---

## 6. 安全与隐私

- `state` 可能包含工具参数、命令、文件预览；发送前做 secret 脱敏，并只发送决策所需的最小片段。
- 决策内容需要独立 `EgressGrant`；没有授权时决策点禁用。
- 决策模型与执行 Agent 的模型相互独立；它不能审批自己发起的动作（它本来也不发起动作）。
- 所有自动放行都可审计、可撤销；高风险 / 不可逆永远人工。
- 决策响应是不可信输入：严格校验 schema，绝不把响应文本拼进命令或路径。
- 失败安全统一为「更保守」：审批 → 人；压缩 → 确定性截断；路由 → 默认工具。

---

## 7. 事件

| 事件 | 字段 |
|---|---|
| `DecisionRequested` | `callId`, `point`, `model`, `stateChars`, `questions` |
| `DecisionReturned` | `callId`, `latencyMs`, `inputTokens`, `answers`（概率 / 置信度） |
| `DecisionFailed` | `callId`, `error`, `retryable` |
| `DecisionAutoApproved` | `callId`, `toolId`, `actionDigest`, `threshold`, `probabilities`, `risk` |
| `ToolOutputCompacted` | `invocationId`, `strategy`, `beforeChars`, `afterChars`, `blobRef` |

事件可以落在会话日志（带 `sessionId`）或节点 / 系统日志；不产生 Turn / Run / Step。

---

## 8. UI 与 headless

### 8.1 设置页

- 「决策模型」独立区域：base URL、Key、模型、超时、测试连接。
- 决策点开关与阈值：审批分流、长工具输出；每个点显示最近调用、自动放行次数、人工比例。
- 「智能审批」作用域：允许的工具、命令模式、风险等级；默认只开低风险文件 / 记忆写入，`shell_exec` 默认关闭。
- 审批历史：自动放行记录、概率、阈值、动作摘要、撤销入口。

### 8.2 审批卡

- 自动放行的动作在历史里标注「决策模型自动放行」，可展开概率与阈值。
- 人工审批卡不显示模型猜测，只显示动作、风险、摘要；避免用概率诱导用户。

### 8.3 headless

```bash
phi-headless decision configure --base-url ... --model typesafe/jev-latest --api-key env:OPENROUTER_API_KEY
phi-headless decision test --state "..." --question approve:noul:"是否低风险？"
phi-headless decision history --limit 20
phi-headless decision revoke <grant-id>
phi-headless approvals policy show
```

- 测试用脚本化 decision provider，覆盖高/低概率、超时、schema 错误。
- 所有 headless 输出不含密钥；概率和阈值可打印。

---

## 9. 测试

- 审批：高风险 / 不可逆 / `Deny` 永不调用决策模型；低风险 + 高置信度 + 授权 → 自动放行并审计；低置信度 / 超时 / 错误 → 人工。
- 压缩：四种策略；`summarize` 未绑定 → `head_tail`；完整 Blob 可读；`ToolOutputCompacted` 字段正确。
- 配置：未配置 / provider 删除 / 能力不匹配 → 决策点禁用；阈值修改只影响后续。
- 隐私：`state` 脱敏；无 EgressGrant 禁用；事件不含 secret。
- 不产生 Run：决策调用不触发 `TurnStarted` / `RunScheduled` / `StepStarted`。

---

## 10. 待定

| 问题 | 影响 | 倾向 |
|---|---|---|
| 首批是否自动放行 `shell_exec` | 安全 | 默认否；只允许用户显式命令白名单，且不可逆命令永远人工 |
| 阈值校准工具做到什么程度 | 可用性 | 第一版手工；后续加样例校准命令 |
| 压缩策略是否允许模型选择 `keep_full` | 成本 | 允许，但受上下文预算上限约束 |
| 决策模型不可用时是否允许用户配置「默认放行」 | 安全 | 不允许；失败一律更保守 |
| 是否用于循环终止 | 质量 | 后续；先用确定性的工具调用终止条件 |
