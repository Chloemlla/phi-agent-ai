# Android 崩溃诊断（Lumen Crash SDK 接入）

> 状态：🌱 草稿 · 最后更新：2026-11-22
> 相关：`docs/60-frontend/multi-frontend.md`、`docs/70-quality/quality-attributes.md`

---

## 1. 要解决的问题

Android 外壳是「Kotlin 宿主 + Rust 运行时 + WebView 前端」三层，**Rust 与 WebView
的死亡对 JVM 完全不可见**：

| 现象 | JVM 能看到什么 | 用户看到什么 |
|---|---|---|
| Rust panic 逸出入口点 → `std::process::abort()` | 没有任何 Java 异常，只有进程退出 | 闪退，桌面图标重进后一切正常 |
| 后台 worker 线程 panic | 同上（进程可能还活着） | 某个操作静默失败 |
| WebView renderer 进程死亡 | 主进程存活 | 白屏，重进恢复 |
| 主线程阻塞在 Rust/JNI | ANR（系统弹窗） | 转圈 → 系统「应用无响应」 |
| 前端未捕获异常 | 页面里抛错 | 局部坏掉或整页空白 |

`getHistoricalProcessExitReasons()` 只给出「上次进程死于 `REASON_CRASH_NATIVE`」这类
**结论**，拿不到 panic 消息、线程名和调用栈。没有这层信息，线上反馈只能靠「打开就闪退」
这样的描述，无法定位。

因此目标不是「再包一层 try/catch」，而是**把三个运行时各自的证据汇到同一份报告里**，
在**下一次启动**时展示给用户并提供复制/分享。

---

## 2. 数据通路

```
   崩溃时刻（进程正在死）                       下一次启动
 ┌──────────────────────────┐            ┌───────────────────────────────┐
 │ Rust panic hook          │            │ PhiApplication.onCreate       │
 │  message / thread / BT   │            │  LumenCrash.installSafely {   │
 │        ↓ write.json      │            │    externalInboxDirectory     │
 │ <files>/crash-inbox/*.json│  ────────▶ │    crashReportBackend=false   │
 └──────────────────────────┘   ingest   │  }                            │
 ┌──────────────────────────┐            │        ↓ 合并成一份 CrashReport │
 │ 前端 window.onerror      │            │  LaunchGateActivity           │
 │  → invoke(record_web_    │  ────────▶ │   pending? CrashReportActivity│
 │    failure) → 同目录      │  fatal:    │            : MainActivity     │
 └──────────────────────────┘  false     └───────────────────────────────┘
 ┌──────────────────────────┐
 │ Android 自身              │
 │  exit reason / tombstone  │  ────────▶ 由 SDK 在 install 时读取
 │  ANR watchdog             │
 └──────────────────────────┘
```

三方的职责边界：

- **SDK**：进程退出原因、native tombstone、ANR/冻结检测、inbox 文件解析、报告的序列化与展示。
- **Rust**：只能提供 panic 时刻的信息（消息、线程、backtrace），写成 inbox 文件。
- **前端**：只能提供 JS 异常的上下文，同样写 inbox，但标记为非致命。

---

## 3. 五类失败的报告含义

| 失败类型 | 报告里的关键线索 | SDK 归因 |
|---|---|---|
| Rust panic → abort | `kind=crash`，`component=native`，退出原因 `REASON_CRASH_NATIVE`，`stackTrace` 首行是 panic 消息，栈里出现 `libphi_agent_lib.so` | 主进程死亡，占「待展示」槽位 |
| worker 线程 panic | inbox 文件存在，`kind=crash` 但无退出原因 | 占槽位（进程若无退出原因则下次启动才可见） |
| WebView renderer 崩溃 | 进程退出记录里出现 `:sandboxed_process0`，`component=webview` | **不占槽位**：主进程还活着，弹崩溃页是误报 |
| ANR / 主线程冻结 | `kind=anr` / `kind=freeze`，`component=main-thread` | 冻结不占槽位；ANR 按是否已有真崩溃决定 |
| 前端异常 | inbox 文件 `fatal=false`，`kind=external` | 只作为证据，**绝不弹崩溃页** |

`fatal` 是关键约定：inbox 里默认 `true`（有文件通常意味着进程死了），JS 侧必须显式写
`false`，否则一条 `TypeError` 会让用户下次启动看到崩溃页。

---

## 4. 三个接入点

### 4.1 Kotlin 宿主（`PhiApplication`）

```kotlin
val inbox = File(filesDir, "crash-inbox")
runCatching { inbox.mkdirs() }
LumenCrash.installSafely(this) {
  appDisplayName = getString(R.string.app_name)
  externalInboxDirectory = inbox
  crashReportBackendEnabled = false   // 本地优先：不自动上传
  anrWatchdogEnabled = true
  priorExitCaptureEnabled = true
  startupHangWatchdogEnabled = false  // 首帧在 WebView 里，无法从 Android 侧判定
}
```

用 `installSafely`：SDK 的完整性校验是 fail-closed 的，绝不能因为它让应用启动失败。
`startupHangWatchdogEnabled` 保持关闭——首帧由 WebView 决定，「界面一直没出现」在
Android 侧无法区分「启动慢」和「真的挂了」，开启只会制造误报。

