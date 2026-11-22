# 辅助模型：角色、服务调用与后台任务

> 状态：🌿 评审中 · 最后更新：2026-11-22
> 上游：ADR-0014、`40-runtime/model-gateway.md` §4.1 / §8
> 关联：`40-runtime/policy-and-approval.md`、`40-runtime/context-and-memory.md`、`40-runtime/multi-agent.md`

---

## 0. 一句话

辅助模型不是 Agent，也不是 Run：它们是**按角色绑定的服务调用 / 后台任务**，有自己的审计、用量和外发授权，但不进入 Turn / Run / 执行树。本文件覆盖**文本辅助角色**；生图与决策模型是两条独立能力，分别见 §6 与 `decision-model.md`。

---

## 1. 角色

| 角色 | 用途 | 调用形态 | 输出 | 状态 |
|---|---|---|---|---|
| `session_title` | 为会话生成短标题 | 后台单次文本调用 | 一行 ≤ 60 字符 | 第一版 |
| `memory_maintenance` | 合并 / 拆分 / 纠正 / 更新记忆概述 | 后台批量调用 + 本地管理操作 | 受限 JSON 提案 | 第一版 |
| `approval_judge` | 辅助审批判定 | 受限短提示 | allow / deny / ask + 理由 | 后续（或由决策模型承担，见 ADR-0015） |
| `summarizer` | 上下文 / 长工具输出压缩 | 受限调用 | 摘要文本 | 后续 |
| `embedding` | 检索嵌入 | 受限调用 | 向量 | 后续 |

> **生图不在这张表里**：它是独立的 `image.json` + `image.generate` 端口，模型由用户选择，Agent 主动调用工具；见 §6 与 ADR-0016。

角色注册表允许插件扩展，但新增角色必须同时定义：能力要求、输入裁剪规则、输出 schema、外发内容类别、超时和审计事件。

---

## 2. 绑定与配置

### 2.1 绑定

- 应用级绑定，保存在 `<data-dir>/auxiliary.json`：`role -> { providerId, modelId, thinking?, enabled }`。生图与决策模型不使用这个文件（分别用 `image.json` / `decision.json`）。
- 保存时校验：provider 存在、模型存在且启用、模型具备角色要求的能力（文本角色要 `chat`）。
- 未绑定 / provider 被删除 / 模型被禁用：功能禁用，并在设置页和 headless 状态里说明原因；**不回退**到 Lead 或其他模型。
- 第一版不做按会话覆盖；需要时以后再加 `sessionId` 维度的覆盖。

### 2.2 外发授权

每个角色声明自己发送的内容类别，沿用 `model-gateway.md` §7.2 的 `EgressGrant`：

| 角色 | 内容类别 |
|---|---|
| `session_title` | 会话文本（首条用户消息 + 首条助手回复，截断） |
| `memory_maintenance` | 记忆事实 + 相关 episode 片段 |
| `approval_judge` | 工具名、参数摘要、策略上下文 |
| `summarizer` | 待压缩的工具输出 / 上下文片段 |
| `embedding` | 待嵌入文本 |

生图与决策模型的外发授权分别在 ADR-0016 / ADR-0015 定义。

没有对应授权时，角色即使绑定也不能运行。首次启用某个角色时明确告知接收方和用途，授权后正常调用不反复弹窗。

### 2.3 headless

```bash
phi-headless roles list
phi-headless roles set session_title --provider prov_x --model gpt-6.5-luna
phi-headless roles clear memory_maintenance
phi-headless roles test session_title --session <id>
# 生图使用独立配置，不走 roles
phi-headless image configure --provider openai_images --base-url ... --model gpt-image-2.6
```

`roles list` 输出绑定、能力校验、外发授权状态和最近一次调用结果（不含敏感内容）。

---

## 3. 调用模型：服务调用，不是 Run

### 3.1 事件

辅助调用发布自己的审计事件，不复用 Run 的 `StepStarted`：

