use std::ffi::{OsStr, OsString};
use std::iter::once;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::ptr::null_mut;
use std::process::Command;

use serde::{Serialize, Deserialize};

use winapi::shared::windef::{HWND, RECT, PPOINT, POINT};
use winapi::shared::minwindef::LPARAM;
use winapi::um::errhandlingapi::{GetLastError, SetLastError};
use winapi::um::winuser::{
    GetClassNameW, IsChild, ShowWindow, GetWindowRect, MapWindowPoints,
    MonitorFromPoint, GetMonitorInfoW, SetWindowPos,
    SW_SHOW, 
    MONITOR_DEFAULTTONEAREST,
    MONITORINFO
};

fn find_window_by_class(class: &str) -> HWND {
    use winapi::um::winuser::FindWindowW;
    unsafe { FindWindowW(to_wide(class).as_ptr(), null_mut()) }
}

fn find_window_by_name(name: &str) -> HWND {
    use winapi::um::winuser::FindWindowW;
    unsafe { FindWindowW(null_mut(), to_wide(name).as_ptr()) }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

pub fn get_window_name(hwnd: HWND) -> String {
    use winapi::um::winuser::{GetWindowTextLengthW, GetWindowTextW};

    if hwnd.is_null() {
        panic!("Invalid HWND");
    }

    let text = unsafe {
        let text_length = GetWindowTextLengthW(hwnd);
        let mut text: Vec<u16> = vec![0; text_length as usize + 1];
        GetWindowTextW(hwnd, text.as_mut_ptr(), text_length + 1);
        OsString::from_wide(&text[..text.iter().position(|&c| c == 0).unwrap()])
    };

    text.into_string().expect("Failed to convert string to UTF-8")
}

/**
 * Spawn a wallpaper window if it doesn't already exists and return handle to it.
 * 
 * `progman` - a valid handle to the `Progman`.
 * 
 * This function is unsafe, because user is responsible for providing valid progman handle.
 */
unsafe fn find_or_spawn_worker(progman: HWND) -> HWND {
    use winapi::um::winuser::{SendMessageW, EnumWindows};

    extern "system" fn find_worker(hwnd: HWND, data: LPARAM) -> i32 {
        use winapi::um::winuser::FindWindowExW;

        let data = data as *mut UserData;

        unsafe {
            if FindWindowExW(hwnd, null_mut(), (*data).shell_class.as_ptr(), null_mut()).is_null() {
                return 1;
            }
            
            let worker = FindWindowExW(null_mut(), hwnd, (*data).worker_class.as_ptr(), null_mut());
            if worker.is_null() {
                return 1;
            }

            (*data).worker = worker;
            (*data).parent = hwnd;    
        }

        return 0;
    }

    struct UserData {
        shell_class: Vec<u16>,
        worker_class: Vec<u16>,
        worker: HWND,
        parent: HWND,
    }
    
    let mut user_data = UserData {
        shell_class: to_wide("SHELLDLL_DefView"),
        worker_class: to_wide("WorkerW"),
        worker: null_mut(),
        parent: null_mut(),
    };

    SetLastError(0);
    EnumWindows(Some(find_worker), &mut user_data as *mut UserData as LPARAM);
    if GetLastError() != 0 {
        panic!("EnumWindows failed, GetLastError says: '{}'", GetLastError());
    }

    if user_data.worker.is_null() {
        // this is basically all the magic. it's an undocumented window message that
        // forces windows to spawn a window with class "WorkerW" behind deskicons
        SendMessageW(progman, 0x052C, 0xD, 0);
        SendMessageW(progman, 0x052C, 0xD, 1);
        
        SetLastError(0);
        EnumWindows(Some(find_worker), &mut user_data as *mut UserData as LPARAM);
        if GetLastError() != 0 {
            panic!("EnumWindows failed, GetLastError says: '{}'", GetLastError());
        }

        if user_data.worker.is_null() {
            eprintln!("W: couldn't spawn WorkerW window, trying old method");
    
            SendMessageW(progman, 0x052C, 0, 0);

            SetLastError(0);
            EnumWindows(Some(find_worker), &mut user_data as *mut UserData as LPARAM);
            if GetLastError() != 0 {
                panic!("EnumWindows failed, GetLastError says: '{}'", GetLastError());
            }
        }
    
    }

    user_data.worker
}

unsafe fn get_window_style(hwnd: HWND) -> (i32, i32) {
    use winapi::um::winuser::{GetWindowLongW, GWL_STYLE, GWL_EXSTYLE};

    SetLastError(0);
    let style = GetWindowLongW(hwnd, GWL_STYLE);
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);

    if (style == 0 || ex_style == 0) && GetLastError() != 0 {
        panic!("GetWindowLongW failed, GetLastError says: '{}'", GetLastError());
    }

    (style, ex_style)
}

