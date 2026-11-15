// Frontend internationalization.
//
// English message ids are the source; the Chinese catalog is the only place
// that contains translated text. The Rust node selects the language and
// exposes it in Bootstrap, so both shells agree on the setting.

export type Lang = "en" | "zh-CN";

import { createSignal } from "solid-js";

const [current, setCurrent] = createSignal<Lang>("en");

export function setLang(value: string | null | undefined) {
  setCurrent(value === "zh-CN" ? "zh-CN" : "en");
}

export function lang(): Lang {
  return current();
}

const ZH: Record<string, string> = {
  // shell
  "New session": "新会话",
  "Sessions": "会话",
  "Draft": "草稿",
  "Node {id}": "节点 {id}",
  "Mode {mode}": "模式 {mode}",
  "Settings": "设置",
  "Minimize window": "最小化窗口",
  "Maximize window": "最大化窗口",
  "Close window": "关闭窗口",
  "Collapse sidebar": "收起侧栏",
  "Expand sidebar": "展开侧栏",

  // time
  "just now": "刚刚",
  "{n} min ago": "{n} 分钟前",
  "{n} h ago": "{n} 小时前",
  "{n} d ago": "{n} 天前",
  "Expired": "已过期",

  // conversation
  "Loading…": "加载中…",
  "You": "你",
  "Agent": "Agent",
  "Start a task": "开始一个任务",
  "Describe what you want the agent to do. It can read your files, edit them and run commands — always with your approval.":
    "描述你希望 Agent 做的事。它可以读取并修改你的文件、执行命令——每一步都需要你的批准。",
  "“Map the repository and explain how the app is structured.”": "“理清仓库结构，并说明这个应用是怎么组织的。”",
  "“Find the failing test and fix it.”": "“找到失败的测试并修复它。”",
  "No provider is configured. Add one in Settings to start a task.":
    "当前没有配置供应商。请先在设置中添加一个。",
  "The node is not in normal mode; sending is disabled": "节点未处于正常模式，发送消息已被禁用",
  "Call {name}": "调用 {name}",
  "Tool result": "工具结果",
  "Tool error": "工具错误",
  "The agent is working…": "Agent 正在工作…",
  "Describe a task… (Enter to send, Shift+Enter for a newline)": "输入任务…（Enter 发送，Shift+Enter 换行）",
  "Message": "消息",

  // approvals
  "Approval required": "需要审批",
  "Low risk": "低风险",
  "Medium risk": "中风险",
  "High risk": "高风险",
  "Approve": "批准",
  "Approve and run": "批准并运行",
  "Reject": "拒绝",
  "Expires in {time}": "{time} 后过期",
  "Action digest {digest}": "动作摘要 {digest}",
  "This approval applies only to this command and these arguments.": "批准仅对这条命令和这组参数生效。",

  // run bar and composer
  "Steps": "步数",
  "Cost": "成本",
  "Time": "时间",
  "Cancel": "取消",
  "Budget: {steps} steps / ${cost} / {time}": "预算：{steps} 步 / ${cost} / {time}",
  "Send": "发送",
  "Workbench": "工作台",
  "Updated": "已更新",
  "v{version}": "v{version}",
  "The agent will lay out the task here when a visual overview helps.":
    "当可视化的总览有帮助时，Agent 会在这里布置任务。",
  "Unsupported view component: {kind}": "不支持的视图组件：{kind}",
  "Done": "已完成",
  "Current": "进行中",
  "To do": "待办",
  "Chart data": "图表数据",
  "Label": "标签",

  // settings: general
  "Workspace": "工作区",
  "The folder the agent may read and edit.": "Agent 可以读取和编辑的文件夹。",
  "Absolute workspace path": "工作区绝对路径",
  "Save": "保存",
  "Saved": "已保存",
  "General": "通用",
  "Back to chat": "返回对话",
  "Settings are stored on this node.": "设置保存在当前节点上。",
  "Workspace boundary, language and the node this window is connected to.":
    "工作区边界、语言，以及这个窗口连接的节点。",
  "Interface language": "界面语言",
  "Changes apply to this node and to every shell connected to it.":
    "更改会应用到当前节点，以及所有连接到它的外壳。",
  "English": "English",
  "Chinese (Simplified)": "简体中文",

  // settings: providers
  "Model providers": "模型供应商",
  "Configure endpoints, credentials and the models each provider offers. One provider is active at a time.":
    "配置端点、凭据，以及每个供应商提供的模型。同一时间只有一个供应商处于当前状态。",
  "Add provider": "添加供应商",
  "No providers yet.": "还没有供应商。",
  "No providers yet": "还没有供应商",
  "Add a provider to call a model. The agent cannot run without one.":
    "添加一个供应商以调用模型；没有供应商时 Agent 无法运行。",
  "Active": "当前",
  "{count} models": "{count} 个模型",
  "Key configured": "已配置密钥",
  "No key": "无密钥",
  "Disabled": "已禁用",
  "Enabled": "启用",
  "Start from a preset": "从预设开始",
  "Provider saved.": "供应商已保存。",
  "Provider deleted.": "供应商已删除。",
  "Provider is now active.": "供应商已设为当前。",
  "Fetched {count} models.": "已获取 {count} 个模型。",
  "The provider returned no models.": "供应商没有返回任何模型。",
  "Delete provider {name}? This cannot be undone.": "删除供应商 {name}？此操作无法撤销。",
  "New provider": "新建供应商",
  "Provider": "供应商",
  "What do you want to connect?": "要连接什么？",
  "Start from a preset, then adjust any field.": "从预设开始，然后按需调整。",
  "Anthropic Messages API.": "Anthropic Messages 接口。",
  "OpenAI Chat Completions API.": "OpenAI Chat Completions 接口。",
  "OpenAI Responses API.": "OpenAI Responses 接口。",
  "Any OpenAI-compatible endpoint, including local runtimes such as Ollama or LM Studio.":
    "任意 OpenAI 兼容端点，包括 Ollama、LM Studio 等本地运行时。",
  "An Anthropic Messages-compatible gateway or proxy.": "兼容 Anthropic Messages 的网关或代理。",
  "Connection": "连接",
  "Name": "名称",
  "My provider": "我的供应商",
  "Protocol": "协议",
  "Anthropic Messages": "Anthropic Messages",
  "OpenAI Chat Completions": "OpenAI Chat Completions",
  "OpenAI Responses": "OpenAI Responses",
  "Base URL": "基础 URL",
  "Authentication": "认证",
  "API key header (x-api-key)": "API Key 请求头（x-api-key）",
  "Bearer token": "Bearer 令牌",
  "No authentication": "无认证",
  "API key": "API 密钥",
  "Leave empty to keep the current key.": "留空则保留当前密钥。",
  "API key or env:NAME reference": "API 密钥或 env:NAME 引用",
  "Show key": "显示密钥",
  "Hide key": "隐藏密钥",
  "Current key: {hint}": "当前密钥：{hint}",
  "The key will be cleared when you save.": "保存后将清除密钥。",
  "Clear key": "清除密钥",
  "Website": "网站",
  "Notes": "备注",
  "Models": "模型",
  "Add model ids manually or fetch them from the provider. Metadata is auto-configured from the model name and can be edited.":
    "可以手动添加模型 ID，或从供应商获取。元数据会按模型名自动配置，也可以手动修改。",
  "Fetch from provider": "从供应商获取",
  "Add model": "添加模型",
  "Primary model": "主模型",
  "Model id": "模型 ID",
  "No models yet. Add one manually or fetch the provider's model list.":
    "还没有模型。可以手动添加，或获取供应商的模型列表。",
  "New model": "新模型",
  "Remove model": "移除模型",
  "Display name": "显示名称",
  "Description": "描述",
  "What is this model good at?": "这个模型擅长什么？",
  "Tags": "标签",
  "fast, reasoning, vision": "fast, reasoning, vision",
  "Context window": "上下文窗口",
  "Inherit": "继承",
  "Max output tokens": "最大输出 Token",
  "Input $/Mtok": "输入 $/百万 Token",
  "Output $/Mtok": "输出 $/百万 Token",
  "Temperature": "温度",
  "Auto-configured from the model name. Edit any field to override; explicit values are kept.":
    "已按模型名自动配置。修改任一字段即可覆盖；显式填写的值会保留。",
  "Auto": "自动",
  "Fetched": "已获取",
  "Manual": "手动",
  "Model roles": "模型角色",
  "Roles map a logical name (for example fast or reasoner) to a model id. Agents can request a role instead of a concrete model.":
    "角色把逻辑名称（如 fast、reasoner）映射到具体模型 ID。Agent 可以请求角色，而不是写死模型。",
  "Role name": "角色名",
  "Remove role": "移除角色",
  "Add role": "添加角色",
  "Custom headers": "自定义请求头",
  "Headers are sent with every request. Authorization and x-api-key are managed by the gateway and cannot be overridden here.":
    "这些请求头会随每次调用发送。Authorization 与 x-api-key 由网关管理，不能在这里覆盖。",
  "Header": "请求头",
  "Value": "值",
  "Remove header": "移除请求头",
  "Add header": "添加请求头",
  "Provider defaults": "供应商默认值",
  "Used when a model does not declare its own pricing, output limit or temperature.":
    "当模型没有单独声明价格、输出上限或温度时使用。",
  "Default": "默认",
  "Use protocol defaults": "使用协议默认值",
  "Connected in {ms} ms, {count} models available.": "连接成功，耗时 {ms} ms，可用模型 {count} 个。",
  "Connection failed: {error}": "连接失败：{error}",
  "Saving…": "保存中…",
  "Test connection": "测试连接",
  "Set as active": "设为当前",
  "Delete provider": "删除供应商",

  // run / node states
  "Scheduled": "已排程",
  "Running": "运行中",
  "Waiting for approval": "等待审批",
  "Succeeded": "已完成",
  "Failed": "失败",
  "Aborted": "已中止",
  "Needs review": "待核对",
  "Normal": "正常",
  "Degraded": "降级",
  "Maintenance": "维护",

  // settings: memory
  "Memory": "记忆",
  "Facts the agent remembers across runs, stored in the local data directory. Deleting a fact erases it and its stored source excerpt from memory; past conversations are not rewritten.":
    "Agent 跨运行记住的事实在本地数据目录中。删除一条事实会同时从记忆中删除它的原文；过去的对话记录不会被改写。",
  "{total} facts · {active} active · {episodes} source excerpts":
    "{total} 条事实 · {active} 条有效 · {episodes} 条原文",
  "Add a stable fact the agent should remember":
    "添加一条 Agent 应记住的稳定事实",
  "Project": "项目",
  "User": "用户",
  "Run": "运行",
  "Add": "添加",
  "Search memories": "搜索记忆",
  "Search": "搜索",
  "Clear": "清除",
  "Delete": "删除",
  "Confirm delete": "确认删除",
  "Click delete again to confirm permanent deletion.":
    "再次点击删除以确认永久删除。",
  "Deleted {records} fact(s) and {episodes} source excerpt(s).":
    "已删除 {records} 条事实和 {episodes} 条原文。",
  "Memory saved.": "记忆已保存。",
  "This fact is already stored.": "这条事实已经存在。",
  "No memories match this search.": "没有匹配的记忆。",
  "No memories yet. The agent can save facts with its memory tool.":
    "还没有记忆。Agent 可以用记忆工具保存事实。",
  "Redacted": "已脱敏",
  "Confidence": "置信度",
  "From user": "来自用户",
  "From system": "来自系统",
  "From tool": "来自工具",
  "From model": "来自模型",
  "From external": "来自外部",
  "State active": "有效",
  "State invalidated": "已失效",
  "State superseded": "已取代",
};

export function t(key: string): string {
  if (current() === "en") return key;
  return ZH[key] ?? key;
}

export function tArgs(
  key: string,
  args: Record<string, string | number>,
): string {
  let text = t(key);
  for (const [name, value] of Object.entries(args)) {
    text = text.split(`{${name}}`).join(String(value));
  }
  return text;
}
