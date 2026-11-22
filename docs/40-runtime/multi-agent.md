# 多 Agent：线程化 Lead-Sidekick 委派

> 状态：🌿 评审中 · 后端已实现（2026-11-22）；桌面 UI 待接 · 最后更新：2026-11-22
> 上游：ADR-0012（Matter）、ADR-0013（线程化委派）、`40-runtime/agent-loop.md` §7
> 关联：`20-domain/domain-model.md`、`40-runtime/model-gateway.md`、`40-runtime/auxiliary-models.md`

---

## 0. 一句话

第一版多 Agent 是**同一个 Session 里的 Lead + 多个 Sidekick**：Lead 是唯一面向用户的接口，Sidekick 是独立消息线程里的子 Run；委派是持久事实，报告按请求者路由，人可以进入任何 Agent 的线程查看、审批、取消。

---

## 1. 范围

**这一版做**：

- 单层委派：Lead 可以派发多个 Sidekick；Sidekick **不能**再生 Sidekick。
- Lead 在委派时选择 Sidekick 的模型；候选固定、选择可审计、不得越级。
- Sidekick 之间可以定向通信（不广播、不群聊）。
- 人可以监督：查看任一 Agent 线程、审批其动作、取消其运行、向活跃 Agent 追加消息。
- 文件工作模式先支持 `shared` / `read_only`；同一时刻只有一个 shared writer。
- headless 先行：所有能力都能通过 `phi-headless` 或组合同一 `Node` 的集成测试验证。

第一版**不新增** `strategy.orchestration` 槽位：委派属于运行时骨架（`plugin-loop` / `RuntimePort`）。只有将来需要替换编排策略时，才把它提升为插件槽（ADR-0013）。

**这一版不做**（留给后续）：

- 嵌套委派、动态角色生成、group-chat、DAG/图编排。
- `isolated_write`（worktree 分叉 + 三方合并）。
- 跨 Harness（Codex / Claude Code / ACP）适配。
- 超级 Agent 的日程、主动跟进、跨会话调度、Matter 看板。
- 辅助模型（起标题、记忆整理、生图）——它们走另一条服务调用路径，见 `auxiliary-models.md`。

---

## 2. 层级：Turn / Run / Step 到底谁是谁

这是最容易混的一组词。先给结论，再给当前实现为什么让人感觉不对。

| 层 | 是什么 | 生命周期 | 是否面向用户 |
|---|---|---|---|
| **Matter（事项）** | 一个长期目标 / 理解容器，可跨 Session、Turn、Run | 手动完成或归档 | 是（超级 Agent 看板） |
| **Session（会话）** | 一段交互上下文，也是换端单位 | 可归档、可重新激活 | 是 |
| **Turn（轮次）** | 一次「用户发言 + Agent 全部回应」 | 用户发起到根 Run 终态 | 是 |
| **AgentInstance（Agent 实例）** | 一个持久 Agent 身份：Lead 或 Sidekick；拥有自己的消息线程 | 创建到归档 | 可进入查看 |
| **Run（运行）** | 一次 Agent 行动循环的执行；根 Run 或子 Run | Scheduled → 终态；可恢复 | 间接（用量面板） |
| **Step（步骤）** | Run 内的一次循环迭代（一次模型调用 + 其工具调用） | 开始到完成 | 一般不对用户展示 |

```
Matter（可选，长期）
  └─ Session
       └─ Turn（用户轮次）
            ├─ Run（根 Run = Lead，thread = main）
            │    ├─ Step 1 … Step n
            │    ├─ Run（子 Run = Sidekick A，thread = agent:a）
            │    └─ Run（子 Run = Sidekick B，thread = agent:b）
            └─ …（同一 Turn 不新建用户轮次）
```

**为什么现在会觉得 Run 像 Turn 里的一步**：

1. 当前实现里 `Turn : Run = 1 : 1`。`node.send_message_full` 先开 Turn，再 `start_run`；`run_loop` 结束时发布 `TurnClosed`。所以用户看到的「一次回复」同时是 Turn 和 Run，两者的边界被压平了。
2. 文档里的 Run 又是聚合根：有状态机、检查点、可恢复、可换端。它承担的责任比「一次回复」重得多。
3. 真正的「Turn 里的一步」是 **Step**：一次规划→行动→观察。它已经在代码里存在（`StepStarted/StepCompleted`），只是不面向用户。

