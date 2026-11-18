//! Ask the player for the private-server address via the platform text dialog.
//!
//! The address has to be typeable: a hostname is letters, digits, dots and a
//! colon, and the in-game overlay draws with a 5x7 ASCII bitmap font that has no
//! text-input machinery at all. Rather than build a soft keyboard inside the
//! overlay, this reaches `MainActivity.promptServerAddress` over JNI — the same
//! classloader dance [`crate::orientation`] uses — and gets the OS keyboard, IME
//! and clipboard paste for free.
//!
//! The Java side WRITES the endpoint file (`wie_server.txt`) itself, so nothing
//! has to be handed back across the boundary: [`crate::network::AndroidNetwork`]
//! re-reads it on the game's next connect. That keeps this a one-way, fire-and-
//! forget call with no shared state to invalidate and no restart required.
//!
//! All failures are logged, never fatal.

/// Open the server-address dialog, pre-filled with `current` (`[tls://]host:port`).
#[cfg(target_os = "android")]
pub fn prompt(current: &str) {
    match call_with_string("promptServerAddress", current) {
        Ok(()) => log::info!("server-address dialog opened (current = {current})"),
        Err(e) => log::error!("promptServerAddress failed: {e:?}"),
    }
}

#[cfg(not(target_os = "android"))]
pub fn prompt(_current: &str) {}

/// Open the account-code dialog, pre-filled with the identity as 32 hex chars.
///
/// Separate from [`prompt`] because it is a different kind of value with a
/// different risk: this one IS the credential, and replacing it silently
/// re-points every online feature at another account.
#[cfg(target_os = "android")]
pub fn prompt_account(current_hex: &str) {
    match call_with_string("promptAccountCode", current_hex) {
        Ok(()) => log::info!("account-code dialog opened"),
        Err(e) => log::error!("promptAccountCode failed: {e:?}"),
    }
}

#[cfg(not(target_os = "android"))]
pub fn prompt_account(_current_hex: &str) {}

/// Show the on-device diagnostics dialog (settings -> DIAG). `info` is the Rust
/// half of the text (audio status, sizes, mode flags); the Java side prepends
/// app version / Android version / device model and offers a 복사 button — the
/// whole point is that a REMOTE tester can read and paste it without adb.
#[cfg(target_os = "android")]
pub fn show_diagnostics(info: &str) {
    match call_with_string("showDiagnostics", info) {
        Ok(()) => log::info!("diagnostics dialog opened"),
        Err(e) => log::error!("showDiagnostics failed: {e:?}"),
    }
}

#[cfg(not(target_os = "android"))]
pub fn show_diagnostics(_info: &str) {}

/// ★ 2026-10-02 세이브 옮기기: what the boot did with an import the player scheduled
/// (`vault::take_import_notice`) — they closed the app for it, so the relaunch has to say whether
/// it happened. A platform dialog like the rest of 세이브 관리; the first line is its title.
#[cfg(target_os = "android")]
pub fn show_save_message(text: &str) {
    match call_with_string("showSaveMessage", text) {
        Ok(()) => log::info!("save message dialog opened"),
        Err(e) => log::error!("showSaveMessage failed: {e:?}"),
    }
}

#[cfg(not(target_os = "android"))]
pub fn show_save_message(_text: &str) {}

/// ★ 2026-10-28 환경설정의 세이브 행: open a file screen — `spec` = `export|<id>|<file>|<suggested name>`
/// (the system's 「저장 위치」 screen, then a copy of `<file>` to where the player chose) or
/// `import|<id>|<file>` (its 「열기」 screen, then a copy of the chosen file to `<file>`). Java
/// answers `nativeSaveDocResult(id, status)` exactly once. `false` = Java could not be reached —
/// no answer will come.
#[cfg(target_os = "android")]
pub fn save_doc(spec: &str) -> bool {
    match call_with_string("saveDoc", spec) {
        Ok(()) => true,
        Err(e) => {
            log::error!("saveDoc failed: {e:?}");
            false
        }
    }
}

#[cfg(not(target_os = "android"))]
pub fn save_doc(_spec: &str) -> bool {
    false
}

/// ★ 2026-10-28: the start that does not run the game (`vault::boot_blocked`) — a dialog that only
/// closes the app. `false` = it could not be shown.
#[cfg(target_os = "android")]
pub fn show_blocked(text: &str) -> bool {
    match call_with_string("showBlocked", text) {
        Ok(()) => true,
        Err(e) => {
            log::error!("showBlocked failed: {e:?}");
            false
        }
    }
}

#[cfg(not(target_os = "android"))]
pub fn show_blocked(_text: &str) -> bool {
    false
}

/// ★ 2026-10-28: close the app — a file is staged and the next start applies it. If Java cannot be
/// reached the process ends here: a game that went on past a stage would be overwritten by it at
/// the next start (`savebundle` rule 5).
pub fn quit_app() -> ! {
    #[cfg(target_os = "android")]
    {
        match call_with_string("quitAppWith", "") {
            Ok(()) => {
                // `quitApp` ends the process from the UI thread; give it the moment it needs.
                std::thread::sleep(std::time::Duration::from_secs(3));
                log::error!("quitAppWith did not end the process — ending it here");
            }
            Err(e) => log::error!("quitAppWith failed: {e:?} — ending the process here"),
        }
    }
    std::process::exit(0)
}

/// Call `MainActivity.<method>(String)` — one classloader dance for both dialogs.
#[cfg(target_os = "android")]
fn call_with_string(method: &str, arg: &str) -> Result<(), jni::errors::Error> {
    use jni::objects::{JClass, JObject, JValue};

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    // ndk_context hands native code the *Application*, not the Activity, so the
    // app class has to come from the context's classloader — a FindClass on this
    // thread cannot see app classes.
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };

    let result: Result<(), jni::errors::Error> = (|| {
        let loader = env
            .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        let cls_name = env.new_string("net.wie.emulator.MainActivity")?;
        let cls_name_ref: &JObject = &cls_name;
        let cls_obj = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(cls_name_ref)],
            )?
            .l()?;
        let cls = JClass::from(cls_obj);
        let jarg = env.new_string(arg)?;
        let arg_ref: &JObject = &jarg;
        env.call_static_method(&cls, method, "(Ljava/lang/String;)V", &[JValue::Object(arg_ref)])?;
        Ok(())
    })();

    if result.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result
}
