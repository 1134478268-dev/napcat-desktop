//! Mutex 访问统一入口。
//!
//! 为什么必须统一：项目里 35 处 `state.xxx.lock().unwrap()`，任何线程一旦
//! panic（哪怕只是一次索引越界），对应 Mutex 就进入中毒状态，之后**所有**
//! 命令和 watcher 线程的 lock().unwrap() 都会连锁 panic，表现为"点任何按钮
//! 都没反应、进程还活着"。这是典型的雪崩式故障，而不是单点故障。
//!
//! 这里把"取锁"收敛成一个函数：中毒时打印告警并取回内部数据继续跑。
//! 对于本项目的状态（bool / Option<pid> / 配置副本），中毒后继续使用是安全的，
//! 因为这些值都是整体替换，不存在"改到一半"的中间态。

use std::sync::{Mutex, MutexGuard};

/// 取锁，中毒时恢复而不是 panic。
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("[warn] mutex poisoned, recovering inner value");
            poisoned.into_inner()
        }
    }
}
