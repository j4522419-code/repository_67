//! Opening Grandium with the Windows key on its own, instead of Start.
//!
//! Windows has no API for claiming a lone modifier key, so this uses a
//! low-level keyboard hook, like other launchers and AutoHotkey do. It
//! treats the Windows key as a "prefix":
//!
//! - When the Windows key goes down, the hook holds it back from Windows.
//! - If it comes back up with nothing pressed in between, that's a tap:
//!   the release is held back too, so Windows never saw the key at all and
//!   can't open Start. Grandium opens instead.
//! - If another key is pressed first, it's a shortcut like Win+E: the hook
//!   hands Windows the Windows key after all, followed by that key, and
//!   everything from then on passes through untouched. Win+L is the
//!   exception: Windows won't lock for a replayed Win+L, so the hook locks
//!   the PC itself.
//!
//! The hook only follows the Windows key; other keys it passes on without
//! recording them. (Masking the release with a dummy key, the lighter
//! approach, doesn't keep Start closed on Windows 11.)

use std::cell::Cell;
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, VIRTUAL_KEY, VK_LWIN, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT,
    WM_SYSKEYDOWN,
};

use super::message;

/// Tags the key events we send, so the hook lets them through.
const OURS: usize = 0x6772_616E; // "gran"

static ON_TAP: Mutex<Option<Sender<()>>> = Mutex::new(None);

#[derive(Clone, Copy)]
enum State {
    Idle,
    /// The Windows key is down and Windows doesn't know yet.
    Held {
        key: u32,
        scan: u32,
    },
    /// The Windows key is part of a shortcut; Windows knows it's down.
    Shortcut,
    /// The Windows key was used for Win+L, which Windows never saw; `at` is
    /// when the hook last saw the key.
    Locked {
        at: u32,
    },
}

/// Holding a key repeats it at least once a second, so a press that comes
/// longer than this after the last one is new.
const NEW_PRESS_MS: u32 = 1500;

thread_local! {
    // The hook always runs on the thread that installed it.
    static STATE: Cell<State> = const { Cell::new(State::Idle) };
}

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
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if event.dwExtraInfo != OURS && hold_back(event, down) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Follows the Windows key; returns whether to keep this event from
/// Windows.
fn hold_back(event: &KBDLLHOOKSTRUCT, down: bool) -> bool {
    let windows_key = event.vkCode == u32::from(VK_LWIN.0) || event.vkCode == u32::from(VK_RWIN.0);
    STATE.with(|state| match (state.get(), windows_key, down) {
        // Wait and see whether this is a tap or a shortcut.
        (State::Idle, true, true) => {
            state.set(State::Held {
                key: event.vkCode,
                scan: event.scanCode,
            });
            true
        }
        // Holding the key down repeats it.
        (State::Held { .. }, true, true) => true,
        // A tap. Windows never saw the key, so Start stays closed.
        (State::Held { .. }, true, false) => {
            state.set(State::Idle);
            if let Some(on_tap) = ON_TAP.lock().unwrap().as_ref() {
                let _ = on_tap.send(());
            }
            true
        }
        (State::Held { .. }, false, true) if event.vkCode == u32::from(b'L') => {
            state.set(State::Locked { at: event.time });
            let _ = unsafe { LockWorkStation() };
            true
        }
        // A shortcut: replay the Windows key, then this key. Only if that
        // worked is the original held back; otherwise it goes through as is.
        (State::Held { key, scan }, false, true) => {
            state.set(State::Shortcut);
            unsafe { replay(key, scan, event) }
        }
        (State::Shortcut, true, false) => {
            state.set(State::Idle);
            false
        }
        // The release after Win+L can happen on the lock screen, out of the
        // hook's sight; then the next press is a new one.
        (State::Locked { at }, true, true) => {
            state.set(if event.time.wrapping_sub(at) > NEW_PRESS_MS {
                State::Held {
                    key: event.vkCode,
                    scan: event.scanCode,
                }
            } else {
                State::Locked { at: event.time }
            });
            true
        }
        (State::Locked { .. }, true, false) => {
            state.set(State::Idle);
            true
        }
        _ => false,
    })
}

/// Sends the held-back Windows key press, then the key pressed with it.
/// Returns whether both were accepted.
unsafe fn replay(windows_key: u32, windows_scan: u32, pressed: &KBDLLHOOKSTRUCT) -> bool {
    let key = |key: u32, scan: u32, extended: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key as u16),
                wScan: scan as u16,
                dwFlags: if extended {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: OURS,
            },
        },
    };
    let inputs = [
        key(windows_key, windows_scan, true),
        key(
            pressed.vkCode,
            pressed.scanCode,
            pressed.flags.contains(LLKHF_EXTENDED),
        ),
    ];
    SendInput(&inputs, size_of::<INPUT>() as i32) as usize == inputs.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_A;

    fn key(key: VIRTUAL_KEY, time: u32) -> KBDLLHOOKSTRUCT {
        KBDLLHOOKSTRUCT {
            vkCode: u32::from(key.0),
            time,
            ..Default::default()
        }
    }

    /// Replaying shortcuts and locking need a real desktop, so this covers
    /// taps: Windows never sees the key, and Grandium hears about it once.
    #[test]
    fn taps_are_kept_from_windows() {
        let (on_tap, taps) = mpsc::channel();
        *ON_TAP.lock().unwrap() = Some(on_tap);

        // A release Windows already knows about goes through.
        assert!(!hold_back(&key(VK_LWIN, 0), false));
        // Typing goes through.
        assert!(!hold_back(&key(VK_A, 10), true));
        assert!(!hold_back(&key(VK_A, 20), false));

        // Press, hold (the key repeats), release.
        assert!(hold_back(&key(VK_LWIN, 100), true));
        assert!(hold_back(&key(VK_LWIN, 600), true));
        assert!(taps.try_recv().is_err(), "opened before the release");
        assert!(hold_back(&key(VK_LWIN, 650), false));
        assert!(taps.try_recv().is_ok(), "the tap wasn't reported");

        // The right Windows key too, and afterwards typing is untouched.
        assert!(hold_back(&key(VK_RWIN, 700), true));
        assert!(hold_back(&key(VK_RWIN, 750), false));
        assert!(taps.try_recv().is_ok());
        assert!(!hold_back(&key(VK_A, 800), true));
    }
}
