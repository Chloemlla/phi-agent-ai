//! Runtime internationalization.
//!
//! Code and identifiers are English. Every string that a person can see at
//! runtime goes through this module: tool output, approval notes, stop
//! reasons, errors, logs and CLI text. English is the canonical message id;
//! the Chinese catalog is the only place that contains translated text.
//!
//! The language is process-wide because plugins are shared by every shell
//! (desktop and headless). A shell may override it per run, and the selected
//! language is persisted in `config.json` by `phi_node::compose`.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    ZhCn,
}

impl Lang {
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "en" | "en-us" | "en-gb" | "english" => Some(Lang::En),
            "zh" | "zh-cn" | "zh-hans" | "zh-sg" | "cn" | "chinese" => Some(Lang::ZhCn),
            _ => None,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhCn => "zh-CN",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(
        match lang {
            Lang::En => 0,
            Lang::ZhCn => 1,
        },
        Ordering::Relaxed,
    );
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::ZhCn,
        _ => Lang::En,
    }
}

/// Resolve the language for a process.
///
/// Precedence: explicit override (CLI) > `PHI_LANG` > persisted config >
/// locale environment variables > English.
pub fn resolve(override_lang: Option<&str>, configured: Option<&str>) -> Lang {
    override_lang
        .and_then(Lang::parse)
        .or_else(|| env_lang("PHI_LANG"))
        .or_else(|| configured.and_then(Lang::parse))
        .or_else(|| env_lang("LC_ALL"))
        .or_else(|| env_lang("LC_MESSAGES"))
        .or_else(|| env_lang("LANG"))
        .unwrap_or(Lang::En)
}

fn env_lang(name: &str) -> Option<Lang> {
    std::env::var(name).ok().and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        Lang::parse(value)
    })
}

/// Translate a message id. English ids fall through unchanged.
pub fn t(msgid: &str) -> String {
    match current() {
        Lang::En => msgid.to_string(),
        Lang::ZhCn => zh(msgid).unwrap_or(msgid).to_string(),
    }
}