**多 Agent 之后必须区分开**：一个 Turn 只有一个活跃根 Run，但可以有 N 个子 Run；子 Run 共享 `turnId`，不新建用户轮次（`I-3-5`）。Step 仍然只属于某一个 Run。以后做超级 Agent 的持久委派时，可以在 Matter 与 Run 之间再加一层 **Dispatch/Operation**（一次交办，可能跨越多个 Run），但第一版 Fusion 不需要。

---

## 3. AgentInstance：持久委派事实

每次 `delegate` 创建一个 `AgentInstance`，而不是一次匿名函数调用。

| 字段 | 说明 |
|---|---|
| `agentId` | `agent_*` |
| `sessionId`, `turnId` | 归属；子 Agent 共享父 Turn |
| `role` | `lead` / `sidekick` |
| `parentAgentId?` | 谁派发的；Lead 为空 |
| `rootAgentId` | 同一条委派链的根（第一版就是 Lead） |
| `model` | pinned 的 `ModelSpec`（id、scale、context window、能力快照） |
| `thinking?` | 本次委派的思考强度 |
| `threadId` | `main` 或 `agent:<agentId>` |
| `workMode` | `shared` / `read_only` |
| `reportTo?` | 报告交付给哪个 Agent；默认派发者 |
| `spawnedByRunId` | 创建它的父 Run |
| `status` | `idle` / `running` / `waiting_approval` / `succeeded` / `failed` / `cancelled` / `archived` |
| `createdAt`, `updatedAt` | |

一个 `AgentInstance` 可以接收多条 brief / 消息，产生多个 Run。Run 是执行，AgentInstance 是身份；身份比单次执行活得久。

---

## 4. 消息线程与上下文隔离

### 4.1 线程

- `main`：用户与 Lead 的线程。用户消息、Lead 的回复、Lead 的工具调用与结果都在这里。
- `agent:<agentId>`：每个 Sidekick 自己的线程。系统提示、brief、Sidekick 的回复、工具轨迹、收到的消息都在这里。

**硬规则**：

- Lead 的上下文只组装 `main` 线程；Sidekick 的上下文只组装自己的线程。
- Sidekick 的完整轨迹**不进入** Lead 的上下文。Lead 只通过 report 看到摘要。
- 旧日志没有 `threadId` 时默认 `main`，可继续回放。
- `SessionView.messages` 默认只返回 `main`；要读某个 Agent 的线程，走单独的查询接口。

### 4.2 消息事件

`MessageAppended` 增加：

| 字段 | 说明 |
|---|---|
| `threadId` | 默认 `main` |
| `agentId?` | 作者 Agent；人类用户为空 |
| `toAgentId?` | 定向消息的接收者；普通消息为空 |
| `kind` | `user` / `assistant` / `tool` / `brief` / `message` / `report` / `system` |

`role`（user/assistant）保留，用于兼容 provider 协议；`kind` 用于产品语义和投影。工具结果仍然以 user role 进入 provider 历史，但 `kind=tool`。

### 4.3 缓存

Fusion 省钱的机制是「两套稳定上下文各自命中缓存」。因此：

- 每个 Agent 的 system prompt、工具集在生命周期内保持稳定；brief 和消息只追加，不重写前缀。
- 同一个 Sidekick 接收多次 brief 时，复用同一线程，历史继续累积；provider 的前缀缓存可以连续命中。
- 不要在每个 brief 前清空 Sidekick 历史；确实需要干净上下文时，显式创建新的 AgentInstance。

---

## 5. 委派协议

### 5.1 Lead 的工具

| 工具 | Lead | Sidekick | 说明 |
|---|---|---|---|
| `delegate` | ✅ | ❌ | 创建 Sidekick，返回 `agentId`；可等待首份报告 |
| `wait_for_agents` | ✅ | ❌ | 等待指定 Agent 的报告或状态变化；只有 Lead 能阻塞等待，避免 Sidekick 互相等待死锁 |
| `send_message` | ✅ | ✅ | 定向发消息；不能发给用户 |
| `cancel_agent` | ✅ | ❌ | 收回控制权 |
| `image_generate` | ✅（生图配置完成时） | ❌ | 独立生图工具，用户选模型，见 `auxiliary-models.md` §6 |
| `fs_*` / `shell_exec` / `memory_*` | ✅ | ✅（受 work mode 与策略约束） | 权限不放大 |