| 事件 | 字段 |
|---|---|
| `AuxiliaryCallIssued` | `callId`, `role`, `model`, `sessionId?`, `matterId?`, `inputChars`, `purpose` |
| `AuxiliaryCallReturned` | `callId`, `inputTokens`, `outputTokens`, `cacheReadTokens?`, `cacheWriteTokens?`, `finishReason`, `durationMs` |
| `AuxiliaryCallFailed` | `callId`, `error`, `retryable` |
| `SessionTitleSuggested` | `sessionId`, `title`, `model`, `callId`, `applied` |
| `SessionRenamed` | `sessionId`, `title`, `source: user/auxiliary/import` |
| `MemoryMaintenanceStarted` | `runId?`, `batchSize`, `trigger` |
| `MemoryMaintenanceProposed` | `proposalCount`, `kinds`, `callId` |
| `MemoryMaintenanceApplied` | `appliedCount`, `skippedCount`, `reason`, 不含事实正文 |

生图事件（`ImageConfigured` / `ImageGenerated` / `ImageDeleted`）属于 §6 的独立生图路径。

这些事件可以落在会话日志（带 `sessionId`）或节点 / 系统日志（记忆整理、无会话归属的调用），但**不得**产生 `TurnStarted` / `RunScheduled` / `StepStarted`。

### 3.2 受限调用路径

- 文本角色走模型网关的 `callClass=auxiliary`：固定短提示、低温度、`stream=false`、严格 `maxOutputTokens`、不组装完整会话历史。
- 文本辅助调用**不进审批链**（防递归）；它们的副作用由所属任务自己约束：记忆整理走记忆管理操作。生图是独立的工具调用，按 §6 的策略与外发授权处理。
- 超时：标题 20 秒，记忆整理 120 秒，生图 120 秒；超时即失败，不无限挂起。
- 可取消：会话归档取消标题任务；节点退出取消全部后台任务；取消后不重试。

---

## 4. 会话标题

### 4.1 触发

- 会话的首条助手回复持久化之后触发；只在该会话标题仍是默认值（`New session`）且 `titleSource != user` 时运行。
- 每个会话最多一个进行中的标题任务；重复触发直接忽略。
- 节点处于只读 / Maintenance 模式时跳过。

### 4.2 输入与输出

- 输入：首条用户消息 + 首条助手回复，各截断到 2,000 字符；固定系统提示要求「输出一行标题，不加引号、不加句号、不超过 60 字符」。
- 输出校验：单行、非空、≤ 60 字符、去除 Markdown 标记；不通过则保持默认标题，只记 debug 日志，不向用户报错。
- 应用：发布 `SessionTitleSuggested`；满足条件时自动发布 `SessionRenamed { source: auxiliary }` 并更新投影。
- 用户手动改名后 `titleSource=user`，后续不再自动覆盖。

### 4.3 UI

- 侧栏会话标题直接更新；不弹 toast。
- 设置页或会话菜单可以显示「标题由 <model> 生成」，并提供重新生成 / 关闭该角色。

---

## 5. 记忆整理

### 5.1 触发

- 写入阈值：自上次整理后新增 N 条已准入事实（建议 N=20）；
- 空闲：数据目录 10 分钟没有写入；
- 显式：设置页「立即整理」或 `phi-headless memory maintain`。
- 每个数据目录单飞；与写入共用数据目录锁；provider 不可用或外发未授权时跳过并记录原因。

### 5.2 输入

- 一批 active 事实（id、scope、trust、confidence、updatedAt、fact 文本）与相关 episode 摘要；批量上限（建议 200 条 / 60k 字符）。
- 不发送整个会话历史；不发送已 forget 的内容。

### 5.3 提案格式

```json
{
  "operations": [
    { "op": "merge", "ids": ["mem_a", "mem_b"], "fact": "合并后的完整事实", "reason": "duplicate" },
    { "op": "supersede", "oldId": "mem_c", "fact": "新事实", "reason": "newer evidence" },
    { "op": "correct", "id": "mem_d", "fact": "修正后的事实", "reason": "contradiction" },
    { "op": "invalidate", "id": "mem_e", "reason": "no longer true" }
  ]
}
```

校验：id 必须存在且属于同一可见范围；fact 长度、有效区间、secret 脱敏走现有准入屏障；不允许跨用户 / 跨项目提升可见性。

### 5.4 应用策略

| 操作 | 默认 |
|---|---|
| `merge` 重复事实 | 自动应用（审计 + 可查看） |
| `supersede` 新事实替换旧事实 | 自动应用（审计） |
| `correct` / `invalidate` | 进入「待确认」，用户在记忆页批准后应用 |
| `forget` | **永不自动**；只能由用户显式执行 |