unsafe fn update_window_styles(wnd: HWND, and: i32, ex_and: i32, or: i32, ex_or: i32) -> bool {
    use winapi::um::winuser::{SetWindowLongW, GWL_STYLE, GWL_EXSTYLE};

    let (mut style, mut ex_style) = get_window_style(wnd);

    style &= and;
    ex_style &= ex_and;
    style |= or;
    ex_style |= ex_or;

    SetLastError(0);
    let style = SetWindowLongW(wnd, GWL_STYLE, style);
    let ex_style = SetWindowLongW(wnd, GWL_EXSTYLE, ex_style);
    if (style == 0 || ex_style == 0) && GetLastError() != 0 {
        panic!("SetWindowLongW failed, GetLastError says: '{}'", GetLastError());
    }

    return true;
}

unsafe fn get_window_rect(wnd: HWND) -> Option<RECT> {
    let rect: RECT = Default::default();
    let failed = GetWindowRect(wnd, &rect as *const RECT as *mut RECT) == 0;
    if failed {
        eprintln!("GetWindowRect failed, GetLastError says: '{}'", GetLastError());
        return None;
    }
    return Some(rect);
}

unsafe fn map_window_rect(wallpaper: HWND, wnd: HWND) -> Option<RECT> {
    if let Some(rect) = get_window_rect(wnd) {
        MapWindowPoints(null_mut(), wallpaper, &rect as *const RECT as PPOINT, 2);
        return Some(rect);
    }
    return None;
}

unsafe fn move_window(wnd: HWND, rect: RECT) -> bool {
    let success = SetWindowPos(
        wnd, null_mut(), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, 0
    );
    if success == 0 {
        eprintln!("SetWindowPos failed, GetLastError says: '{}'", GetLastError());
        return false;
    }
    return true;
}

unsafe fn add_window_as_wallpaper(wallpaper: HWND, wnd: HWND) -> bool {
    use winapi::um::winuser::{
        SetParent,
        WS_CHILD, WS_CAPTION, WS_THICKFRAME, WS_SYSMENU, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
        WS_EX_DLGMODALFRAME, WS_EX_COMPOSITED, WS_EX_WINDOWEDGE, WS_EX_CLIENTEDGE, WS_EX_LAYERED, WS_EX_STATICEDGE, 
        WS_EX_TOOLWINDOW, WS_EX_APPWINDOW,
    };

    let wnd_class = {
        let wnd_class: &mut [u16] = &mut [0; 512];
        GetClassNameW(wnd, wnd_class.as_mut_ptr(), wnd_class.len() as i32 - 1);
        OsString::from_wide(&wnd_class[..wnd_class.iter().position(|&c| c == 0).unwrap()])
    };

    if wallpaper == wnd || wnd_class == "Shell_TrayWnd" {
        eprintln!("can't add this window");
        return false;
    }

    let is_child = IsChild(wallpaper, wnd) != 0;
    if is_child {
        eprintln!("already added");
        return false;
    }

    /*
     * styles blacklist taken from https://github.com/Codeusa/Borderless-Gaming/
     * blob/2fef4ccc121412f215cd7f185c4351fd634cab8b/BorderlessGaming.Logic/
     * Windows/Manipulation.cs#L70
     */

    /* TODO: somehow save old styles so we can restore them */

    let and: i32 = !(
        WS_CAPTION |
        WS_THICKFRAME |
        WS_SYSMENU |
        WS_MAXIMIZEBOX |
        WS_MINIMIZEBOX
    ) as i32;

    let ex_and: i32 = !(
        WS_EX_DLGMODALFRAME |
        WS_EX_COMPOSITED |
        WS_EX_WINDOWEDGE |
        WS_EX_CLIENTEDGE |
        WS_EX_LAYERED |
        WS_EX_STATICEDGE |
        WS_EX_TOOLWINDOW |
        WS_EX_APPWINDOW
    ) as i32;

    if !update_window_styles(wnd, and, ex_and, WS_CHILD as i32, 0) {
        return false;
    }

    /* window retains screen coordinates so we need to adjust them */
    map_window_rect(wallpaper, wnd).unwrap(); 

    let prev_parent = SetParent(wnd, wallpaper);
    if prev_parent.is_null() {
        panic!("SetParent failed, GetLastError says: '{}'", GetLastError());
    }
    ShowWindow(wnd, SW_SHOW);

    return true;
}