Sidekick **没有** `delegate`，因此深度固定为 1。

### 5.2 `delegate`

```json
{
  "model": "gpt-6.5-luna",
  "thinking": "medium",
  "work_mode": "shared",
  "wait": false,
  "brief": {
    "objective": "实现登录页的 remember-me，并补测试",
    "constraints": ["不要改 public API", "复用现有 session 表"],
    "acceptance": ["cargo test 通过", "手动验证勾选后 7 天内免登录"],
    "context_paths": ["src/login", "src/session"],
    "base": "workspace 当前状态",
    "report": "摘要 + 改动文件 + 测试结果 + 未决问题"
  }
}
```

- `model` 必填。候选来自运行开始时固定的 ModelRoster；Lead 自己不能选 scale 高于自己的模型（`may_delegate_to`）。
- `thinking` 可选；缺省用 provider 默认。
- `work_mode` 可选；`shared` 需要写租约，`read_only` 不提供写工具。
- `wait` 默认 `false`，便于一次派发多个 Sidekick；`true` 等同派发后立即 `wait_for_agents`。
- 无效模型、越级、写租约被占用都**明确报错**，不回退到其他模型。

`delegate` 返回：

```json
{ "agentId": "agent_abc", "threadId": "agent:agent_abc", "status": "running" }
```

### 5.3 `wait_for_agents`

只有 Lead 可以使用；Sidekick 之间的问答通过消息异步完成，避免两个 Sidekick 互相阻塞。

```json
{ "agent_ids": ["agent_abc", "agent_def"], "mode": "all", "timeout_secs": 180 }
```

返回每个 Agent 的状态、report（若已终态）、用量摘要；超时返回 `running` / `waiting_approval`，不假装完成。默认超时有上限（建议 600 秒），避免工具调用无限挂起。

### 5.4 `send_message`

```json
{ "to_agent_id": "agent_def", "message": "登录页的组件在 src/auth/Login.tsx，别重复实现", "mode": "steer" }
```

- 只能发给同一 `rootAgentId` 家族内的其他 Agent；不能发给用户，不能广播。
- `steer`：接收者正在运行时，下个 Step 组装上下文即可看到；空闲时追加后启动一个新的子 Run。
- `queue`：接收者当前 Run 结束后再处理。
- Sidekick 没有 `wait_for_agents`：发出问题后继续自己的工作，回复会在下一个 Step 或新 Run 中出现；需要同步决策时把问题写进报告，由 Lead 处理。
- 系统把消息追加到接收者线程，`kind=message`，`agentId=发送者`，`toAgentId=接收者`。可见性只覆盖发送者、接收者、Lead 和人类；不向所有 Agent 广播。

### 5.5 报告

- 子 Run 进入终态后，最后一条 assistant 消息就是报告正文；失败 / 取消 / 部分结果同样形成报告，`outcome` 字段标明。
- 报告**交付给请求者**（`reportTo`），默认是派发它的 Agent，不总是回到根节点。
- 三态分开：`completed`（子 Run 终态）≠ `delivered`（已追加到请求者线程）≠ `read`（已进入请求者下一次模型上下文）。UI 可以显示「已交付、未读取」，不把「子 Agent 说完成」当成「用户已被告知」。
- 请求者通过 `wait_for_agents` 或下一次 Step 读取报告；人类回复是另一件事，不伪造。

### 5.6 扇出与收尾

- 每个 Turn 的 Sidekick 数量上限可配置（建议默认 4，硬上限 8）；超过时 `delegate` 明确失败。
- 根 Run 不能在还有活跃子 Run 时直接成功收尾。运行时会注入一条系统提示，要求 Lead `wait_for_agents` 或 `cancel_agent`；这是循环保护，不是模型自律。
- 根 Run 进入终态后，遗留子 Run 在宽限期内递归取消（`I-3-6`）；无法确认停止的，标记 `OutcomeUnknown` 并写停止确认。

---

## 6. 权限、文件与安全

### 6.1 权限不放大

- Sidekick 的每次工具调用同样经过 `policy.evaluate`、审批和沙箱；`AgentInstance` 不携带任何额外权限。
- 人类仍是唯一审批者。审批卡标注 Agent id / role / model；Lead **不能**通过工具代批。
- `SandboxPort` 的路径解析、审批摘要绑定、结果未知处理全部复用现有链路。
- Sidekick 不能调用 `delegate`、`cancel_agent`、审批接口或 shell 之外的越权通道。

