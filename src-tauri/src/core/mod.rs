//! Tracer 核心模块：感知层（窗口/音频/空闲/电源）与计时落库。
//! 阶段一只实现感知与事件投递，详见 `docs/实现计划.md`。

pub mod audio;
pub mod event;
pub mod idle;
pub mod owner;
pub mod power;
pub mod process;
pub mod tracker;
