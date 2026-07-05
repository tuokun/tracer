//! 进程解析 —— 阶段一 S2
//!
//! `HWND → PID → 可执行文件绝对路径 → 进程名 + 显示名`。全程 `Option` 降级，回调里绝不 panic。
//!
//! `name` 是 exe 文件名（如 `chrome.exe`），用作应用身份去重键。
//! `display_name` 来自 exe PE 资源的 `FileDescription` 字段（如 `Helium`、`Windows 资源管理器`），
//! 这是 Windows 任务管理器显示友好名用的同一字段；为 `None` 表示该 exe 没写版本信息。

use std::ffi::c_void;
use std::iter::once;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};

// version.dll 的版本信息 API（windows crate 未直接绑定，用 raw FFI）。
#[link(name = "version")]
extern "system" {
    fn GetFileVersionInfoSizeW(pFilename: *const u16, pdwHandle: *mut u32) -> u32;
    fn GetFileVersionInfoW(
        pFilename: *const u16,
        dwHandle: u32,
        dwLen: u32,
        pData: *mut c_void,
    ) -> BOOL;
    fn VerQueryValueW(
        pBlock: *const c_void,
        lpSubBlock: *const u16,
        lplpBuffer: *mut *mut c_void,
        puLen: *mut u32,
    ) -> BOOL;
}

/// 一个前台窗口对应的进程信息。
#[derive(Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub display_name: Option<String>,
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
        let display_name = read_file_description(&path);
        if name.eq_ignore_ascii_case("ApplicationFrameHost.exe") {
            if let Some(real) = resolve_uwp_child(hwnd) {
                return Some(real);
            }
        }
        Some(ProcessInfo { pid, name, display_name, path })
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
    let display_name = read_file_description(&path);
    *slot = Some(ProcessInfo { pid, name, display_name, path });
    BOOL::from(false) // 命中真实进程，停止枚举
}

/// 从绝对路径取文件名（兼容 `\` 与 `/` 的最后一段）。
fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// 读取 exe PE 资源里的 `FileDescription` 字段（Windows 任务管理器显示的友好名同源）。
/// 失败（无版本信息/字段缺失）返回 `None`。
fn read_file_description(exe_path: &str) -> Option<String> {
    let wide: Vec<u16> = exe_path.encode_utf16().chain(once(0u16)).collect();
    unsafe {
        let mut handle = 0u32;
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if !GetFileVersionInfoW(wide.as_ptr(), handle, size, buf.as_mut_ptr() as *mut c_void).as_bool() {
            return None;
        }
        // 1. 取 translation (lang_id, codepage_id)
        let trans_block: Vec<u16> = "\\VarFileInfo\\Translation"
            .encode_utf16()
            .chain(once(0u16))
            .collect();
        let mut trans_ptr: *mut c_void = std::ptr::null_mut();
        let mut trans_len: u32 = 0;
        if !VerQueryValueW(
            buf.as_ptr() as *const c_void,
            trans_block.as_ptr(),
            &mut trans_ptr,
            &mut trans_len,
        )
        .as_bool()
        {
            return None;
        }
        if trans_len < 4 {
            return None;
        }
        let trans = std::slice::from_raw_parts(trans_ptr as *const u16, 2);
        let lang = trans[0];
        let codepage = trans[1];

        // 2. 取 FileDescription（按 translation 拼路径）
        let sub = format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", lang, codepage);
        let sub_wide: Vec<u16> = sub.encode_utf16().chain(once(0u16)).collect();
        let mut desc_ptr: *mut c_void = std::ptr::null_mut();
        let mut desc_len: u32 = 0;
        if !VerQueryValueW(
            buf.as_ptr() as *const c_void,
            sub_wide.as_ptr(),
            &mut desc_ptr,
            &mut desc_len,
        )
        .as_bool()
        {
            return None;
        }
        if desc_len == 0 {
            return None;
        }
        let chars = std::slice::from_raw_parts(desc_ptr as *const u16, desc_len as usize);
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        if end == 0 {
            return None;
        }
        let mut s = String::from_utf16_lossy(&chars[..end]);
        // 部分应用 FileDescription 含尾部多余空白（如 "Google Chrome    "），裁掉。
        s = s.trim().to_owned();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
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