### 4.2 启动路由（`LaunchGateActivity`）

`MainActivity : TauriActivity` 的 `onCreate` 会创建 Rust WebView，**正是可能崩溃的代码路径**。
从那里弹崩溃页会：① 重新进入故障路径，② 让崩溃页被一个起不来的 WebView 盖住。

所以 MAIN/LAUNCHER 交给 `LaunchGateActivity`（`android:Theme.NoDisplay`，不建 WebView）：

```
LaunchGateActivity.onCreate
  ├─ LumenCrash.loadPendingReportSafely() != null → CrashReportActivity（Compose）
  └─ 否则                                        → MainActivity（Tauri）
```

`CrashReportActivity` 只做宿主：`setContent { LumenCrashReportScreen(report, onContinue = ::launchApp) }`；
用户点「清除并继续」时 SDK 自己清空待展示报告，随后启动 `MainActivity`。

### 4.3 Rust 与前端

Rust 侧（`src-tauri/src/android.rs`）：

- `init_crash_reports()` 在 `run()` 早期调用：JNI 取 `getFilesDir()` → `crash-inbox`，
  然后替换 panic hook；
- hook 里用 `Backtrace::force_capture()`（不受 `RUST_BACKTRACE` 影响）并写
  `<source>-<millis>-<pid>.json`，先写 `.tmp` 再 rename，避免半截 JSON；
- 消息 2 KiB、栈 12 KiB 截断：SDK 单文件上限 64 KiB，超限即整体解析失败、报告丢失。

前端（`src/platform/crash.ts`）：

- `installWebFailureReporting()` 在 `render()` 之前挂 `error` / `unhandledrejection`；
- 去重 + 每次会话最多 5 条，避免渲染循环把 inbox 刷满；
- Solid `ErrorBoundary` 的 fallback 也会补一条 `webview.solid`。

---

## 5. inbox 文件格式

`<files>/crash-inbox/*.json`：

```json
{
  "source": "rust.panic",
  "message": "called `Option::unwrap()` on a `None` value",
  "stackTrace": "panic at src/foo.rs:42:9\n   0: ...",
  "timestampMillis": 1732219200000,
  "thread": "main",
  "fatal": true
}
```

- `fatal`：`true` 才允许占用「待展示」槽位；前端写 `false`。
- 别名：`msg`/`error`、`stack`/`backtrace`、`timestamp`/`occurredAtMillis`、`isFatal`。
- `*.txt` / `*.log` 也可（首行消息、其余栈），一律按 `fatal=true` 处理。
- 只有成功转成报告才删除文件；解析失败留在原地，下次启动重试。

---

## 6. 隐私与边界

- `crashReportBackendEnabled = false`：**不自动上传**。报告默认只在本机。
- 报告里的复制/分享是显式用户动作（含 SDK 的红acted 文本与粘贴上传选项）。
- 报告会经过 SDK 的脱敏规则（家目录路径、`content://`/`file://`、`Bearer`、
  `token=/key=/secret=` 等）后才对用户展示。
- 已知取舍：宿主不能改崩溃页文案样式（SDK 自带 Compose 主题），换来的是跨应用一致的
  诊断体验；AAR 体积（含 Compose Material3）由 R8 收缩。

---

## 7. 拿到报告后的读法

```
① kind / component   → 谁死了：native（Rust）、webview、main-thread、external
② 退出证据 exitReason → REASON_CRASH_NATIVE 是 SIGABRT，配合 stackTrace 首行看 panic 消息
③ 栈里的库名          → libphi_agent_lib.so 帧 = Rust；libwebviewchromium = renderer
④ 面包屑             → 崩溃前最后的工具调用 / 模型请求 / 前端异常
```

没有 inbox 文件、只有 `REASON_CRASH_NATIVE` 时，属于「panic 消息丢失」——
说明 panic 发生在 hook 装好之前，或写文件失败（存储满 / 目录不可写）。

---

## 8. 已知缺口

- 桌面端没有崩溃页（SDK 只覆盖 Android）；桌面崩溃仍靠日志与 `phi-headless`。
- `startupHangWatchdogEnabled` 关闭，冷启动卡死只能靠系统 ANR 发现。
- Rust backtrace 未符号化：报告里有地址与 `.so` 名，符号化需要保留未 strip 的构建产物。
- WebView renderer 的 tombstone 需要 API 31+；更低版本只有退出原因字符串。

---

## 9. 相关文件

| 位置 | 作用 |
|---|---|
| `src-tauri/gen/android/app/src/main/java/com/sukirya/phi/PhiApplication.kt` | 安装 SDK、创建 inbox 目录 |
| `.../LaunchGateActivity.kt` | 启动路由 |
| `.../CrashReportActivity.kt` | 承载 SDK 崩溃页 |
| `src-tauri/src/android.rs` | panic hook、inbox 写入、`getFilesDir` JNI |
| `src-tauri/src/api.rs` | `record_web_failure` 命令 |
| `src/platform/crash.ts` | 前端未捕获异常的收集 |
| `scripts/fetch-lumen-crash.mjs` | 从 GitHub Release 落地 SDK 产物到本地 Maven 树 |
| `docs/80-roadmap/scope-additions.md` §2.29 | 范围增补记录 |
