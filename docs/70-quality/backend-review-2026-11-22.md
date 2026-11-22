# 后端架构与工程质量审查（2026-11-22）

> 状态：🌿 评审记录
> 日期：2026-11-22
> 范围：`src-tauri/` 全部 Rust crate（kernel/domain/contracts/plugins/adapters/node/headless/i18n 与桌面壳）
> 性质：只记录发现与建议，不修改实现
> 参照：`docs/10-architecture/module-map.md`、`docs/40-runtime/*`、`docs/70-quality/quality-attributes.md`、`AGENT.md`

审查基于当前工作区代码；行号会随后续提交漂移。本次只做审查，没有改代码。

---

## 0. 结论

宏观分层健康，微观结构开始退化。

健康的部分：`phi-kernel`/`phi-domain` 是叶子，`phi-contracts` 依赖 domain，`phi-plugins`/`phi-adapters` 依赖 contracts/domain/kernel，`phi-node` 是唯一组合根，desktop 与 headless 共享同一个 `Node`；Cargo manifest 真正约束住了依赖方向，不是只写在文档里。事件日志、审批摘要、取消传播、进程树终止这些主线也有明确设计。

正在退化的部分：功能追加后，核心逻辑集中在少数 1.5k–2.2k 行的文件里；契约开始字符串化；适配器边界被内置插件绕开；一些“为未来预留”的抽象没有落地，形成了双轨制和死代码。现在修改成本最高的地方是 `loop_plugin.rs`、`model_protocol.rs`、`phi-headless/main.rs` 和 `Node/Services/compose`。

| 维度 | 判断 | 说明 |
|---|---|---|
| 分层与依赖方向 | 强 | crate 边界和组合根清晰 |
| 模块内聚 | 弱–中 | 多个上帝文件，职责混合 |
| 可演进性 | 中 | 新增工具/可选模型面/run 状态会波及多处 |
| API 契约 | 中–弱 | 状态与错误普遍是本地化字符串 |
| 简洁性 | 中–弱 | 重复 helper、三套 scope 解析、手写 CLI 解析 |
| 测试与 CI | 强 | 约 190+ Rust 测试，32 个真实二进制 E2E；缺 clippy 与生成类型校验 |
| 性能 | 中 | 无 journal 快照/增量读模型，存在可预见的性能悬崖 |

---

## 1. 高优先级问题

### 1.1 `shell_exec` 的“只读白名单”不是只读，也不是工作区内边界

`policy.rs` 根据命令文本把 `cat/rg/grep/sed/sort/git grep` 等直接判为 `ReadOnly` 并自动放行（`src-tauri/crates/phi-plugins/src/policy.rs:274-276`、`352-367`）。但 `shell_exec` 实际运行在用户权限下，路径不受 `FsSandboxPlugin` 限制；只有 `fs_*` 工具走 `resolve_path`。

可直接读取工作区外内容：

- `cat ~/.ssh/id_rsa`
- `rg API_KEY ~`

多个“只读”命令本身可以执行子进程：

- `rg --pre '...' pattern .`
- GNU `sed` 的 `e` 命令：`sed -n '1e id' file`
- `sort --compress-program=...`
- `git grep -O ...` / `git --ext-diff`

`context.rs:53-59` 的系统提示写着“工作区外路径会被拒绝”，这对 `shell_exec` 不成立。

建议：把 `shell_exec` 视为独立风险层。只自动放行经过结构化校验的 argv 模板，并显式排除会执行子进程的选项；否则默认走审批。真正的工作区隔离需要 OS 级沙箱或能力令牌，而不是命令文本分类器。至少先修正白名单和提示文案。

### 1.2 Journal torn-tail 修复会导致下一次启动失败

崩溃后最后一行可能没有换行符。`JsonlJournal::append` 在下一次追加时会先补一个换行（`src-tauri/crates/phi-adapters/src/fs_journal.rs:36-49`），把这条截断记录变成文件中间的一行。`EventBus::restore` 只容忍“最后一行”不可解析，中间行不可解析会直接返回错误拒绝启动（`src-tauri/crates/phi-kernel/src/event_bus.rs:152-163`）。

