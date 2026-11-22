# 0013. 多 Agent 采用同会话消息线程与子 Run（Lead-Sidekick）

- 状态：已接受（2026-11-22；实现未开始）
- 日期：2026-11-22
- 决策者：Sukirya

## 背景

多 Agent 在原始设计中是 v1 预留（`40-runtime/agent-loop.md` §7），代码尚未实现。调研 Fusion 与 Varin 后，第一版选择**单层 Lead-Sidekick**：Lead 是唯一面向用户的接口，负责规划、审查和选择模型；Sidekick 是执行单元，使用独立上下文，与 Lead 交换 brief / 结果，而不是共享完整对话。可以有多个 Sidekick 并行，但 Sidekick 不允许再生 Sidekick；Sidekick 之间可以定向通信。

现在需要决定：委派落在什么数据单位上——独立 Session、同 Session 的隐藏线程，还是内存中的一次函数调用。

## 决定

- **委派发生在同一个 Session 内**：根 Run（Lead）+ 子 Run（Sidekick），子 Run 带 `parentRunId`，共享同一个 Matter / Turn，**不新建用户轮次**。
- **消息按线程隔离**：`main` 是 Lead / 用户线程；`agent:<agentId>` 是每个 Sidekick 的线程。Lead 不读 Sidekick 的完整轨迹，只收 brief 的结果 / 摘要；context assembler 按线程取历史，provider 缓存前缀不被跨 Agent 污染。
- **每次委派创建持久 `AgentInstance`**：记录角色（Lead / Sidekick）、父 Agent、pinned 模型、thinking、线程、工作模式、状态和报告目标。一个 Agent 可以接收多次消息并产生多个 Run；委派关系可查询、可恢复、可进入。
- **模型由 Lead 在委派时选择**：候选来自运行开始时的 enabled model roster，选择结果 pinned 到 AgentInstance；`delegate` 校验 `ModelSpec::may_delegate_to`（子模型 scale 不得高于 Lead），无效即报错，不静默回退或换渠道。
- **深度固定为 1、扇出有上限**：Sidekick 没有 `delegate` 工具。Sidekick 可以用 `send_message` 定向发给同一委派家族的 Agent（不广播、不能直接对用户）；Lead 用 `wait_for_agents` 等待报告、用 `cancel_agent` 收回控制权，Sidekick 不能阻塞等待或取消同伴。
- **人类仍是唯一审批者**：Sidekick 的写文件 / 执行命令同样经过策略、审批和沙箱；审批卡标注 Agent 身份，Lead 不能代批。多 Agent 不放大权限。
- **文件工作模式先做 `shared` / `read_only`**：同一时刻只允许一个 shared writer（写入租约）；`isolated_write`（worktree + 三方合并）推迟到后续迭代。
- **报告是原执行的结果，不是伪造的回复**：子 Run 终态后，最终助手答复作为 report 交付给请求者；区分 completed / delivered / read 三态，用户没有被告知不等于任务已完成。

## 取舍

- **选同会话消息线程（A）而不是隐藏 Session（B）**：交接单位仍是整个 Session（ADR-0011），委派关系不会跨会话漂移，旧日志也能回放；代价是消息事件、投影和上下文组装要增加 thread 维度，`active_run` 语义要改。
- **选固定 Lead-Sidekick 而不是 group-chat / DAG / 图编排**：符合当前单写者、审批绑定、诚实取消的约束；放弃通用拓扑的灵活性，后续需要时可在 `AgentInstance` 之上扩展。
- **选 Lead 选模型而不是用户配对 / 运行时自动路由**：模型选择前置到委派决策点，候选固定、选择可审计；代价是 Lead 需要看到候选模型 roster，系统提示词会略长。
- **选单写者租约而不是立即做 worktree**：先保证不并行写同一文件；代价是并行 Sidekick 的写能力受限，真正并行写要等 `isolated_write`。

## 后果

- `MessageAppended` 增加 `threadId` / `agentId` / `toAgentId` / `delivery`；`RunScheduled` 增加 `agentId` / `threadId` / `parentRunId`；`SessionView` 增加 agents，`messages` 默认只含 main。
- `active_run` 的语义改为「每个 thread 一个活跃 Run；每个 Turn 一个活跃根 Run」，不能简单按 Session 拒绝第二个 Run。
- 取消传播、审批归属、用量归集、headless JSONL 与 UI 都要按 Agent 标注。
- 换端仍以 Session 为单位，所有 Agent 线程随 Session 一起交接。
- 旧日志默认 main thread，可继续回放。
- 何时重新审视：需要跨会话持久委派、异构 Harness、worktree 合并或超级 Agent 调度时；`AgentInstance` / 消息线程是这些能力的接缝。
