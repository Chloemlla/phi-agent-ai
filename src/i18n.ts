// Frontend internationalization.
//
// English message ids are the source; the Chinese catalog is the only place
// that contains translated text. The Rust node selects the language and
// exposes it in Bootstrap, so both shells agree on the setting.

export type Lang = "en" | "zh-CN";

import { createSignal } from "solid-js";

const [current, setCurrent] = createSignal<Lang>("en");

export function setLang(value: string | null | undefined) {
  const next = value === "zh-CN" ? "zh-CN" : "en";
  setCurrent(next);
  if (typeof document !== "undefined") {
    document.documentElement.lang = next;
  }
}

export function lang(): Lang {
  return current();
}

const ZH: Record<string, string> = {
  // shell
  "New session": "新会话",
  Sessions: "会话",
  Draft: "草稿",
  "Node {id}": "节点 {id}",
  "Mode {mode}": "模式 {mode}",
  Settings: "设置",
  "Minimize window": "最小化窗口",
  "Maximize window": "最大化窗口",
  "Close window": "关闭窗口",
  "Collapse sidebar": "收起侧栏",
  "Expand sidebar": "展开侧栏",
  "No sessions": "还没有会话",
  "Create a session to start a task.": "新建一个会话开始任务。",
  "Delete session": "删除会话",
  "Close sidebar": "关闭侧栏",
  Session: "会话",
  "Session actions": "会话操作",
  "Session actions for {title}": "会话操作：{title}",
  Rename: "重命名",
  "Rename session": "重命名会话",
  "Session deleted.": "会话已删除。",
  Notifications: "通知",
  Note: "提示",
  Success: "成功",
  Warning: "警告",
  Preferences: "偏好",
  "Back to settings": "返回设置",
  "Thinking effort": "思考等级",
  "Workspace boundary, language and interface preferences.": "工作区边界、语言与界面偏好。",
  "Endpoints, credentials and the models each provider offers.":
    "配置端点、凭据以及每个供应商提供的模型。",
  "Inspect, search and prune what the agent remembers.": "查看、搜索和清理 Agent 记住的内容。",
  "Delete session {title}": "删除会话 {title}",
  "Click again to delete": "再次点击删除",
  "Cancel the active run before deleting this session.": "请先取消正在运行的任务，再删除这个会话。",
  "Creating session…": "正在创建会话…",
  Stop: "停止",

  // time
  "just now": "刚刚",
  "{n} min ago": "{n} 分钟前",
  "{n} h ago": "{n} 小时前",
  "{n} d ago": "{n} 天前",
  "Messages: {n}": "消息：{n}",
  Expired: "已过期",

  // conversation
  "Loading…": "加载中…",
  You: "你",
  Agent: "Agent",
  "Start a task": "开始一个任务",
  "Describe what you want the agent to do. It can read your files, edit them and run commands — always with your approval.":
    "描述你希望 Agent 做的事。它可以读取并修改你的文件、执行命令——每一步都需要你的批准。",
  "“Map the repository and explain how the app is structured.”":
    "“理清仓库结构，并说明这个应用是怎么组织的。”",
  "“Find the failing test and fix it.”": "“找到失败的测试并修复它。”",
  "No provider is configured. Add one in Settings to start a task.":
    "当前没有配置供应商。请先在设置中添加一个。",
  "The node is not in normal mode; sending is disabled": "节点未处于正常模式，发送消息已被禁用",
  "Call {name}": "调用 {name}",
  "Tool result": "工具结果",
  "The agent is working…": "Agent 正在工作…",
  "Describe a task…": "输入任务…",
  "Describe a task… (Enter to send, Shift+Enter for a newline)":
    "输入任务…（Enter 发送，Shift+Enter 换行）",
  Message: "消息",
  "Attach images": "添加图片",
  "Attached image": "附带的图片",
  "Attached images": "已添加的图片",
  "Remove image": "移除图片",
  "Uploading image…": "正在上传图片…",
  "Image unavailable": "图片不可用",
  Images: "图片",
  "Open or create a session before attaching images.": "请先打开或新建一个会话，再添加图片。",
  Input: "输入",
  Output: "输出",
  Error: "错误",
  "Running…": "运行中…",
  "Thought for a while": "思考了一会儿",
  "Thought for {duration}": "思考用时 {duration}",
  "Reading {n} file": "正在读取 {n} 个文件",
  "Reading {n} files": "正在读取 {n} 个文件",
  "Read {n} file": "读取了 {n} 个文件",
  "Read {n} files": "读取了 {n} 个文件",
  "Searching for {n} pattern": "正在搜索 {n} 个模式",
  "Searching for {n} patterns": "正在搜索 {n} 个模式",
  "Searched for {n} pattern": "搜索了 {n} 个模式",
  "Searched for {n} patterns": "搜索了 {n} 个模式",
  "Listing {n} directory": "正在列出 {n} 个目录",
  "Listing {n} directories": "正在列出 {n} 个目录",
  "Listed {n} directory": "列出了 {n} 个目录",
  "Listed {n} directories": "列出了 {n} 个目录",
  "No result recorded.": "没有记录到结果。",
  "Read file": "读取文件",
  "List directory": "列出目录",
  "Write file": "写入文件",
  "Run command": "运行命令",
  "Search memory": "搜索记忆",
  "Write memory": "写入记忆",
  "Delegate task": "委派任务",
  "Tool calls": "工具调用",
  "{label} ×{n}": "{label} ×{n}",
  "{n} failed": "{n} 个失败",

  // context meter
  "Context window": "上下文窗口",
  "Session stats": "会话统计",
  Messages: "消息",
  "System tools": "系统工具",
  "System prompt": "系统提示",
  Remaining: "剩余空间",
  "Turns / steps": "轮次 / 步数",
  "Total tokens": "累计 Token",
  "Average speed": "平均速度",
  "Cache hit": "缓存命中",
  "Estimated from character share.": "组成按字符比例估算。",
  "No model calls yet.": "还没有模型调用。",
  "Context usage": "上下文占用",

  // approvals
  "Approval required": "需要审批",
  "Approval required ({count})": "需要审批（{count} 项）",
  "Low risk": "低风险",
  "Medium risk": "中风险",
  "High risk": "高风险",
  Approve: "批准",
  "Approve and run": "批准并运行",
  Reject: "拒绝",
  "Expires in {time}": "{time} 后过期",
  "Action digest {digest}": "动作摘要 {digest}",
  "This approval applies only to this command and these arguments.":
    "批准仅对这条命令和这组参数生效。",

  // run bar and composer
  Steps: "步数",
  Time: "时间",
  Cancel: "取消",
  Send: "发送",
  Label: "标签",

  // settings: general
  Workspace: "工作区",
  "The folder the agent may read and edit.": "Agent 可以读取和编辑的文件夹。",
  "Absolute workspace path": "工作区绝对路径",
  "Choose folder": "选择文件夹",
  Save: "保存",
  Saved: "已保存",
  General: "通用",
  "Generated interface": "生成的界面",
  "Back to chat": "返回对话",
  "Settings are stored on this node.": "设置保存在当前节点上。",
  "Workspace boundary, language and the node this window is connected to.":
    "工作区边界、语言，以及这个窗口连接的节点。",
  "Interface language": "界面语言",
  "Changes apply to this node and to every shell connected to it.":
    "更改会应用到当前节点，以及所有连接到它的外壳。",
  English: "English",
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
  Active: "当前",
  "{n} model": "{n} 个模型",
  "{n} models": "{n} 个模型",
  "Key configured": "已配置密钥",
  "No key": "无密钥",
  "No authentication": "无认证",
  "Provider saved.": "供应商已保存。",
  "Provider deleted.": "供应商已删除。",
  "Provider is now active.": "供应商已设为当前。",
  "The provider returned no models.": "供应商没有返回任何模型。",
  "Delete provider {name}? This cannot be undone.": "删除供应商 {name}？此操作无法撤销。",
  "New provider": "新建供应商",
  Provider: "供应商",
  Connection: "连接",
  Name: "名称",
  "My provider": "我的供应商",
  "Anthropic Messages": "Anthropic Messages",
  "OpenAI Chat Completions": "OpenAI Chat Completions",
  "OpenAI Responses": "OpenAI Responses",
  "Base URL": "基础 URL",
  "API key": "API 密钥",
  "Leave empty to keep the current key.": "留空则保留当前密钥。",
  "Show key": "显示密钥",
  "Hide key": "隐藏密钥",
  "Current key: {hint}": "当前密钥：{hint}",
  "The key will be cleared when you save.": "保存后将清除密钥。",
  "Clear key": "清除密钥",
  Website: "网站",
  Notes: "备注",
  Models: "模型",
  "Add model": "添加模型",
  "No models yet. Add one manually or fetch the provider's model list.":
    "还没有模型。可以手动添加，或获取供应商的模型列表。",
  "Remove model": "移除模型",
  "Custom headers": "自定义请求头",
  "Headers are sent with every request. Authorization and x-api-key are managed by the gateway and cannot be overridden here.":
    "这些请求头会随每次调用发送。Authorization 与 x-api-key 由网关管理，不能在这里覆盖。",
  Header: "请求头",
  Value: "值",
  "Remove header": "移除请求头",
  "Add header": "添加请求头",
  "Connected in {ms} ms, {n} model available.": "连接成功，耗时 {ms} ms，可用模型 {n} 个。",
  "Connected in {ms} ms, {n} models available.": "连接成功，耗时 {ms} ms，可用模型 {n} 个。",
  "Connection failed: {error}": "连接失败：{error}",
  "Saving…": "保存中…",
  "Test connection": "测试连接",
  "Save & test": "保存并测试",
  "Set as active": "设为当前",
  "Delete provider": "删除供应商",

  // run / node states
  Scheduled: "已排程",
  Running: "运行中",
  "Waiting for approval": "等待审批",
  Succeeded: "已完成",
  Failed: "失败",
  Aborted: "已中止",
  "Needs review": "待核对",
  Normal: "正常",
  Degraded: "降级",
  Maintenance: "维护",

  // settings: memory
  Memory: "记忆",
  "Facts the agent remembers across runs, stored in the local data directory. Deleting a fact erases it and its stored source excerpt from memory; past conversations are not rewritten.":
    "Agent 跨运行记住的事实在本地数据目录中。删除一条事实会同时从记忆中删除它的原文；过去的对话记录不会被改写。",
  "{n} fact": "{n} 条事实",
  "{n} facts": "{n} 条事实",
  "{n} active": "{n} 条有效",
  "{n} source excerpt": "{n} 条原文",
  "{n} source excerpts": "{n} 条原文",
  "Add a stable fact the agent should remember": "添加一条 Agent 应记住的稳定事实",
  Project: "项目",
  User: "用户",
  Run: "运行",
  Scope: "范围",
  Add: "添加",
  "Search memories": "搜索记忆",
  Search: "搜索",
  Clear: "清除",
  Delete: "删除",
  "Confirm delete": "确认删除",
  "Discard unsaved changes?": "放弃未保存的更改？",
  Discard: "放弃",
  "Something went wrong.": "出错了。",
  Reload: "重新加载",
  "Click delete again to confirm permanent deletion.": "再次点击删除以确认永久删除。",
  "Deleted facts: {records} · source excerpts: {episodes}":
    "已删除事实：{records} 条 · 原文：{episodes} 条",
  "Memory saved.": "记忆已保存。",
  "This fact is already stored.": "这条事实已经存在。",
  "No memories match this search.": "没有匹配的记忆。",
  "Showing up to 50 matches.": "最多显示前 50 条匹配结果。",
  "No memories yet. The agent can save facts with its memory tool.":
    "还没有记忆。Agent 可以用记忆工具保存事实。",
  Redacted: "已脱敏",
  Confidence: "置信度",
  "From user": "来自用户",
  "From system": "来自系统",
  "From tool": "来自工具",
  "From model": "来自模型",
  "From external": "来自外部",
  "State active": "有效",
  "State invalidated": "已失效",
  "State superseded": "已取代",

  // chat shell: model picker, markdown, notifications, tool errors
  "API key (optional)": "API 密钥（可选）",
  "Add manually": "手动添加",
  "Add model IDs manually": "手动添加模型 ID",
  "Add model by id": "按 ID 添加模型",
  "Add {n} model": "添加 {n} 个模型",
  "Add {n} models": "添加 {n} 个模型",
  "Added {n} model.": "已添加 {n} 个模型。",
  "Added {n} models.": "已添加 {n} 个模型。",
  Advanced: "高级",
  Back: "返回",
  "Base URL is required.": "基础 URL 不能为空。",
  "Base URL must start with http:// or https://.": "基础 URL 必须以 http:// 或 https:// 开头。",
  "Choose the model and thinking effort": "选择模型与思考强度",
  "Choose a model first.": "请先选择一个模型。",
  "A run is already active.": "已有运行正在进行，请先等待或取消。",
  Code: "代码",
  "Comma or newline separated": "用逗号或换行分隔",
  "Connect an endpoint, then choose the models to add.": "先连接端点，再选择要添加的模型。",
  Continue: "继续",
  Copied: "已复制",
  Copy: "复制",
  Defaults: "默认设置",
  Details: "详情",
  Default: "默认",
  Dismiss: "关闭",
  "(empty)": "（空）",
  "Existing models are kept; selected models are added.": "现有模型会保留；勾选的模型会追加。",
  "Fetch models": "获取模型",
  "Fetching models…": "正在获取模型…",
  "Extra high": "极高",
  High: "高",
  "Input {input} · Output {output} tokens": "输入 {input} · 输出 {output} 个 Token",
  "Keep current key": "保留当前密钥",
  Low: "低",
  Mapped: "已映射",
  Maximum: "最高",
  Medium: "中",
  Model: "模型",
  "Model scale (1-5)": "模型规模（1–5）",
  "Models without a mapping use default settings.": "没有映射结果的模型使用默认设置。",
  "Name is required.": "名称不能为空。",
  "No details were recorded.": "没有记录到详情。",
  "No model": "无模型",
  "No model provider is configured. Add one in Settings.":
    "还没有配置模型供应商。请先在设置中添加一个。",
  "No models are enabled for this provider.": "这个供应商没有启用的模型。",
  "Optional note, one line": "可选备注，一行",
  "Optional. Leave empty for endpoints that do not require authentication.":
    "可选。不需要认证的端点可以留空。",
  Primary: "主模型",
  Progress: "进度",
  "Reasoning hidden by the provider": "推理内容已被供应商隐藏",
  "Request format": "请求格式",
  "Restore window": "还原窗口",
  Retry: "重试",
  "Save the provider before adding models.": "请先保存供应商，再添加模型。",
  "Search models": "搜索模型",
  "Select all": "全选",
  "Select none": "全不选",
  "Select the models to add to this provider.": "选择要添加到这个供应商的模型。",
  Stopped: "已停止",
  "The model cannot be changed while a run is active.": "运行进行中，无法切换模型。",
  Thinking: "思考",
  Tokens: "Token 数",
  "Tool failed": "工具失败",
  "Tool failed: {name}": "工具失败：{name}",
  "Tool result: {name}": "工具结果：{name}",
  "Website must start with http:// or https://.": "官网必须以 http:// 或 https:// 开头。",
  "{selected} of {total} selected": "已选择 {selected} / {total}",
};

export function t(key: string): string {
  if (current() === "en") return key;
  return ZH[key] ?? key;
}

export function tArgs(key: string, args: Record<string, string | number>): string {
  let text = t(key);
  for (const [name, value] of Object.entries(args)) {
    text = text.split(`{${name}}`).join(String(value));
  }
  return text;
}

/**
 * Plural-aware lookup. English one/other forms are both message ids; the
 * Chinese catalog maps both to the same sentence because Chinese does not
 * inflect the noun. Tense is not handled here: callers pick the message id
 * (for example "Read {n} files" vs "Reading {n} files").
 */
export function tPlural(
  one: string,
  other: string,
  count: number,
  args: Record<string, string | number> = {},
): string {
  const form =
    new Intl.PluralRules(current() === "zh-CN" ? "zh-CN" : "en").select(count) === "one"
      ? one
      : other;
  return tArgs(form, { n: count, ...args });
}