失败时序：

1. 崩溃留下半行，没有换行。
2. 下次启动时它是最后一行，`restore` 跳过它，节点正常启动。
3. 第一次 append 给半行补上换行，再追加新事件。
4. 再下一次启动时，半行位于中间，`restore` 报错，节点无法启动。

建议：修复 torn tail 时截断到最后一个完整换行，而不是补换行；补一个“半行 + 重启两次”的回归测试。

---

## 2. 架构与工程质量

### 2.1 上帝文件

按生产代码行数（去掉内联测试）：

| 文件 | 总行数 | 生产代码 |
|---|---:|---:|
| `phi-plugins/src/loop_plugin.rs` | 2255 | 2142 |
| `phi-plugins/src/model_protocol.rs` | 2256 | 1535 |
| `phi-headless/src/main.rs` | 1954 | 1953 |
| `phi-plugins/src/session.rs` | 1579 | 1192 |
| `phi-node/src/node.rs` | 1167 | 1166 |
| `phi-plugins/src/provider_store.rs` | 890 | 676 |
| `phi-adapters/src/memory_sqlite.rs` | 888 | 887 |
| `phi-plugins/src/auxiliary.rs` | 834 | 833 |

职责混合情况：

- `loop_plugin.rs`：run 生命周期、委派校验、上下文组装、记忆注入、工具执行、策略/审批、输出压缩、角色提示、错误恢复。
- `model_protocol.rs`：三种协议的请求构造、响应解析、SSE 解码、错误分类、参数恢复。
- `session.rs`：投影数据结构、事件折叠、读模型、崩溃恢复、上下文用量统计。
- `main.rs`：CLI 参数解析、所有命令实现、输出格式、帮助文本。
- `node.rs`：组合根 + 所有应用服务入口 + 部分业务校验。
- `auxiliary.rs`：辅助角色配置 + 记忆维护的完整业务逻辑。

建议的目标结构（先目录化，不急于拆 crate）：

```text
phi-plugins/src/
  runtime/        loop/{mod,run,context,tool_pipeline}.rs
  model/          gateway.rs, protocol/{anthropic,chat,responses,common}.rs
  session/        projection.rs, view.rs, recovery.rs, port.rs
  tools/          builtin.rs, subagent.rs, image_tool.rs
  memory/         (保持现状，作为正面样板)
  auxiliary/      roles.rs, maintenance.rs
  providers/      store.rs, catalog.rs
  decision/, image/, policy/, sandbox/, approver/
phi-node/src/node/
  mod.rs, compose.rs, sessions.rs, providers.rs, memory.rs,
  image.rs, decision.rs, auxiliary.rs, runtime.rs
phi-headless/src/
  main.rs (只做入口)
  cli/{parse,help}.rs
  commands/{run,sessions,providers,memory,image,decision,roles,status}.rs
```

Rust 允许在多个文件里对同一个 `Node` 写 `impl`，拆分不会破坏现有 API。

### 2.2 适配器边界被绕过

`docs/10-architecture/module-map.md` 的 R5 写着“插件里的领域逻辑不得出现文件系统、socket、UI、数据库驱动等平台符号”。当前大量 I/O 直接写在 `phi-plugins` 里：

- `tools.rs` 直接 `std::fs` 读写工作区、写 backup。
- `provider_store.rs` 直接读写 `providers.json`、做原子替换。
- `json_store.rs` 是插件内的通用文件存储。
- `assets.rs` 直接管理图片文件。
- `sandbox.rs` 直接起子进程。
- `decision.rs`、`image.rs`、`model_gateway.rs` 直接发 HTTP。

`phi-adapters` 实际只覆盖 journal、blob、app config、SQLite memory、control files。结果是适配器层名存实亡，换平台、换存储、做插件替换或纯内存测试时要改很多插件文件。