/// Translate and substitute named placeholders written as `{name}`.
pub fn t_args(msgid: &str, args: &[(&str, String)]) -> String {
    let mut text = t(msgid);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

fn zh(msgid: &str) -> Option<&'static str> {
    Some(match msgid {
        // Generic
        "… (truncated)" => "…（已截断）",
        "New session" => "新会话",
        "Unknown language {lang}; expected en or zh-CN" => "未知语言 {lang}；可选 en 或 zh-CN",
        "Unable to locate the application data directory: {error}" => {
            "无法定位应用数据目录：{error}"
        }

        // Tools
        "Command is empty" => "命令为空",
        "Command execution failed: {error}" => "命令执行失败：{error}",
        "Unknown tool: {tool}" => "未知工具：{tool}",
        "(command timed out after {secs}s; terminated)\n" => "（命令超时，已终止，{secs}s）\n",
        "Not a directory: {path}" => "不是目录：{path}",
        "Failed to read directory: {error}" => "读取目录失败：{error}",
        "Directory: {path}\n" => "目录：{path}\n",
        "… ({total} entries total; showing first 300)\n" => {
            "…（共 {total} 项，已显示前 300 项）\n"
        }
        "(empty directory)\n" => "（空目录）\n",
        "Missing path argument" => "缺少 path 参数",
        "Failed to read file: {error}" => "读取文件失败：{error}",
        "Not a file: {path}" => "不是文件：{path}",
        "File is too large (>1.5 MB); use shell_exec to read it in chunks" => {
            "文件过大（>1.5MB），请改用 shell_exec 分段读取"
        }
        "Binary file detected; refusing to read it as text" => "检测到二进制文件，拒绝按文本读取",
        "File: {path} ({total} lines)\n" => "文件：{path}（共 {total} 行）\n",
        "… (showing lines {start}-{end} of {total}; use offset/limit to continue)\n" => {
            "…（已显示 {start}-{end} 行，共 {total} 行；用 offset/limit 继续读取）\n"
        }
        "Missing content argument" => "缺少 content 参数",
        "Content is too large (>2 MB)" => "内容过大（>2MB）",
        "Failed to create directory: {error}" => "创建目录失败：{error}",
        "Parent directory does not exist; pass create_dirs=true to create it" => {
            "父目录不存在；如需创建请传 create_dirs=true"
        }
        "Failed to read previous content: {error}" => "读取旧内容失败：{error}",
        "Backup failed: {error}" => "备份失败：{error}",
        "Previous content backed up to {path}" => "旧内容已备份到 {path}",
        "Write failed: {error}" => "写入失败：{error}",
        "Wrote {path} ({bytes} bytes). {note}" => "已写入 {path}（{bytes} 字节）。{note}",

        // Session
        "The process restarted while an approval was pending; the action was not executed. Review it and send the request again." => {
            "进程重启前正在等待审批，动作尚未执行。请核对后重新发起。"
        }
        "The process restarted before the run finished; side effects are unknown. Inspect the workspace before retrying." => {
            "进程重启前运行未正常结束，副作用结果未知。请检查工作区后决定是否重试。"
        }
        "(content could not be parsed)" => "（内容无法解析）",
        "(content is not readable)" => "（内容不可读）",

        // Provider config
        "Unknown protocol {protocol}; expected anthropic_messages, openai_chat or openai_responses" => {
            "未知协议 {protocol}；可选：anthropic_messages、openai_chat、openai_responses"
        }
        "Unknown auth mode {auth}; expected x_api_key, bearer or none" => {
            "未知认证方式 {auth}；可选：x_api_key、bearer、none"
        }
        "Environment variable {name} is not set" => "环境变量 {name} 未设置",
        "Environment variable {name} is empty" => "环境变量 {name} 为空",
        "OS Keychain is not enabled yet ({name}); use an env: reference or literal: for development" => {
            "OS Keychain 尚未启用（{name}）；请先使用 env: 引用或 literal:（仅开发）"
        }
        "API key is empty" => "API Key 为空",

        // Sandbox
        "Workspace is not available: {error}" => "工作区不可用：{error}",
        "Access outside the workspace is denied" => "拒绝访问工作区之外的路径",
        "Invalid path" => "路径非法",
        "Path is not available: {error}" => "路径不可用：{error}",
        "Failed to resolve path" => "路径解析失败",
        "Failed to start command: {error}" => "启动命令失败：{error}",
        "Failed to wait for command: {error}" => "等待命令失败：{error}",

        // Approvals
        "Approval request does not exist or has expired" => "审批请求不存在或已过期",
        "Approval request has no waiting channel" => "审批请求没有等待通道",
        "Approver exited" => "审批者已退出",
        "Approval timed out; treated as rejected" => "审批超时，按拒绝处理",
        "Run was cancelled; approval is void" => "运行已取消，审批失效",

        // Policy
        "Read-only access inside the workspace" => "工作区内只读访问",
        "Writing to the workspace modifies data and requires human approval (previous content is backed up)" => {
            "写入工作区会修改数据，需要人工审批（旧内容会备份）"
        }
        "Empty command" => "空命令",
        "Command matches a dangerous pattern; denied by default" => "命令命中危险模式，默认拒绝",
        "Command execution is irreversible and requires human approval" => {
            "命令执行不可逆，需要人工审批"
        }
        "No policy matches {tool}; denied by default" => "没有匹配 {tool} 的策略，默认拒绝",
        "Reading long-term memory does not touch the workspace" => "读取长期记忆不会触碰工作区",
        "Memory writes pass the admission barrier and can be forgotten later" => {
            "记忆写入会经过准入，之后可以遗忘"
        }

        // Memory
        "The candidate fact is empty or has no meaningful content" => "候选事实为空或没有实质内容",
        "The candidate fact is too long ({max} characters max)" => {
            "候选事实过长（最多 {max} 个字符）"
        }
        "This memory scope needs a project, agent or run context" => {
            "该记忆作用域需要项目、Agent 或运行上下文"
        }
        "The validity window is invalid" => "有效期区间无效",
        "The confidence must be a finite number between 0 and 1" => {
            "置信度必须是 0 到 1 之间的有限数字"
        }
        "The candidate looks like a secret; sensitive values are not stored as memories" => {
            "候选内容疑似密钥；敏感值不会写入记忆"
        }
        "An identical fact already exists" => "已存在相同的事实",
        "Unknown memory scope: {scope}" => "未知记忆作用域：{scope}",
        "Run-scoped memories are written by the agent inside a run" => {
            "运行作用域的记忆由 Agent 在运行中写入"
        }
        "Memory record not found" => "记忆记录不存在",
        "Memory record is already inactive" => "记忆记录已失效",
        "User requested deletion" => "用户请求删除",
        "Deleted from settings" => "从设置中删除",
        "Select which memories to forget" => "请选择要遗忘哪些记忆",
        "No memory ids were provided" => "没有提供记忆 id",
        "Missing query argument" => "缺少 query 参数",
        "No matching memories." => "没有匹配的记忆。",
        "Memory search failed: {error}" => "记忆检索失败：{error}",
        "Missing scope argument" => "缺少 scope 参数",
        "Memory write failed: {error}" => "记忆写入失败：{error}",
        "Memory is not available" => "记忆功能不可用",
        "Unknown memory subcommand: {command}" => "未知 memory 子命令：{command}",
        "Unknown memory scope {scope}; expected user, project, agent or run" => {
            "未知记忆作用域 {scope}；应为 user、project、agent 或 run"
        }
        "Unknown memory source {source}; expected user, system, tool, model or external" => {
            "未知记忆来源 {source}；应为 user、system、tool、model 或 external"
        }
        "Unknown memory state {state}; expected active, invalidated or superseded" => {
            "未知记忆状态 {state}；应为 active、invalidated 或 superseded"
        }
        "Unknown episode source {source}; expected user_input, tool_result, model_output or external" => {
            "未知原文来源 {source}；应为 user_input、tool_result、model_output 或 external"
        }
        "--limit is not a number: {value}" => "--limit 不是数字：{value}",
        "--offset is not a number: {value}" => "--offset 不是数字：{value}",
        "{flag} is not a number: {value}" => "{flag} 不是数字：{value}",
        "{flag} must be a finite number: {value}" => "{flag} 必须是有限数字：{value}",
        "Conflicting memory ids" => "记忆 id 冲突",
        "memory search requires a query" => "memory search 需要一个查询词",
        "memory add requires --fact TEXT" => "memory add 需要 --fact TEXT",
        "This memory command requires --reason TEXT" => "该 memory 命令需要 --reason TEXT",
        "Unknown memory list argument: {arg}" => "未知 memory list 参数：{arg}",
        "Unknown memory search argument: {arg}" => "未知 memory search 参数：{arg}",
        "Unknown memory add argument: {arg}" => "未知 memory add 参数：{arg}",
        "Unknown memory invalidate argument: {arg}" => "未知 memory invalidate 参数：{arg}",
        "Unknown memory correct argument: {arg}" => "未知 memory correct 参数：{arg}",
        "memory correct requires --fact TEXT" => "memory correct 需要 --fact TEXT",
        "Unknown memory supersede argument: {arg}" => "未知 memory supersede 参数：{arg}",
        "memory supersede requires --fact TEXT" => "memory supersede 需要 --fact TEXT",
        "memory supersede requires --scope" => "memory supersede 需要 --scope",
        "Unknown memory forget argument: {arg}" => "未知 memory forget 参数：{arg}",
        "Do not combine --all with other forget selectors" => {
            "不要将 --all 与其他遗忘选择器组合使用"
        }
        "memory forget requires --id, --scope, --project, --run or --all" => {
            "memory forget 需要 --id、--scope、--project、--run 或 --all"
        }

        // Provider store
        "Unknown provider: {id}" => "未知 provider：{id}",
        "maxOutputTokens must be greater than 0" => "maxOutputTokens 必须大于 0",
        "temperature must be between 0 and 2" => "temperature 必须在 0 到 2 之间",
        "Provider name cannot be empty" => "provider 名称不能为空",
        "baseUrl cannot be empty" => "baseUrl 不能为空",
        "baseUrl must start with http:// or https://" => "baseUrl 必须以 http:// 或 https:// 开头",
        "model cannot be empty" => "model 不能为空",
        "This auth mode requires an API key (an env:NAME reference works)" => {
            "该认证方式需要 API Key（可用 env:NAME 引用）"
        }
        "No provider is available" => "没有可用的 provider",
        "Provider {name} is disabled" => "provider {name} 已禁用",
        "Provider {name} is missing baseUrl" => "provider {name} 缺少 baseUrl",
        "Provider {name} is missing model" => "provider {name} 缺少 model",
        "Provider {name} has no primary model yet" => "provider {name} 还没有主模型",
        "Model {model} is disabled in this provider" => "模型 {model} 在此 provider 中已禁用",
        "Provider {name} has no API key" => "provider {name} 未配置 API Key",
        "Provider {name}: {error}" => "provider {name}: {error}",
        "Failed to create config directory: {error}" => "创建配置目录失败：{error}",
        "Failed to write provider config: {error}" => "写入 provider 配置失败：{error}",
        "Failed to replace provider config: {error}" => "替换 provider 配置失败：{error}",
        "Custom header name cannot be empty" => "自定义 header 名称不能为空",
        "Header {key} is managed by the gateway and cannot be overridden in custom headers" => {
            "header {key} 由网关管理，不能在自定义 headers 中覆盖"
        }
        "Header {key} contains an invalid newline" => "header {key} 含有非法换行",

        // Runtime loop
        "Session store is not available (session.store slot is missing)" => {
            "会话存储不可用（session.store 缺失）"
        }
        "Session store is not available" => "会话存储不可用",
        "This session still has a running task: {run_id}" => "该会话仍有运行中的任务：{run_id}",
        "Workspace does not exist or is not a directory" => "工作区不存在或不是目录",
        "Stopped: the in-flight request was aborted." => "已停止：进行中的请求已中止。",
        "Stopped: model request was aborted; no tool side effects occurred." => {
            "已停止：模型请求已中止，未产生工具副作用。"
        }
        "Stopped: cancelled during retry backoff." => "已停止：重试等待期间取消。",
        "Model call returned no response" => "模型调用没有得到响应",
        "Failed to save assistant message: {error}" => "保存助手消息失败：{error}",
        "Failed to save tool results: {error}" => "保存工具结果失败：{error}",
        "Run cancelled; action was not executed." => "运行已取消；动作未执行。",
        "Stopped: the command was terminated and may have produced partial side effects. Inspect and verify the workspace (no automatic retry)." => {
            "已停止：命令被终止，但可能已产生部分副作用。请检查工作区并核对（不会自动重试）。"
        }
        "Stopped: the in-flight tool call was aborted." => "已停止：进行中的工具调用已中止。",
        "(stdout truncated at 1 MB; the rest was discarded)" => {
            "（stdout 超过 1 MB，已截断，其余内容已丢弃）"
        }
        "(stderr truncated at 1 MB; the rest was discarded)" => {
            "（stderr 超过 1 MB，已截断，其余内容已丢弃）"
        }
        "The previous response was cut off by the output token limit. Continue exactly where you stopped, without repeating what you already wrote." => {
            "上一条响应因输出 token 上限被截断。请从停下的地方继续，不要重复已经写过的内容。"
        }
        "The model response was truncated by the output token limit ({max_output_tokens} tokens) three times; the run stopped to avoid an endless loop." => {
            "模型响应连续三次因输出 token 上限（{max_output_tokens} tokens）被截断；为避免无限循环，运行已停止。"
        }
        "The model repeatedly sent tool arguments that were not valid JSON; the run stopped without executing them." => {
            "模型反复发送不是合法 JSON 的工具参数；运行已停止，这些工具均未执行。"
        }
        "Response was truncated before the tool call completed; the tool was not executed." => {
            "响应在工具调用完成前被截断；该工具未执行。"
        }
        "Policy component is unavailable; denied by default" => "策略组件不可用，按默认拒绝处理",
        "modify is not supported in this version; denied" => "当前版本不支持 modify，按拒绝处理",
        "Approval component is unavailable; denied by default" => "审批组件不可用，按默认拒绝处理",
        "Irreversible: code execution and permissions" => "不可逆：代码执行与权限",
        "Reversible: data modification" => "可逆：数据修改",
        "Failed to create approval: {error}" => "创建审批失败：{error}",
        "Approval outcome: {status}" => "审批结果：{status}",
        "The action changed after approval; the approval is no longer valid" => {
            "动作在审批后发生变化，审批失效"
        }
        "Human approval granted" => "人工审批通过",
        "Sandbox component is unavailable; execution denied (never run without isolation)" => {
            "沙箱组件不可用，拒绝执行（不会无隔离运行）"
        }
        "No tool implementation: {tool_id}" => "没有工具实现：{tool_id}",
        "The command was terminated and we cannot confirm whether side effects occurred; the run is awaiting reconciliation and will not retry automatically." => {
            "命令被终止，无法确认是否已产生副作用；运行进入待核对，不会自动重试。"
        }
        "Action denied: {rationale}" => "动作被拒绝：{rationale}",

        // Node
        "Failed to create data directory: {error}" => "创建数据目录失败：{error}",
        "Failed to import provider environment variables: {error}" => {
            "provider 环境变量导入失败：{error}"
        }
        "All required slots are available" => "所有必需槽位齐备",
        "Run mode {mode}: {reason}" => "运行模式 {mode}：{reason}",
        "Failed to create workspace: {error}" => "创建工作区失败：{error}",
        "Workspace must be a directory" => "工作区必须是目录",
        "Unknown session: {id}" => "未知会话：{id}",
        "Node is in {mode} mode: {reason}" => "节点处于 {mode} 模式：{reason}",
        "Message is empty" => "消息为空",
        "Unknown thinking effort {effort}; expected off, low, medium, high, xhigh or max" => {
            "未知思考强度 {effort}；可选 off、low、medium、high、xhigh 或 max"
        }
        "Set a workspace first" => "请先设置工作区",
        "Workspace does not exist; set it again" => "工作区不存在，请重新设置",
        "Approval does not exist or was already handled" => "审批不存在或已处理",
        "Cancelled by user" => "用户取消",
        "Model gateway is not available" => "模型网关不可用",

        // Model gateway / protocol
        "network: request failed: {error}" => "network: 请求失败：{error}",
        "network: failed to read response: {error}" => "network: 读取响应失败：{error}",
        "invalid_response: response is not valid JSON: {error}" => {
            "invalid_response: 响应不是合法 JSON：{error}"
        }
        "network: failed to read stream: {error}" => "network: 读取流失败：{error}",
        "network: failed to list models: {error}" => "network: 请求模型列表失败：{error}",
        "invalid_response: model list is not valid JSON: {error}" => {
            "invalid_response: 模型列表不是合法 JSON：{error}"
        }
        "No provider is configured; add a provider or set the environment variables" => {
            "未配置模型提供方；请添加 provider 或设置环境变量"
        }
        "Provider has no model configured" => "provider 未配置模型",
        "Provider has no API key configured" => "provider 未配置 API Key",
        "(model returned no content)" => "（模型没有返回内容）",
        "Model response is missing choices" => "模型响应缺少 choices",
        "Model response failed: {error}" => "模型响应失败：{error}",
        "network: stream ended before the model finished" => "网络：模型尚未结束，流就中断了",
        "Tool arguments are not valid JSON" => "工具参数不是合法 JSON",
        "Tool {tool} was not executed: the model sent arguments that are not valid JSON. Parser error: {error}. Raw arguments (truncated): {raw}. Re-issue the tool call with valid JSON arguments." => {
            "工具 {tool} 未执行：模型发送的参数不是合法 JSON。解析错误：{error}。原始参数（已截断）：{raw}。请重新发起工具调用并提供合法的 JSON 参数。"
        }
        "SSE data is not valid JSON: {error}" => "SSE 数据不是合法 JSON：{error}",
        "SSE error: {message}" => "SSE 错误：{message}",
        "SSE error: HTTP {status}" => "SSE 错误：HTTP {status}",

        // Model gateway
        "No provider is configured; add one in Settings" => "还没有配置模型供应商；请在设置中添加",

        // Headless CLI
        "Message is empty (use --message or pipe text through stdin)" => {
            "消息为空（用 --message 或从 stdin 输入）"
        }
        "Timed out waiting for the run to finish ({secs}s)" => "等待运行结束超时（{secs}s）",
        "Approval failed: {error}" => "审批失败：{error}",
        "Failed to read stdin: {error}" => "读取 stdin 失败：{error}",
        "--data-dir requires a value" => "--data-dir 缺少值",
        "--workspace requires a value" => "--workspace 缺少值",
        "--lang requires a value" => "--lang 缺少值",
        "Unknown sessions subcommand: {command}" => "未知 sessions 子命令：{command}",
        "Unknown command: {command}" => "未知命令：{command}",
        "Unknown approval mode {mode}; expected deny, allow or prompt" => {
            "未知审批模式 {mode}；可选 deny、allow、prompt"
        }
        "--max-steps is not a number: {value}" => "--max-steps 不是数字：{value}",
        "--max-cost is not a number: {value}" => "--max-cost 不是数字：{value}",
        "--max-wall is not a number: {value}" => "--max-wall 不是数字：{value}",
        "Unknown run argument: {arg}" => "未知 run 参数：{arg}",
        "Unknown approve argument: {arg}" => "未知 approve 参数：{arg}",
        "approve requires --allow or --deny" => "approve 需要 --allow 或 --deny",
        "Missing id" => "缺少 id",
        "Unknown providers subcommand: {command}" => "未知 providers 子命令：{command}",
        "--model-map expects key=model: {pair}" => "--model-map 需要 key=model 形式：{pair}",
        "--max-output-tokens is not a number: {value}" => "--max-output-tokens 不是数字：{value}",
        "--temperature is not a number: {value}" => "--temperature 不是数字：{value}",
        "--sort-index is not a number: {value}" => "--sort-index 不是数字：{value}",
        "Unknown provider argument: {arg}" => "未知 provider 参数：{arg}",
        "update requires a provider id" => "update 需要 provider id",
        "Internal error: index out of bounds ({flag})" => "内部错误：索引越界（{flag}）",
        "{flag} requires a value" => "{flag} 缺少值",

        // Read-only shells and persistence
        "The node is read-only" => "该节点为只读模式",
        "Read-only shell" => "只读外壳",
        "Another phi-agent process is using this data directory" => {
            "另一个 phi-agent 进程正在使用该数据目录"
        }
        "Failed to open data directory lock: {error}" => "打开数据目录锁失败：{error}",
        "Failed to lock data directory: {error}" => "锁定数据目录失败：{error}",
        "Failed to serialize settings: {error}" => "序列化设置失败：{error}",
        "Failed to save settings: {error}" => "保存设置失败：{error}",
        "Required services are missing: {slots}" => "缺少必需的服务：{slots}",
        "Runtime is not available" => "运行时不可用",
        "Approval component is unavailable" => "审批组件不可用",
        "Journal unavailable" => "事件日志不可用",
        "Session projection did not materialize" => "会话投影未生成",
        "Run interrupted before the tool result was recorded; the action may have executed. The run needs manual reconciliation and will not be retried automatically." => {
            "运行在记录工具结果之前中断；动作可能已经执行。该运行需要人工核对，不会自动重试。"
        }

        // Journal failures during a run
        "Failed to record the step start: {error}" => "记录步骤开始失败：{error}",
        "Failed to record the model call: {error}" => "记录模型调用失败：{error}",
        "Failed to record the model response: {error}" => "记录模型响应失败：{error}",
        "Failed to record the model failure: {error}" => "记录模型失败失败：{error}",
        "Failed to record the step completion: {error}" => "记录步骤完成失败：{error}",
        "Failed to record the tool decision: {error}" => "记录工具判定失败：{error}",
        "Action not executed: journal write failed ({error})." => {
            "动作未执行：事件日志写入失败（{error}）。"
        }
        "Action executed, but its audit record could not be written: {error}" => {
            "动作已执行，但审计记录写入失败：{error}"
        }

        // Subagent delegation
        "Only the lead agent can delegate to a subagent" => "只有主代理可以派发子代理",
        "Only the lead agent can wait for subagents" => "只有主代理可以等待子代理",
        "Only the lead agent can cancel a subagent" => "只有主代理可以取消子代理",
        "Only subagents can be cancelled" => "只有子代理可以被取消",
        "Missing required argument: {key}" => "缺少必填参数：{key}",
        "Missing required argument: brief.objective" => "缺少必填参数：brief.objective",
        "Unknown work mode {mode}; expected shared or read_only" => {
            "未知工作模式 {mode}；应为 shared 或 read_only"
        }
        "Too many active subagents ({count}); wait for one to finish or cancel it first" => {
            "活跃子代理过多（{count}）；请等待一个完成或先取消它"
        }
        "Unknown subagent model: {model}. Available models: {available}" => {
            "未知子代理模型：{model}。可用模型：{available}"
        }
        "Unknown subagent model: {model}" => "未知子代理模型：{model}",
        "Subagent model {model} (scale {scale}) is more capable than the lead model (scale {lead}); choose a smaller or equal model" => {
            "子代理模型 {model}（规模 {scale}）强于主代理模型（规模 {lead}）；请选择更小或相等的模型"
        }
        "Subagent model {model} is more capable than the lead model" => {
            "子代理模型 {model} 强于主代理模型"
        }
        "Another agent holds the workspace write lease ({agent}); wait for it to finish or use read_only" => {
            "另一个代理持有工作区写租约（{agent}）；请等待它完成或改用 read_only"
        }
        "Another agent holds the workspace write lease" => "另一个代理持有工作区写租约",
        "Another agent holds the workspace write lease ({agent}); the action was denied." => {
            "另一个代理持有工作区写租约（{agent}）；动作已被拒绝。"
        }
        "Failed to record the subagent: {error}" => "记录子代理失败：{error}",
        "Failed to save the subagent brief: {error}" => "保存子代理简报失败：{error}",
        "Failed to start the subagent: {error}" => "启动子代理失败：{error}",
        "The agent_ids argument must contain at least one agent id" => {
            "agent_ids 参数至少需要包含一个代理 id"
        }
        "Unknown message mode {mode}; expected steer or queue" => {
            "未知消息模式 {mode}；应为 steer 或 queue"
        }
        "The sending agent is not known" => "发送消息的代理不存在",
        "Unknown agent: {agent}" => "未知代理：{agent}",
        "An agent cannot send a message to itself" => "代理不能给自己发消息",
        "The process restarted while this agent was active; it was not resumed." => {
            "进程重启时该代理仍在活动；它没有被自动恢复。"
        }
        "The recipient is not part of this delegation family" => "接收者不属于同一委派家族",
        "Agent {agent} is not part of this delegation family" => "代理 {agent} 不属于同一委派家族",
        "Failed to deliver the message: {error}" => "投递消息失败：{error}",
        "The lead is not running; start a new turn to talk to it" => {
            "主代理当前没有运行；请开始新的轮次再与它对话"
        }
        "The turn is already closed; start a new turn to wake this subagent" => {
            "当前轮次已关闭；请开始新的轮次唤醒该子代理"
        }
        "Failed to wake the subagent: {error}" => "唤醒子代理失败：{error}",
        "The subagent is not part of this delegation family" => "该子代理不属于同一委派家族",
        "Unknown agent tool: {tool}" => "未知代理工具：{tool}",
        "Parent run not found: {run_id}" => "父运行不存在：{run_id}",
        "The parent run belongs to another session" => "父运行属于另一个会话",
        "The parent run is no longer active" => "父运行已不再活跃",
        "A subagent cannot delegate to another subagent" => "子代理不能再派发子代理",
        "A subagent run requires an agent id" => "子代理运行需要代理 id",
        "A subagent run requires a thread id" => "子代理运行需要线程 id",
        "Unknown subagent: {agent}" => "未知子代理：{agent}",
        "Subagent {agent} is already running" => "子代理 {agent} 已在运行",
        "The lead tried to finish while subagents were still active; the run stopped to avoid orphaned work." => {
            "主代理在子代理仍活跃时尝试结束；运行已停止，避免留下无人管理的执行。"
        }
        "You still have active subagents: {agents}. Call wait_for_agents to collect their reports, or cancel_agent to stop them, before finishing." => {
            "你仍有活跃的子代理：{agents}。结束前请调用 wait_for_agents 收集报告，或调用 cancel_agent 停止它们。"
        }
        "The subagent failed: {error}" => "子代理失败：{error}",
        "The subagent was stopped: {reason}. {stop}" => "子代理已停止：{reason}。{stop}",
        "Report from subagent {agent} ({status}):\n{report}" => {
            "来自子代理 {agent} 的报告（{status}）：\n{report}"
        }
        "(the subagent produced no text report)" => "（子代理没有产生文本报告）",
        "Agent coordination does not touch the workspace; each subagent action is still policy-checked" => {
            "代理协调不触碰工作区；每个子代理动作仍会经过策略检查"
        }
        "Unknown agents subcommand: {command}" => "未知 agents 子命令：{command}",
        "agents requires a subcommand: list <session-id> or show <agent-id>" => {
            "agents 需要子命令：list <session-id> 或 show <agent-id>"
        }

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language_aliases() {
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("zh_CN"), Some(Lang::ZhCn));
        assert_eq!(Lang::parse("zh-Hans"), Some(Lang::ZhCn));
        assert_eq!(Lang::parse("fr"), None);
    }

    #[test]
    fn core_messages_have_translations() {
        for msgid in [
            "New session",
            "Command is empty",
            "Access outside the workspace is denied",
            "Message is empty",
            "… (truncated)",
            "The node is read-only",
            "Another phi-agent process is using this data directory",
            "Failed to record the step start: {error}",
            "Memory record not found",
            "The candidate looks like a secret; sensitive values are not stored as memories",
            "memory forget requires --id, --scope, --project, --run or --all",
            "Unknown thinking effort {effort}; expected off, low, medium, high, xhigh or max",
            "Tool arguments are not valid JSON",
            "Tool {tool} was not executed: the model sent arguments that are not valid JSON. Parser error: {error}. Raw arguments (truncated): {raw}. Re-issue the tool call with valid JSON arguments.",
            "The previous response was cut off by the output token limit. Continue exactly where you stopped, without repeating what you already wrote.",
            "The model response was truncated by the output token limit ({max_output_tokens} tokens) three times; the run stopped to avoid an endless loop.",
            "The model repeatedly sent tool arguments that were not valid JSON; the run stopped without executing them.",
        ] {
            assert!(zh(msgid).is_some(), "missing translation: {msgid}");
        }
    }
}
