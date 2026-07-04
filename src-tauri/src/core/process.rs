//! 进程解析 —— 阶段一 S2
//!
//! `HWND → PID → 可执行文件绝对路径 → 进程名`。全程 `Option` 降级，回调里绝不 panic。

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};

/// 一个前台窗口对应的进程信息。
#[derive(Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
}

/// 解析前台窗口对应的进程信息。解析失败（无效句柄/权限不足/进程已退出）返回 `None`。
///
/// UWP 特殊处理：其前台窗口句柄属于 `ApplicationFrameHost.exe`，需遍历子窗口取真实
/// 业务进程（如 `Microsoft.Windows.Photos.exe`）；子进程尚未出现时降级返回 frame host。
pub fn resolve(hwnd: HWND) -> Option<ProcessInfo> {
    unsafe {
        let pid = pid_of(hwnd)?;
        let path = image_path(pid)?;
        let name = file_name(&path).to_owned();
        if name.eq_ignore_ascii_case("ApplicationFrameHost.exe") {
            if let Some(real) = resolve_uwp_child(hwnd) {
                return Some(real);
            }
        }
        Some(ProcessInfo { pid, name, path })
    }
}

unsafe fn pid_of(hwnd: HWND) -> Option<u32> {
    let mut pid: u32 = 0;
    // 返回值为线程 id；pid 写入 out 参数。无效句柄时 pid 保持 0。
    GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
    if pid == 0 {
        None
    } else {
        Some(pid)
    }
}

unsafe fn image_path(pid: u32) -> Option<String> {
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let _guard = HandleGuard(handle); // RAII：离开作用域自动 CloseHandle

    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len)
        .ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// 遍历 UWP frame 的子窗口，找到首个非 `ApplicationFrameHost` 的真实业务进程。
unsafe fn resolve_uwp_child(frame: HWND) -> Option<ProcessInfo> {
    let mut found: Option<ProcessInfo> = None;
    // 通过 LPARAM 把结果槽指针传给回调（WNDENUMPROC 不能捕获上下文）。
    let slot = LPARAM(&mut found as *mut Option<ProcessInfo> as isize);
    let _ = EnumChildWindows(Some(frame), Some(enum_child_proc), slot);
    found
}

unsafe extern "system" fn enum_child_proc(child: HWND, lparam: LPARAM) -> BOOL {
    let slot = &mut *(lparam.0 as *mut Option<ProcessInfo>);
    let Some(pid) = pid_of(child) else {
        return BOOL::from(true); // 取不到 PID，继续下一个
    };
    let Some(path) = image_path(pid) else {
        return BOOL::from(true); // 权限不足/已退出，继续
    };
    if file_name(&path).eq_ignore_ascii_case("ApplicationFrameHost.exe") {
        return BOOL::from(true); // 跳过 frame host 自身
    }
    let name = file_name(&path).to_owned();
    *slot = Some(ProcessInfo { pid, name, path });
    BOOL::from(false) // 命中真实进程，停止枚举
}

/// 从绝对路径取文件名（兼容 `\` 与 `/` 的最后一段）。
fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// RAII 句柄守卫，确保 `CloseHandle` 被调用。
struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
