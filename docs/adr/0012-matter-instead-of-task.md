# 0012. 用 Matter（事项）取代隐式 Task 作为长期目标

- 状态：已接受（2026-11-22；实现未开始）
- 日期：2026-11-22
- 决策者：Sukirya
- 取代：ADR-0008

## 背景

ADR-0008 引入 Task 作为顶层目标，但实现把它做成了每次 Run 的隐式关联 id：`plugin-loop` 每个 Run 都创建一个隐藏 Task，Task 没有聚合状态、真实标题、任务树，前端只有 `taskId` 字段。结果是 Task 既没有承担「长期目标」，又和 Session / Turn 的边界重叠：一个会话一轮对话就产生一个 Task，而用户真正在跟踪的「那件事」往往比会话活得更久。

Varin 的调研给出了一个更贴近产品的语义：**事项（matter）** 位于会话之上，可以跨多次执行、跨项目、跨日程、跨时间延续；它保存的是理解和关联（目标描述、相关交流与任务引用），不是实时状态；Run 结束不会自动把事项标成完成。这正是超级 Agent 需要的工作单位。

## 决定

引入 `Matter` 作为长期目标 / 理解容器，取代 Task 的领域语义：

- Matter 是**可选**聚合：`Matter { id, title, understanding, state, links, createdAt, updatedAt, source }`。`understanding` 是自由正文，不是事实表或用户画像。
- Matter 跨 Session、Turn、Run；通过 `links` 关联会话、委派、文件、日程和记忆引用；不保存执行的实时状态。
- `Run.matterId` 可选；Run 结束不自动完成 Matter，只有用户或超级 Agent 显式完成 / 归档。
- **取消「每个 Run 惰性创建隐藏 Task」的行为**；纯问答不创建任何目标实体。
- **删除 Task，而不是兼容**：从领域模型、事件、投影和前端类型中移除 `Task`、`TaskCreated` 与 `taskId`；不保留旧日志兼容。项目处于 early access，没有外部兼容承诺；旧本地开发数据可以清空，旧 Task 事件不再支持。
- 超级 Agent（Bot）以 Matter 看板作为「现在在跟进什么」的单位；调度、主动性和看板实现放在后续迭代。

## 取舍

- **选 Matter 取代 Task**：语义更贴近「长期目标」，避免 Task 与 Turn / Run 竞争；代价是放弃 ADR-0008 的「数据上恒在」不变量，并接受一次不兼容的删除。
- **保留 Task 并扩展**：改动小，但 Task 已经和「一次执行」绑定，继续扩展会把执行状态塞进目标实体，越走越重。
- **保留 Task 做兼容层**：旧日志可回放，但会长期背着两个目标概念；early access 阶段不值得，直接删除更干净。
- **不做目标实体**：会话列表短期够用，但跨会话 / 跨项目的「那件事」没有抓手，超级 Agent 无法建立注意力面板，后续再补的代价更高。

## 后果

- `20-domain/domain-model.md` §3.0 改写为 Matter；`Run.taskId` 删除，改为可选的 `matterId`。
- 新增 Matter 事件（`MatterCreated` / `MatterUpdated` / `MatterLinked` / `MatterCompleted` / `MatterArchived`）；`TaskCreated` 与 `RunImplicitTaskCreated` 删除。
- 前端与 headless 暂不展示 Matter；等超级 Agent 阶段再做看板、日程和主动性。
- 不提供数据迁移：旧日志中的 Task 事件不再支持。早期访问阶段可直接清空本地开发数据。
- 何时重新审视：超级 Agent 的日程 / 跟进 / 团队协作需要更丰富的 Matter 状态，或需要把 Matter 与执行预算绑定的时候。