建议二选一：

1. 把设置存储、资产存储、HTTP client、进程执行器收进 `phi-adapters`；或
2. 明确修改 R5，承认“官方内置插件可以直接用平台 API，只有第三方插件走端口”。

现状“文档禁止、代码照做”是最危险的状态。

### 2.3 组合根按功能线性膨胀

`Services` 是 12 个 `RwLock<Option<Arc<dyn ...>>>` 的服务袋（`phi-contracts/src/services.rs:16-29`），`Node` 有 55 个公开方法，`compose` 从 154 行写到 417 行。新增一个可选产品面（image/decision/auxiliary）需要同步修改：

- contracts DTO + port
- plugin 实现
- `Services` 字段 + getter/setter
- `compose` 构造与注册
- `Node` 方法
- Tauri command + `bridge.ts`
- headless 的 `Command`、`execute`、`parse_*`、help
- 事件类型、投影、测试

建议做单一 composition manifest：一张表声明“slot -> 服务类型 -> constructor -> 是否必需 -> 是否暴露给 shell”，`Services`、`Kernel` 注册、`Node` 的缺失判断都从这张表生成。

`required slots` 目前有三套来源：

- `phi-contracts/src/lib.rs:16-39` 的常量
- `Services::missing_slots` 的硬编码映射（`services.rs:135-154`）
- `Node::compose` 再单独问 registry（`node.rs:361-362`）
- 每个 plugin 的 `activate` 里再写一次 slot 字符串

加一个必需槽位要改四处，应合并成一套类型化的能力清单。

### 2.4 微内核/生命周期目前更像“仪式”

`Kernel` 的 `evaluate_mode`、`deactivate_plugin`、`active_plugins` 在非测试代码里没有调用者；`RunMode::Degraded` 从未被设置；`Plugin::deactivate` 默认实现是空操作。真正的插件装配发生在 `compose` 里的手工 `set_*()`，`Registry` 只是另一份元数据。

这不意味着要删除 kernel，但应明确它当前的职责：组合期元数据 + 启动日志，而不是运行期热插拔。文档里的 Degraded 场景应标注为未实现。

### 2.5 工具 ID 是隐式全局耦合

工具 ID 字符串散落在：

- 工具定义：`tools.rs`、`subagent.rs`、`memory/mod.rs`、`image.rs`
- 策略：`policy.rs:466-561` 一个巨大的 `match tool_id`
- 循环：`loop_plugin.rs:617-626` 过滤工具、`1268` 写租约、`1297/1334` 风险等级、`1555/1583` 副作用确认、`1736` 内容哈希、`2062-2096` 审计预览
- 契约：`util.rs:130-142` 对 `shell_exec` 的特殊处理
- 决策默认值：`decision.rs:58` 的 `shell_exec` / `fs_write`

新增一个工具不是局部改动，而是要同时找到策略、循环、审计、租约的硬编码分支。`ToolDefinition` 已经有 `SideEffect` 字段（`ports.rs:18-34`），但全仓库只写不读，策略完全没有使用它。

建议把工具元数据升级为能力描述：

```rust
enum ToolCapability {
    WorkspaceRead,
    WorkspaceWrite,
    Shell,
    MemoryWrite,
    AgentControl,
    ExternalEgress,
}
```

让策略、租约、风险等级、审计预览消费同一份元数据。`SideEffect` 要么接入这条链路，要么删掉。

### 2.6 文档承诺与代码的落差

需要区分“已记录的 MVP 取舍”和“没有记录但代码没做到”：

- 已记录：插件编译期内置、无可热插拔；检查点/预算是目标态；TS 插件 ABI 未实现。
- 未对齐：R3 要求每个端口有 Null 实现，实际只有 `NullMemoryStore` 且无人使用；R5 被大量插件违反；“可插拔循环策略”在代码里是单一 `LoopPlugin`，没有 `LoopStrategy` 端口；`Degraded` 从未产生；`EventRecord.schema_version` 写了但从不读取。

