//! Android shell glue that must run before the node starts making requests.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use jni::jni_sig;
use jni::jni_str;
use jni::objects::{JObject, JString};

/// Directory name shared with the Kotlin side (`PhiApplication`) and with the
/// Lumen Crash SDK's `externalInboxDirectory`.
pub const CRASH_INBOX_DIR_NAME: &str = "crash-inbox";

/// Reading stops at 64 KiB per file on the SDK side; keep every report well
/// below that so a truncated (and therefore unparsable) JSON never happens.
const MAX_STACK_CHARS: usize = 12 * 1024;
const MAX_MESSAGE_CHARS: usize = 2 * 1024;

static CRASH_INBOX: OnceLock<PathBuf> = OnceLock::new();

/// Register the JVM handles rustls needs to verify TLS peers through Android's
/// platform certificate verifier.
///
/// reqwest's rustls backend hands every server certificate to
/// `rustls-platform-verifier` on this target. The verifier panics on the first
/// HTTPS request unless it has been given the app's JVM and context, and
/// release builds abort on panic, which surfaces as an app crash.
pub fn init_platform_certificates() {
    let context = ndk_context::android_context();
    // SAFETY: tao initializes the ndk-context pointers before it invokes the
    // Rust entry point, and the application context is a global reference that
    // outlives the process.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
    let outcome = vm.attach_current_thread(|env| {
        let app_context = unsafe { JObject::from_raw(env, context.context().cast()) };
        rustls_platform_verifier::android::init_with_env(env, app_context)
    });
    if let Err(error) = outcome {
        eprintln!("failed to initialize the Android certificate verifier: {error}");
    }
}

/// Point the panic hook at the app's crash inbox and install it.
///
/// Rust panics are invisible to the JVM: on this shell a panic that escapes the
/// entry point ends in `std::process::abort()`, so the process dies with
/// SIGABRT and the only JVM-side evidence is an exit reason. The hook writes
/// what the tombstone cannot: the panic message, the thread name and a captured
/// backtrace. The next launch turns that file into a crash report through the
/// Lumen Crash SDK inbox.
pub fn init_crash_reports() {
    let Some(inbox) = files_dir().map(|files| files.join(CRASH_INBOX_DIR_NAME)) else {
        eprintln!("crash inbox unavailable: the app files directory could not be resolved");
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&inbox) {
        eprintln!("crash inbox unavailable: {error}");
        return;
    }
    let _ = CRASH_INBOX.set(inbox);
    install_panic_hook();
}

/// Records a failure the webview reported (`window.onerror`,
/// `unhandledrejection`).
///
/// These are recoverable by definition — the UI catches them and keeps
/// running — so the entry is written as non-fatal: the SDK keeps it as evidence
/// for the next real crash instead of blocking the next launch with a crash
/// screen.
pub fn record_web_failure(source: &str, message: &str, stack: &str) -> Result<(), String> {
    let source = if source.trim().is_empty() {
        "webview.js"
    } else {
        source.trim()
    };
    write_failure(source, message, stack, false, None).map(|_| ())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = panic_message(info);
        let thread = std::thread::current().name().map(str::to_owned);
        // `force_capture` ignores RUST_BACKTRACE, so this works in release
        // builds where the process aborts right after the hook returns.
        let mut stack = String::new();
        if let Some(location) = info.location() {
            stack.push_str(&format!(
                "panic at {}:{}:{}\n",
                location.file(),
                location.line(),
                location.column()
            ));
        }
        stack.push_str(&std::backtrace::Backtrace::force_capture().to_string());
        if let Err(error) = write_failure("rust.panic", &message, &stack, true, thread.as_deref()) {
            eprintln!("failed to record the panic: {error}");
        }
        previous(info);
    }));
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "panic (non-string payload)".to_owned()
    }
}

/// Writes one inbox entry. The file appears atomically: a partially written
/// `.json` would be ingested as a malformed report and then retried forever.
fn write_failure(
    source: &str,
    message: &str,
    stack: &str,
    fatal: bool,
    thread: Option<&str>,
) -> Result<(), String> {
    let Some(inbox) = CRASH_INBOX.get() else {
        return Err("the crash inbox was not initialised".to_owned());
    };
    let payload = serde_json::json!({
        "source": source,
        "message": truncate(message, MAX_MESSAGE_CHARS),
        "stackTrace": truncate(stack, MAX_STACK_CHARS),
        "timestampMillis": now_millis(),
        "thread": thread.unwrap_or("unknown"),
        "fatal": fatal,
    });
    let name = format!(
        "{}-{}-{}.json",
        source.replace(['/', '\\', ':'], "-"),
        now_millis(),
        std::process::id(),
    );
    let temp = inbox.join(format!("{name}.tmp"));
    std::fs::write(&temp, payload.to_string()).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, inbox.join(name)).map_err(|error| error.to_string())
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(limit).collect();
    kept.push_str("\n… (truncated)");
    kept
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Resolves `Context.getFilesDir()` through JNI.
///
/// The Kotlin installer uses `File(filesDir, "crash-inbox")`, so both runtimes
/// must agree on the directory. Reading it from the JVM keeps a single source of
/// truth instead of duplicating the Android data layout on the Rust side.
fn files_dir() -> Option<PathBuf> {
    let context = ndk_context::android_context();
    // SAFETY: tao initializes the ndk-context pointers before it invokes the
    // Rust entry point.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
    let outcome = vm.attach_current_thread(|env| -> jni::errors::Result<String> {
        let app_context = unsafe { JObject::from_raw(env, context.context().cast()) };
        let file = env
            .call_method(
                &app_context,
                jni_str!("getFilesDir"),
                jni_sig!("()Ljava/io/File;"),
                &[],
            )?
            .l()?;
        let path = env
            .call_method(
                &file,
                jni_str!("getAbsolutePath"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let path: JString = env.cast_local::<JString>(path)?;
        Ok(path.mutf8_chars(env)?.into())
    });
    match outcome {
        Ok(path) if !path.is_empty() => Some(Path::new(&path).to_path_buf()),
        Ok(_) => None,
        Err(error) => {
            eprintln!("failed to resolve the Android files directory: {error}");
            None
        }
    }
}
