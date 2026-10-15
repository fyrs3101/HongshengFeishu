// Android 专属：获取 WifiManager MulticastLock。
//
// 为什么需要它：华为等厂商的 Wi-Fi 驱动在省电策略下会过滤掉广播/组播报文，
// 导致 UDP 广播发现（局域网自动发现）收不到同事的通告。标准做法是持有
// MulticastLock（所有 SSDP/DLNA/局域网发现类应用的通用方案）。
//
// 实现方式：Rust 侧直接走 JNI 调用系统 API，不需要改 Android 工程
// （gen/android 由 CI 每次重新生成，仓库不提交，无法持久化 Java/Kotlin 代码）。
// 任何一步失败（如系统限制、缺权限）都静默降级——此时子网定向广播仍可工作。
#![cfg(target_os = "android")]

use std::sync::OnceLock;

static ACQUIRED: OnceLock<bool> = OnceLock::new();

/// 尝试获取 MulticastLock（整个进程生命周期只执行一次，失败不重试）
pub fn acquire() {
    let _ = ACQUIRED.get_or_init(|| try_acquire().is_ok());
}

fn try_acquire() -> Result<(), String> {
    use jni::objects::{GlobalRef, JObject, JValue};
    use jni::JavaVM;
    use ndk_context::android_context;

    let ctx = android_context();
    // 注意：from_raw 属于包装器类型 jni::JavaVM（jni::sys::JavaVM 只是裸指针类型别名，没有该方法）
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(ctx.context() as *mut jni::sys::_jobject) };
    if activity.is_null() {
        return Err("activity is null".into());
    }

    let result = (|| -> Result<GlobalRef, String> {
        // Context.getSystemService("wifi")
        let svc = env.new_string("wifi").map_err(|e| e.to_string())?;
        let wifi = env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&svc)],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        if wifi.is_null() {
            return Err("wifi service is null".into());
        }
        // WifiManager.createMulticastLock(tag)
        let tag = env
            .new_string("hongsheng_feishu_lan")
            .map_err(|e| e.to_string())?;
        let lock = env
            .call_method(
                &wifi,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[JValue::Object(&tag)],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        if lock.is_null() {
            return Err("multicast lock is null".into());
        }
        // MulticastLock.acquire()
        env.call_method(&lock, "acquire", "()V", &[])
            .map_err(|e| e.to_string())?
            .v()
            .map_err(|e| e.to_string())?;
        // 持有全局引用，让锁与进程同生命周期（仅一次，刻意不释放）
        env.new_global_ref(lock).map_err(|e| e.to_string())
    })();

    match result {
        Ok(global) => {
            // 进程级单例：故意泄漏这一个 GlobalRef，避免 Drop 时释放 JNI 全局引用
            std::mem::forget(global);
            Ok(())
        }
        Err(e) => {
            // Java 侧抛异常（如 SecurityException）时清理挂起异常，避免后续 JNI 调用崩溃
            let _ = env.exception_clear();
            Err(e)
        }
    }
}