---

## 3. 代码质量与简洁性

### 3.1 死代码/死契约

仓库内没有消费者的公开项：

- `SessionPort::messages`（`ports.rs:381`）从未调用。
- `SessionPort` 的 `agents/agent/lead_agent/messages_for_thread/append_message_in_thread/active_root_run/active_run_for_agent/write_lease_holder` 都有默认实现（`ports.rs:387-423`），但只有一个实现者；默认值让“部分实现”可以静默丢功能。
- `ModelPort` 的 `roster/resolve_model/resolve_model_on/invoke_on` 默认实现同理。
- `NullMemoryStore`（`contracts/memory.rs:340-412`）未使用。
- `Kernel::evaluate_mode`、`deactivate_plugin`、`active_plugins` 只被测试使用。
- `ClockPort::monotonic_ms` 没有调用者。
- `StreamHub::set_sink` 没有调用者。
- `ModelSpec::display_name`、`ModelSpec::may_delegate_to`、`SecretRef::env` 没有生产调用者。
- `MemoryMaintenanceFailed` 事件从未发布。
- `SqliteMemoryStore.path` 带 `#[allow(dead_code)]`。
- `StartRunRequest.role` 从未被 `loop_plugin` 读取，实际角色由 `is_child` 推导（`loop_plugin.rs:239-243`）。
- `ModelResponse.estimated_usage` 一路传递但没有任何消费者。
- `SideEffect` 只写不读。
- Tauri 的 `set_api_key`、`get_model_status`、`get_run_status`、`fetch_models` 在 `src/bridge.ts` 没有对应包装，当前 UI 不调用；`Node::send_message` 和 `send_message_with_provider` 也没有调用者。

CI 目前没有 clippy，这类问题不会被自动发现。建议加 `cargo clippy --workspace --all-targets -- -D warnings`；公开 API 的死项还需要人工审计。

### 3.2 重复代码

- `tool()` 定义 helper 在 `tools.rs:48-67`、`subagent.rs:627-645`、`memory/mod.rs:646-664` 各写一遍。
- scope 解析写了三次：`memory/mod.rs:368-379`、`headless/main.rs:1373-1384`、`api.rs:214-223`。
- `ProviderStore` 的 active-id 修复逻辑出现三次：`provider_store.rs:50-69`、`339-358`、`371-377`。
- `SessionPlugin` 的 `sync_agent_status`、`sync_agent_progress`、`add_agent_tokens` 是三个几乎相同的“遍历所有 session 找 run_id”函数（`session.rs:1092-1146`）。
- `loop_plugin` 里“构造 tool_result 错误块”和“journal write failed”消息重复了 4–6 次（如 `loop_plugin.rs:920-987`、`1265-1327`）。
- `decision.rs` 的四个失败分支都在重复 `bus.publish(DecisionFailed)` 模板（`decision.rs:389-455`）。
- `image.rs` 的 `max_per_turn` / `max_bytes` 计算（`image.rs:195-209`）过度绕；等价于“0 用默认值，其他 clamp 到 1..8”。
- headless CLI 手写了整套 flag 解析、数字错误、帮助文本（`main.rs:794-1852`、`1876-1954`）。约 1000 行只是在做 clap 可以做的事；如果不引入依赖，也应抽象一个小的 `FlagSet`。

### 3.3 补丁式/占位语义

契约里承诺了语义，实现里没有：

- `PolicyEffect::Modify` 被接受，但执行时直接 `deny`（`loop_plugin.rs:1313-1317`）。
- `send_message` 的 `mode` 参数只做校验和回显，`steer` 与 `queue` 行为完全相同（`subagent.rs:439-451`）。
- `image_generate` 的 `reference_assets` 出现在 schema 里，但非空时直接返回“不支持”（`image.rs:560`、`648-653`）。
- `keychain:` 凭证引用明确未实现，但仍在设置 UI 中暴露。
- `schema_version` 固定为 1，从不读取。
- `Kernel` 发布的 `PluginActivated`/`PluginFailed`/`NodeModeChanged` 是 `json!()` 原始事件，不在 `DomainEvent` 里。