### 6.2 工作模式与写租约

| 模式 | 能力 | 说明 |
|---|---|---|
| `read_only` | `fs_list` / `fs_read` / `memory_search` | 不暴露 `fs_write` 与 `shell_exec`；用于调研、审查、并行只读任务 |
| `shared` | 全部工具 | 必须持有该 Session / workspace 的**写租约**；同一时刻只有一个持有者 |
| `isolated_write` | 预留 | worktree 分叉 + 合并，第一版不做 |

- 写租约在 `delegate` 时申请；被占用则明确失败，Lead 可以等持有者结束、取消它，或改用 `read_only`。
- 租约在子 Run 终态、取消或失败后释放；释放前不允许另一个 shared Agent 写入。
- `shell_exec` 视为写操作，受租约约束；`read_only` Agent 连 shell 也不暴露，避免用 shell 绕过。
- 这是第一版的诚实边界：**并行只读安全，并行写不安全**。真正并行写要等 worktree + 合并语义。

### 6.3 取消

- 人类或 Lead（`cancel_agent`）取消一个 Sidekick：取消其活动 Run，AgentInstance 进入 `cancelled`，报告写「被收回」，便于 Lead 重新规划。
- 根 Run 取消：递归取消所有活跃子 Run；宽限期后仍无法确认停止的，写 `OutcomeUnknown` 和停止确认，不自动重试。
- 审批等待中的 Sidekick 也要能被取消；审批卡应随之失效。

### 6.4 用量

- 每个 Run 自己的 `ModelCallReturned` 记录 token；按 Agent / Session 聚合展示。
- 不设 Run 级预算硬上限（现行方向）；多 Agent 会放大用量，用量面板和 headless 输出必须按 Agent 分项，避免账单黑箱。

---

## 7. 失败与恢复

- 子 Run 失败 / 取消 / 审批被拒：形成报告交回请求者；Lead 决定重试、换模型、换 Agent 或直接失败，系统不自动重试。
- Sidekick 参数 JSON 损坏、响应截断、结果未知：复用现有的恢复与 `OutcomeUnknown` 语义，按 Run 隔离，不污染其他 Agent。
- 进程重启：从事件重建 `AgentInstance` 与线程；进行中的 Run 标记 `Interrupted`，不自动重放未知副作用。用户可显式重新派发。
- 换端仍以整个 Session 为单位（ADR-0011）：所有 Agent 线程、委派关系、报告随 Session 一起交接；Run 不单独迁移。

---

## 8. 事件与投影

### 8.1 新增 / 扩展事件

| 事件 | 关键字段 |
|---|---|
| `AgentCreated` | `agentId`, `sessionId`, `turnId`, `role`, `parentAgentId?`, `rootAgentId`, `model`, `thinking?`, `threadId`, `workMode`, `reportTo?`, `spawnedByRunId` |
| `AgentStatusChanged` | `agentId`, `status`, `note?` |
| `AgentReported` | `agentId`, `runId`, `outcome`, `reportMessageId`, `deliveredTo` |
| `MessageAppended`（扩展） | 增加 `threadId`, `agentId?`, `toAgentId?`, `kind` |
| `RunScheduled`（扩展） | 增加 `agentId`, `threadId`, `parentRunId?`, `matterId?`；删除 `taskId` |
| `WorkspaceWriteLease` | `sessionId`, `agentId`, `state: acquired/released`, `reason` |

旧日志没有这些字段时按 `main` / Lead / 无父 Run 处理。

### 8.2 投影

- `SessionView` 增加 `agents: AgentView[]`；`messages` 默认只含 main。
- 新增按线程读取的查询：`session_view(sessionId, threadId?)` 或 `agent_view(agentId)`。
- `RunView` 增加 `agentId`、`threadId`、`parentRunId?`、`matterId?`；删除 `taskId`。
- Turn 关闭条件：根 Run 终态，且所有子 Run 已终态或在宽限期内被取消。

---

## 9. 外壳与 headless

### 9.1 Headless

