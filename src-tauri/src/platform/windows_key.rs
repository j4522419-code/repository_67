//! Opening Grandium with the Windows key on its own.
//!
//! Windows has no API for claiming a lone modifier key, so this uses a
//! low-level keyboard hook, like other launchers do. The hook only tracks
//! two things: whether the Windows key is down, and whether any other key
//! was pressed while it was. It doesn't record or keep which keys those
//! were. A Windows-key press with nothing else in between counts as a tap.
//! Shortcuts like Win+E are left alone.
//!
//! To stop the Start menu from also opening on a tap, the hook sends an
//! unassigned key code (0xE8, the usual "mask" key) while the Windows key
//! is down, so Windows treats the press as a shortcut. Nothing is ever
//! blocked, so no key can get stuck.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_LWIN, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use super::message;

/// An unassigned virtual key code; sending it has no effect in any app.
const MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
/// Tags the mask key events we send, so the hook ignores them.
const OURS: usize = 0x6772_616E; // "gran"

static WIN_DOWN: AtomicBool = AtomicBool::new(false);
static OTHER_KEY_PRESSED: AtomicBool = AtomicBool::new(false);
static ON_TAP: Mutex<Option<Sender<()>>> = Mutex::new(None);

/// A running listener; `stop` removes the hook.
pub struct WindowsKeyListener {
    thread_id: u32,
}

/// Starts listening. Every tap of the Windows key sends `()` to `on_tap`.
pub fn start(on_tap: Sender<()>) -> Result<WindowsKeyListener, String> {
    *ON_TAP.lock().unwrap() = Some(on_tap);
    let (ready, started) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), module, 0) {
            Ok(hook) => hook,
            Err(error) => {
                let _ = ready.send(Err(message(error)));
                return;
            }
        };
        let _ = ready.send(Ok(GetCurrentThreadId()));
        // The hook runs on this thread, which must keep handling messages
        // until asked to quit.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
        let _ = UnhookWindowsHookEx(hook);
        WIN_DOWN.store(false, Ordering::SeqCst);
    });
    let thread_id = started
        .recv()
        .map_err(|_| "The Windows key listener didn't start.".to_string())??;
    Ok(WindowsKeyListener { thread_id })
}

impl WindowsKeyListener {
    pub fn stop(self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if event.dwExtraInfo != OURS {
            let windows_key =
                event.vkCode == u32::from(VK_LWIN.0) || event.vkCode == u32::from(VK_RWIN.0);
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            match (windows_key, down) {
                (true, true) => {
                    // Holding the key repeats this event; act on the first.
                    if !WIN_DOWN.swap(true, Ordering::SeqCst) {
                        OTHER_KEY_PRESSED.store(false, Ordering::SeqCst);
                        send_mask_key();
                    }
                }
                (true, false) => {
                    let was_down = WIN_DOWN.swap(false, Ordering::SeqCst);
                    if was_down && !OTHER_KEY_PRESSED.load(Ordering::SeqCst) {
                        if let Some(on_tap) = ON_TAP.lock().unwrap().as_ref() {
                            let _ = on_tap.send(());
                        }
                    }
                }
                (false, true) => {
                    if WIN_DOWN.load(Ordering::SeqCst) {
                        OTHER_KEY_PRESSED.store(true, Ordering::SeqCst);
                    }
                }
                (false, false) => {}
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe fn send_mask_key() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: MASK_KEY,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: OURS,
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    SendInput(&inputs, size_of::<INPUT>() as i32);
}