unsafe fn remove_window_from_wallpaper(wallpaper: HWND, wnd: HWND) -> bool {
    use winapi::um::winuser::{
        SetParent, GetDesktopWindow, InvalidateRect,
        WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, 
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_NOOWNERZORDER
    };

    if SetParent(wnd, GetDesktopWindow()).is_null() {
        eprintln!("SetParent failed, GetLastError says: '{}'", GetLastError());
        return false;
    }

    let or = WS_OVERLAPPEDWINDOW as i32;
    let ex_or = WS_EX_APPWINDOW as i32;

    if !update_window_styles(wnd, -1, -1, or, ex_or) {
        return false;
    }

    SetWindowPos(
        wnd, null_mut(), 0, 0, 0, 0, 
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER
    );

    InvalidateRect(wallpaper, null_mut(), 1);
    // wp_id(); /* can sometimes fix leftover unrefreshed portions */

    true
}

unsafe fn set_fullscreen(wallpaper: HWND, wnd: HWND) -> bool {
    if let Some(current_rect) = get_window_rect(wnd) {
        let monitor = MonitorFromPoint(POINT {x: current_rect.left, y: current_rect.top}, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            eprintln!("MonitorFromWindow failed, GetLastError says: '{}'", GetLastError());
            return false;
        }

        let mut mi: MONITORINFO = Default::default();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let success = GetMonitorInfoW(monitor, &mi as *const MONITORINFO as *mut MONITORINFO);
        if success == 0 {
            eprintln!("GetMonitorInfoW failed, GetLastError says: '{}'", GetLastError());
            return false;
        }
    
        MapWindowPoints(null_mut(), wallpaper, &mi.rcMonitor as *const RECT as PPOINT, 2);

        move_window(wnd, mi.rcMonitor);
        
        return true;
    }

    return false;
}

unsafe fn list_immediate_children(parent: HWND) -> Vec<HWND> {
    use winapi::um::winuser::EnumChildWindows;

    #[repr(C)]
    struct WindowState {
        parent: HWND,
        handles: Vec<HWND>,
    }
    
    let mut s = WindowState { parent, handles: Vec::new() };

    extern "system" fn enum_windows(wnd: HWND, lp: LPARAM) -> i32 {
        use winapi::um::winuser::{GetAncestor, GA_PARENT};

        let s: *mut WindowState = lp as *mut WindowState;
        
        unsafe {
            if GetAncestor(wnd, GA_PARENT) == (*s).parent {
                (*s).handles.push(wnd);
            }
        }
        
        return 1;
    }

    SetLastError(0);
    EnumChildWindows(parent, Some(enum_windows), &mut s as *mut WindowState as LPARAM);
    if GetLastError() != 0 {
        panic!("EnumChildWindows failed, GetLastError says: {}", GetLastError());
    }

    s.handles.sort_unstable();

    return s.handles;
}

unsafe fn find_window_by_pid(pid: u32) -> HWND {
    use winapi::um::winuser::{EnumWindows, GetWindowThreadProcessId};
    use winapi::shared::minwindef::{DWORD, LPDWORD};

    #[repr(C)]
    #[derive(Debug)]
    struct Data {
        handle: HWND,
        pid: u32,
    }

    extern "system" fn enum_windows(wnd: HWND, data: LPARAM) -> i32 {
        let mut data = data as *mut Data;

        unsafe {
            let mut this_pid: DWORD = 0;

            GetWindowThreadProcessId(wnd, &mut this_pid as LPDWORD);

            if this_pid == (*data).pid {
                (*data).handle = wnd;
                return 0;
            }
        }

        return 1;
    }

    let mut data = Data {handle: null_mut(), pid};
    
    SetLastError(0);
    EnumWindows(Some(enum_windows), &mut data as *mut Data as LPARAM);
    if GetLastError() != 0 {
        panic!("EnumWindows failed, GetLastError says: {}", GetLastError());
    }
    
    data.handle
}