这些要么实现，要么从对外 schema 中移除；不要长期保留“有这个字段但没这个能力”的状态。

### 3.4 配置与持久化语义不一致

- `ConfigStore::update` 先改内存再写文件（`adapters/config.rs:42-55`）：写失败时调用方收到错误，但内存里的新值已经生效，直到重启。`JsonStore::update` 先写文件、成功后才替换内存（`plugins/json_store.rs:44-59`）。两种设置存储的失败语义不同。
- `ProviderStore::load` 在主文件和备份都损坏时静默回退到空配置（`provider_store.rs:43-46`），没有日志；用户会看到 provider 全没了。
- `provider_store` 的 secret 以明文写 `providers.json`，Unix 上事后 chmod 0600；`image.json`、`decision.json` 同样。keychain 未实现是已知项，但应在产品文案和设置页明确“当前本地明文存储”。

### 3.5 隐式环境变量

`loop_plugin.rs` 在业务逻辑里直接读：

- `PHI_MAX_OUTPUT_TOKENS`（`:638`）
- `PHI_STREAM`（`:754`）
- `PHI_APPROVAL_TIMEOUT_SECS`（`:1340`）
- `env_u64` / `env_bool`（`:2126-2141`）

`provider_store` 还读 `PHI_ANTHROPIC_BASE_URL`、`PHI_MODEL`、`ANTHROPIC_API_KEY`、`PHI_API_KEY`（`:396-430`）。

这些既不在设置 UI，也不在文档中系统性列出；行为会因环境不同而不同，测试只能靠 `env` 全局变量。建议收敛成一个 `RuntimeConfig`，环境变量只在启动时作为覆盖层解析一次。

---

## 4. API 设计与契约

### 4.1 错误和状态全是字符串

- 所有端口返回 `Result<T, String>`，Tauri command 也返回 `Result<T, String>`；错误是本地化后的自然语言。前端无法按错误类型分支，测试靠英文子串匹配。
- `ApprovalOutcome.status` 是 `String`，代码里直接比较 `"Approved"`（`loop_plugin.rs:1381`）。
- `RunStatus.state`、`RunView.state`、`SessionSummary.last_run_state` 都是 `String`；状态字符串在 `session.rs`、`loop_plugin.rs`、`headless/main.rs` 约 30 处出现。
- `ModelStreamEvent.kind` 是 `String`（`stream.rs:18`）。
- 沙箱取消用 `Err("cancelled")` 哨兵，循环再按字符串比较（`sandbox.rs:168`、`loop_plugin.rs:801`）。

建议：

- 一个 `AppError { code, params, message }`，`code` 稳定、可枚举；本地化只发生在 shell。
- 一个共享的 `RunState` 枚举（`Scheduled/Running/WaitingApproval/Succeeded/Failed/Aborted/AwaitingReconciliation`），`RunView`、`RunStatus`、headless 退出码、前端生成类型全部复用它。
- `ApprovalStatus`、`StreamKind` 同理。

### 4.2 两套 Run 读模型

`RunStatus`（`ports.rs:468-481`）和 `RunView`（`domain/view.rs:37-61`）字段高度重叠，`Node::run_status` 手工转换（`node.rs:1000-1019`）。建议只保留一个读模型，或让 `RunStatus` 成为 `RunView` 的窄视图。

### 4.3 “未找到”的语义不统一

- `get_run_status` 返回 `Option`，不返回 `Result`（`api.rs:107-110`）。
- `cancel_run` 返回 `Result<bool>`，`false` 同时表示“不存在”和“不可取消”。
- `delete_session` 对未知 session 返回 `Err`，但 `remove_provider` 返回 `false`。
- `get_session` 返回 `Err`，`session_view` 返回 `Option`。

建议统一为 `Result<T, NotFound>` 或统一为 `Option`，不要把“不存在”和“操作失败”混在布尔值里。

