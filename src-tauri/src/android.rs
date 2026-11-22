//! Android shell glue that must run before the node starts making requests.

use jni::objects::JObject;

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