- 用户 scope 的事实更敏感：`correct` / `invalidate` 一律待确认；`merge` / `supersede` 也建议默认待确认（可在设置中改为自动）。
- 应用走记忆插件现有的 `supersede` / `correct` / `invalidate` owner 操作；审计事件只记 id、counts、reason，不记事实正文。
- 后台任务不直接改 SQLite；通过记忆端口写入，保持单写者与准入屏障。

### 5.5 UI 与 headless

- 记忆设置页增加「后台整理」：上次运行、触发原因、应用/跳过数量、待确认操作、立即运行、查看历史。
- `phi-headless memory maintain --dry-run` 输出提案；`--apply` 应用安全操作；`--review` 列出待确认项。

---

## 6. 生图：独立配置，用户选模型，Agent 主动调用

生图不是文本辅助角色，也不进 `auxiliary.json`。它有独立的配置、端口和资产库；**模型由用户在设置中选择，Agent 不能选择或切换模型**；Agent 只在需要时主动调用 `image_generate` 工具。

### 6.1 独立配置 `<data-dir>/image.json`

| 字段 | 说明 |
|---|---|
| `enabled` | 是否启用生图 |
| `provider` | 适配器：`openai_images` / `gemini_images` / 插件提供的协议 |
| `baseUrl` | 用户填写；不复用聊天 provider 的端点 |
| `apiKey` | 凭据引用：`env:` / `keychain:` / `literal:` |
| `model` | **用户选择的图片模型 id**；运行时固定使用，Agent 不可改 |
| `defaults` | `size` / `quality` / `format` / `count` |
| `limits` | 每 Turn 最多张数、单文件上限、参考图上限 |
| `egressGrant` | 图片提示词外发授权 |

- 不复用 `providers.json` 的聊天供应商配置，允许图片和聊天使用不同厂商、不同 Key、不同端点。
- 设置页有独立的「生图」区域：选择模型、填写端点和 Key、测试连接、查看能力（4K、蒙版编辑、参考图、GIF / SVG、速度档位）。
- 未配置、未启用或缺少外发授权时：**不暴露** `image_generate` 工具，或调用时明确报错 `image generation is not configured`；不回退到聊天模型。
- Agent 的 `image_generate` 参数里**没有 `model`**；运行时注入配置中的模型。Agent 可以决定“何时生成、提示词是什么”，不能决定“用哪个模型”。

### 6.2 模型能力与目录

图片模型使用独立目录（建议 `src-tauri/assets/image-model-catalog.json`），记录能力而不是价格：

```json
{
  "id": "gpt-image-2.6",
  "name": "GPT-Image-2.6",
  "capabilities": ["text_to_image", "image_edit", "mask_edit", "reference_images", "gif", "svg"],
  "outputFormats": ["png", "jpeg", "webp", "gif", "svg"],
  "maxResolution": "4k",
  "maxReferenceImages": 14,
  "speedClass": "fast"
}
```

- 首批可记录：GPT-Image-2.5 Flare / Sunburst（快速批量 vs 高端精修）、GPT-Image-2.6（GIF / SVG、规划与渲染解耦）、Nano Banana 2.1（最高 4K、蒙版编辑、最多 14 张参考图）。
- 目录是 advisory；未知模型保留手动配置。具体参数以官方 API 为准，不在客户端硬编码未公开的分辨率。
- 用户选择模型时展示能力与限制；Agent 只看到当前模型允许的参数（如 `size` 白名单）。

### 6.3 端口

```rust
// phi-contracts
pub struct ImageRequest {
    pub model: String,          // injected from image.json, not model-authored
    pub prompt: String,
    pub size: Option<String>,   // whitelist per model
    pub count: u32,             // default 1, max 4
    pub reference_assets: Vec<String>,
}

pub struct ImageAsset {
    pub asset_id: String,
    pub mime: String,           // image/png | image/jpeg | image/webp | image/gif | image/svg+xml
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub struct ImageResponse {
    pub assets: Vec<ImageAsset>,
    pub usage: Option<ImageUsage>,
}
```

- 可选槽位 `image.generate`；首批适配器建议 OpenAI Images 与 Gemini Images，后续可由插件提供其他协议。
- 生成结果写入 `<data-dir>/assets/<assetId>`（内容寻址），记录来源模型、提示词摘要和创建时间；不热链远程 URL。
- 大图不进模型上下文：工具结果只返回 `assetId`、尺寸和附件引用；模型需要的是“图已生成”的事实，不是像素数据。