### 4.4 Tauri 同步 command 与死命令

`api.rs` 里除网络调用外几乎都是同步 `fn`，包括 `get_session`、`list_memories`、`get_bootstrap`、`set_workspace`。Tauri 2 默认在主线程执行同步 command；这些方法背后是 journal 投影、SQLite、文件 I/O 和整份 session 序列化，长会话时会阻塞原生事件循环。网络类 command 已经是 `async`，磁盘类也应改为 `async` 或 `spawn_blocking`。

`set_api_key`、`get_model_status`、`get_run_status`、`fetch_models` 当前没有前端调用者；`Node::send_message` 包装也没有。要么删掉，要么在 bridge 里补齐并明确用途。

### 4.5 每消息模型选择没有改变上下文预算

`LoopPlugin` 用 `model.status().context_window` 计算历史预算（`loop_plugin.rs:639-640`），但 `ModelGateway::status` 返回的是 active provider 主模型的上下文窗口（`model_gateway.rs:437-470`），不是本次请求解析后的 `resolved_model`。用户切到另一个模型或子代理使用另一个模型时，预算和 `ModelCallIssued.context_window` 仍按主模型算，可能过早裁剪或超出真实窗口。

建议从 roster 按 `resolved_model` 取 `ModelSpec.context_window`，并记录本次调用实际使用的值。

### 4.6 记忆的 Principal 不一致

- 自动注入用 `ctx.agent_id` 和 `ctx.run_id`（`loop_plugin.rs:2013-2022`）。
- 记忆工具 `tool_principal` 固定 `agent_id: Some("main")`，忽略 `ctx.agent_id`（`memory/mod.rs:309-316`）。

子代理通过工具写入的 agent-scope 记忆归属于 `main`，但它在自动检索时按子代理 id 查，看不到自己刚写的那条；显式 `memory_search` 又按 `main` 查，能看到。需要明确“agent scope 是共享还是按 agent 隔离”，然后统一来源。

### 4.7 事件与本地化耦合

`DomainEvent` 里很多字段存的是本地化文本：

- `ApprovalRequested.risk` 是本地化风险句（`events.rs:316-325`）。
- `RunFailed.error`、`RunInterrupted.note`、`AgentStatusChanged.note` 都是本地化字符串。
- 工具结果和循环错误也直接写本地化文本，随后作为对话内容回传给模型。

后果：

1. 日志/事件回放的语言取决于写入时的全局 i18n 状态，切换语言后旧历史不会重译。
2. 事件不再是与语言无关的领域数据，schema 变化和跨端同步都会受影响。
3. 模型上下文会随 UI 语言变化，工具错误、停止原因等会以中文进入 provider 请求。

建议事件只存稳定 code + 参数；本地化在投影/UI 层完成。工具返回给模型的内容保持英文或代码化。

### 4.8 事件版本与兼容

`schema_version` 在 `EventRecord` 里固定为 1，从未被读取（`event_bus.rs:20`、`106`）。`SessionPlugin::apply_record` 解析失败时静默 `return`（`session.rs:744-748`），与 `docs/50-distributed/local-first.md:156-159` 要求的“未知事件保留原文、不得丢弃”相反。将来改一个字段类型，旧 journal 里的相关事件会被静默跳过，会话投影悄悄缺数据。

建议：解析失败要记录日志并计数；引入最小 `upcast`/`fold` 机制或明确不兼容策略。如果项目还处在可清空本地数据的阶段，也应在启动时给出明确提示。

---

## 5. 性能与效率

多数是“可预见的悬崖”，不是当前故障：

