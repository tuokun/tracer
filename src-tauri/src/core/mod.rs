//! Tracer 核心模块：感知层（窗口/音频/空闲/电源）与计时落库。
//! 详见 `docs/实现计划.md`。

pub mod audio;
pub mod config;
pub mod db;
pub mod event;
pub mod iconer;
pub mod idle;
pub mod owner;
pub mod power;
pub mod process;
pub mod repo;
pub mod tracker;
pub mod types;