### 6.4 工具

- `image_generate { prompt, size?, count?, reference_assets? }`，只在生图配置完整且外发授权存在时暴露。
- 默认只给 Lead；是否给 Sidekick 由配置决定。该工具**不是后台任务**，不会自动生成。
- 参数限制：count 1–4，prompt ≤ 4,000 字符，size 受模型能力白名单约束；参考图只能是用户提供的本地资产，不允许任意 URL。
- 每个 Turn 默认最多 4 张（可配置）；超限明确报错，不静默截断。
- 策略：首次使用某个 provider 时明确告知接收方与用途（EgressGrant）；之后按授权执行，不再逐张审批。若策略要求 `Ask`，审批卡显示 provider、提示词预览和数量。
- 工具结果附带图片附件；UI 在消息流中渲染，支持下载和复制提示词。

### 6.5 安全与格式

- 光栅：PNG / JPEG / WebP；GIF 允许（限制帧数、尺寸和文件大小）。
- **SVG 允许生成和下载，但不在消息流内联渲染**：矢量文件可能包含脚本或外链，`RichHtml` 不接受 SVG 或 `data:` URL。有光栅预览就显示预览，否则提供下载。
- 校验 magic bytes、MIME、尺寸和文件大小（默认 20 MB）；剥离 EXIF / 元数据；不热链远程 URL。
- prompt 与生成结果都视为不可信；不自动把图片内容再喂给视觉模型，除非用户或 Lead 明确要求。
- 图片可能包含敏感信息：资产保留在本地，删除会话 / 资产时一并清理；外发授权按 provider 记录。

### 6.6 headless

```bash
phi-headless image configure --provider openai_images --base-url ... --model gpt-image-2.6
phi-headless image generate --prompt "..." --out ./out.png
phi-headless image list --session <id>
phi-headless image delete <asset_id>
```

测试用脚本化 image provider，验证模型固定、MIME 校验、尺寸上限、SVG 不内联、无授权时工具不存在。

---

## 7. 用量与 UI

- 每次文本辅助调用记录 `AuxiliaryCallReturned` 的 token；生图单独记录调用次数、模型和资产（`ImageGenerated`）。
- 设置页「背景用量」按角色 / 会话 / 天汇总；生图与决策模型各自有独立用量区。不显示价格（项目当前只记 token）。
- 会话页可以显示「标题 / 记忆整理由辅助模型完成」，并提供关闭入口。
- 辅助任务失败不阻塞主对话；只在对应的功能区域显示可展开的失败原因，不弹全局错误。

---

## 8. 测试

- 标题：脚本化 provider 返回合法 / 非法输出；只应用默认标题；用户改名后不再覆盖；不产生 Turn / Run。
- 记忆整理：dry-run 提案；安全操作自动应用、危险操作待确认；准入屏障拒绝跨 scope；`forget` 不自动；审计不含正文。
- 生图：模型固定为用户配置；无授权不暴露工具；MIME / 尺寸校验；SVG 只下载不内联；GIF 限制；资产落盘与清理；工具结果渲染。
- 角色绑定：provider / 模型删除后禁用；能力不匹配拒绝保存；headless `roles` 命令可复现。
- 事件：辅助调用不产生 `TurnStarted` / `RunScheduled` / `StepStarted`；用量可聚合。

---

## 9. 待定

| 问题 | 影响 | 倾向 |
|---|---|---|
| 标题是否自动应用 | UX | 默认标题时自动应用；用户改名后不覆盖 |
| 记忆整理的自动化边界 | 安全 | 重复合并 / 明确替换可自动；纠正 / 失效 / 忘记需确认 |
| 用户 scope 是否一律待确认 | 安全 | 是；可在设置中显式放宽 |
| 生图协议首批范围 | 兼容性 | 先 OpenAI Images / Gemini Images；其他由插件提供（ADR-0016） |
| SVG 是否允许内联 | 安全 | 只允许下载，不内联渲染 |
| 生图模型是否支持多选 | UX | 第一版只允许一个活动模型；用户可随时切换，Agent 不可切换 |
| 是否按会话覆盖角色绑定 | 灵活性 | 第一版不做；超级 Agent 阶段再考虑 |
| 辅助调用是否需要单独账本 | 成本可见性 | 事件已记录；UI 按角色汇总即可，暂不引入预算 |