```bash
# Lead 运行；默认允许委派
phi-headless run --provider prov_x --message "..."

# 指定 Lead 模型；Sidekick 模型由 Lead 在 delegate 时选择
phi-headless run --model claude-opus-5-5 --message "..."

# 关闭委派（调试 / 成本控制）
phi-headless run --no-delegation --message "..."

# 查看 / 干预
phi-headless agents list --session <id>
phi-headless agents show <agent_id>
phi-headless agents send <agent_id> --message "..." --mode steer
phi-headless agents cancel <agent_id>
```

- stdout JSONL 的每个事件带 `agentId` / `threadId` / `parentRunId`（适用时）。
- 测试通过脚本化 provider 验证：Lead 调 `delegate` → Sidekick 写文件 → 报告回到 Lead；Lead 上下文不含 Sidekick 轨迹。

### 9.2 桌面 UI

- 侧栏会话之外增加「协作」区：按事件顺序展示派发、成员消息、报告、文件交接；不是群聊广播。
- 可以进入任一 Agent 线程查看完整对话；对活跃 Agent 可以追加消息（`steer`）、审批、取消。
- 用量面板按 Agent 分项显示步数 / token / 状态；根 Run 展开可见子 Run。
- 审批卡标注 Agent 身份和线程；从卡片可以跳到对应 Agent。
- 没有「所有 Agent 群聊」界面；可见性由线程与定向消息决定。

---

## 10. 分期

| 阶段 | 内容 | 验收 |
|---|---|---|
| P0 | 本文件 + ADR-0012/0013/0014；事件与契约草案 | 文档评审通过 |
| P1 | 消息线程、`AgentInstance`、子 Run、同步 `delegate`、模型 roster 与 scale 校验 | headless 集成测试：单 Sidekick 完成写文件并回报 |
| P2 | 异步 `delegate`、多 Sidekick、`wait_for_agents`、`send_message`、`cancel_agent`、写租约 | 并发只读 / 单写者 / 取消传播测试 |
| P3 | 桌面监督界面、协作面板、按 Agent 的用量面板、审批标注 | 手动验收 + UI 测试 |
| P4 | Matter 与超级 Agent：看板、日程、长期委派、主动性 | 后续单独设计 |

每个阶段都必须先有 headless 路径；不依赖本地完整 Rust 工具链，CI 上验证。

---

## 11. 测试矩阵

- **可见性**：Lead 的请求上下文不含 Sidekick 的工具轨迹；Sidekick 的上下文不含 main 历史。
- **缓存**：同一 Sidekick 多次 brief，请求前缀保持稳定，只追加新消息。
- **模型上限**：`delegate` 选更大 scale 的模型被拒绝；无效模型不回退。
- **深度**：Sidekick 的工具列表中没有 `delegate`。
- **审批**：Sidekick 的写 / shell 需要人类审批，审批卡标注 Agent；Lead 不能代批。
- **取消**：根 Run 取消 → 子 Run 取消并写停止确认；单子 Run 取消 → 报告「被收回」。
- **租约**：两个 shared writer 不能同时运行；`read_only` 看不到写工具。
- **报告三态**：完成但未读时 UI/事件不显示「用户已被告知」。
- **回放**：旧日志默认 main；新事件能重建 Agent 树与线程。
- **辅助模型隔离**：起标题 / 记忆整理 / 生图不产生 Turn / Run / AgentInstance（见 `auxiliary-models.md`）。

---

## 12. 待定

| 问题 | 影响 | 倾向 |
|---|---|---|
| 异步报告到达时根 Run 已结束怎么办 | Turn 生命周期 | 根 Run 有活跃子 Run 时不允许收尾；必要时注入系统提示 |
| 用户能否直接把 Sidekick 变成新的根入口 | 产品语义 | 第一版只允许向活跃 Agent `steer`，不允许改根；超级 Agent 阶段再放开 |
| Sidekick 是否需要阻塞等待同伴回复 | 死锁与编排 | 第一版不允许；异步消息 + 报告给 Lead，后续再评估有向等待图 |
| `isolated_write` 的具体合并策略 | 并行写 | 后续单独 ADR；先不做 |
| Sidekick 的工具子集是否可配置 | 权限与成本 | 第一版按 work mode 给固定集；后续按 AgentDefinition 声明 |
| 模型 roster 在系统提示里的体积 | 上下文成本 | 只放 id / scale / tags / context；超过阈值时改用 `list_models` 工具 |
| 子 Run 的 token 是否计入 Session 总账 | 用量 UI | 计入，并按 Agent 分项；不做硬预算 |