1. 每次 CLI 命令都恢复整个 journal：`open_node` 无论命令是否需要会话投影，都会 `bus.restore()` 全量解析 journal，再 `SessionPlugin::load()` 全量折叠（`node.rs:219-227`、`fs_journal.rs:59-74`）。`providers list`、`memory stats` 这类命令也被迫承担全部历史。journal 没有快照、分段或压缩，启动成本随历史线性增长。
2. 每个事件触发整份 session 快照：桌面端 `scheduleRefresh` 120ms 防抖后调用 `get_session`，返回所有消息和 blocks（`usePhiStream.ts:37-42`、`session.rs:471-515`）。前端已用 `reuseMessages` 缓解渲染，但 IPC 序列化和克隆仍是 O(消息数)。`docs/70-quality/frontend-review-2026-11-20.md:85` 已把“没有 cursor/增量”记为 ARCH-04。
3. 无界缓存：`SessionPlugin.blocks` 以 `content_ref` 为 key 永久缓存（`session.rs:137`、`704-720`）；`ImagePlugin.turn_counts` 以 `session:turn` 为 key 永久累积（`image.rs:83`、`666-671`）。
4. 全局 publish 锁包住 sink：`EventBus::append_raw` 在持有 `publish_lock` 时同步调用所有 sink（`event_bus.rs:96-132`），包括 Tauri emit 和 stdout 写入。慢 sink 会阻塞所有事件发布；如果将来某个 sink 内部再 `publish`，会死锁。
5. O(sessions) 扫描：`sync_agent_status/progress/add_agent_tokens` 每个事件都遍历所有 session 找 run（`session.rs:1092-1146`）；`find_run`、`agent`、`active_run_for_agent` 同样是跨 session 扫描。
6. 每事件 fsync：`JsonlJournal::append` 每次都 `flush + sync_data`。审计可靠性优先是合理取舍，但事件粒度较细时需要知道这是有意的成本。

---

## 6. 测试与 CI

优点：

- 约 190+ 个 Rust 测试，其中 32 个是真实 `phi-headless` 二进制的端到端测试，覆盖协议、审批、取消、崩溃恢复、多代理、记忆、图像/决策。
- `model_protocol.rs` 有 28 个协议/SSE/参数恢复测试；`provider_store` 11 个；`policy` 9 个；内存有 golden set。
- CI 在 Windows 和 Linux 都跑，fmt/frontend typecheck/lint/build/rust tests 都有。

缺口：

- 没有 `cargo clippy`。
- 没有检查 `src/generated` 是否与 Rust DTO 同步；`export-types` 是手工命令。
- 没有 torn-tail + 两次重启的回归测试。
- `loop_plugin` 只有 6 个单元测试，`tools.rs`、`subagent.rs` 没有单元测试。
- `phi-i18n` 的翻译完整性测试只检查了 17 条手选消息（`lib.rs:706-729`）；新增消息忘记翻译时会静默回退英文。
- 协议解析、SSE 分帧、参数恢复都是纯函数，适合加 fuzz/property 测试；目前没有。
- `memory_golden` 是非阻塞基线，检索质量没有 CI 门槛。

---

## 7. 建议的收口顺序

1. 修 shell 白名单与 journal torn-tail，各补一个回归测试。
2. 机械拆分四个上帝文件：`loop_plugin`、`model_protocol`、`session`、`headless/main`；只移动代码、不改行为。
3. 统一状态与错误契约：`RunState`、`ApprovalStatus`、`StreamKind`、`AppError`；重新生成 DTO。
4. 收敛组合根：一张 composition manifest 取代 `Services` + `Registry` + `Node` 三处 slot 判断；删掉死 API 和死事件。
5. 把工具元数据接入策略/租约/审计：新增工具只改工具定义。
6. 明确适配器边界：要么把 provider/json/asset/HTTP/process 移进 `phi-adapters`，要么修改 R5 并承认内置插件例外。
7. 事件演进与读模型增量：`schema_version` 真正生效，解析失败可见；再考虑 journal 快照/分段和 session cursor。

---

## 8. 文档影响

- 本文件登记在 `docs/README.md`。
- 未修改 `docs/10-architecture/module-map.md` 的依赖规则；R3/R5 与现状的落差已记录在 §2.2、§2.6，待后续决策后同步。
- 本次只做审查，未修改代码。