pub fn list_windows() -> Vec<HWND> {
    use winapi::um::winuser::{
        EnumWindows, IsWindowVisible, GetLastActivePopup, GetAncestor, GetWindowTextLengthW, 
        GA_ROOTOWNER, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW
    };

    // https://stackoverflow.com/questions/210504/enumerate-windows-like-alt-tab-does
    unsafe fn should_list(hwnd: HWND) -> bool {
        // Start at the root owner
        let mut hwnd_walk = GetAncestor(hwnd, GA_ROOTOWNER);
        // See if we are the last active visible popup
        let mut hwnd_try = null_mut();
        loop {
            let hwnd_try_next = GetLastActivePopup(hwnd_walk);
            if hwnd_try_next == hwnd_try || IsWindowVisible(hwnd_try_next) == 1 {
                break;
            }
            hwnd_try = hwnd_try_next;
            hwnd_walk = hwnd_try;
        }

        return hwnd_walk == hwnd;
    }

    extern "system" fn list_windows_callback(hwnd: HWND, lp: LPARAM) -> i32 {
        let data = lp as *mut Vec<HWND>;

        unsafe {
            if  IsWindowVisible(hwnd) == 1 && GetWindowTextLengthW(hwnd) > 0 && should_list(hwnd) {
                let (_, ex_style) = get_window_style(hwnd);
                if (ex_style as u32 & WS_EX_NOREDIRECTIONBITMAP) == 0 && (ex_style as u32 & WS_EX_TOOLWINDOW) == 0 {
                    (*data).push(hwnd);
                }
            }
        }

        1
    }

    let mut data: Vec<HWND> = Vec::new();

    unsafe {
        SetLastError(0);
        EnumWindows(Some(list_windows_callback), &mut data as *mut Vec<HWND> as LPARAM);
        if GetLastError() != 0 {
            panic!("EnumWindows failed, GetLastError says: '{}'", GetLastError());
        }
    }

    data
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowSelector<'a> {
    WindowTitle(&'a str),
    None,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperProperties {
    pub fullscreen: bool
}

#[derive(Debug)]
pub enum EngineError {
    ProgmanNotFound,
    UnableToSpawnWorker,
}

#[derive(Debug)]
pub struct Engine {
    progman: HWND,
    worker: HWND,
}

impl Engine {

    pub fn new() -> Result<Engine, EngineError> {
        let progman_handle = find_window_by_class("Progman");
        if progman_handle.is_null() {
            return Err(EngineError::ProgmanNotFound);
        }

        let worker_handle = unsafe { find_or_spawn_worker(progman_handle) };
        if worker_handle.is_null() {
            return Err(EngineError::UnableToSpawnWorker);
        }
        
        Ok(Engine {progman: progman_handle, worker: worker_handle})
    }

    pub fn list_active(&self) -> Vec<HWND> {
        unsafe {
            // TODO this is not safe until we add a check for worker validity here.
            list_immediate_children(self.worker)
        }
    }

    pub fn add_window_by_handle(&self, handle: HWND, properties: WallpaperProperties) -> bool {
        if !unsafe { add_window_as_wallpaper(self.worker, handle) } {
            eprintln!("Cannot add window to wallpaper");
            return false;
        }

        if properties.fullscreen && !unsafe { set_fullscreen(self.worker, handle) } {
            return false
        }

        true
    }
    
    pub fn add_window(&self, 
        command: Option<&mut Command>, selector: WindowSelector, properties: WallpaperProperties, 
        wait_for: u64, attempts: u64
    ) -> bool {

        let process_id = match command {
            Some(command) => command.spawn().expect("command failed to start").id(),
            None => {
                if let WindowSelector::None = selector {
                    eprintln!("One or both of selector and command should be specified");
                    return false;
                }
                0
            }
        };

        let mut handle = null_mut();
        for _attempt in 1..=attempts {
            handle = match selector {
                WindowSelector::None => unsafe { find_window_by_pid(process_id) },
                WindowSelector::WindowTitle(title) => {
                    let windows = list_windows();
                    *windows.iter().find(|&&hwnd| get_window_name(hwnd) == title).unwrap_or(&null_mut())
                },
            };
            
            if handle.is_null() {
                std::thread::sleep(std::time::Duration::from_millis(wait_for));
            } else {
                break;
            }
        }

        if handle.is_null() {
            eprintln!("Cannot find handle using selector: {:?}", selector);
            return false;
        }
        
        self.add_window_by_handle(handle, properties)
    }

    pub fn close_all(self) {
        use winapi::um::winuser::{InvalidateRect, SendMessageW, WM_CLOSE};
        unsafe { 
            for hwnd in self.list_active().into_iter() {
                remove_window_from_wallpaper(self.worker, hwnd);
                std::thread::sleep(std::time::Duration::from_millis(32));
                SendMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            InvalidateRect(null_mut(), null_mut(), 1);
        }
    }

}
