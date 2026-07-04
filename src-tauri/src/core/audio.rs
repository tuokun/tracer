//! WASAPI 音频峰值探测 —— 阶段一 S4
//!
//! 取默认渲染设备的 `IAudioMeterInformation` 峰值；高于阈值即视为“正在播放”，
//! 供休眠判定的音频豁免使用（见 `docs/方案评审记录.md` 一·5）。

use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// 峰值高于此阈值视为“正在播放”（对齐原方案 WASAPI 经验值）。
const PLAY_THRESHOLD: f32 = 1e-8;

/// 默认渲染设备是否正在播放声音。任一步骤失败均返回 `false`，绝不 panic。
#[allow(dead_code)] // 阶段二休眠判定接入（评审一·5）
pub fn is_playing() -> bool {
    peak().map(|p| p > PLAY_THRESHOLD).unwrap_or(false)
}

/// 默认渲染设备当前峰值（0.0..=1.0）。无音频设备 / COM 异常时返回 `None`。
pub fn peak() -> Option<f32> {
    unsafe {
        // COM 初始化：忽略“已初始化”等返回，下游失败自然降级。
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None).ok()?;
        meter.GetPeakValue().ok()
    }
}
