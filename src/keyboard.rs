use crate::rng::SimpleRng;
use crate::typing::{calculate_keystroke_delay, TypingConfig};
use crate::window_manager::is_window_focused;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
    VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

/// Creates a keyboard input event for a key press
fn create_key_input(scan_code: u16, is_key_up: bool, is_virtual_key: bool) -> INPUT {
    let flags = if is_key_up {
        if is_virtual_key {
            KEYEVENTF_KEYUP
        } else {
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
        }
    } else if is_virtual_key {
        Default::default()
    } else {
        KEYEVENTF_UNICODE
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: if is_virtual_key {
                    VK_RETURN
                } else {
                    VIRTUAL_KEY(0)
                },
                wScan: scan_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Sends the Enter key as a VK_RETURN event
fn send_enter_key() -> Result<(), String> {
    unsafe {
        let inputs = vec![
            create_key_input(0, false, true), // Key down
            create_key_input(0, true, true),  // Key up
        ];

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != 2 {
            return Err(format!(
                "Failed to send Enter key: expected 2 events, sent {}",
                sent
            ));
        }
    }
    Ok(())
}

/// Sends a Unicode character using SendInput
fn send_unicode_char(ch: char) -> Result<(), String> {
    unsafe {
        let mut buf = [0u16; 2];
        let utf16_chars = ch.encode_utf16(&mut buf);

        for code_unit in utf16_chars {
            let inputs = vec![
                create_key_input(*code_unit, false, false), // Key down
                create_key_input(*code_unit, true, false),  // Key up
            ];

            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            if sent != 2 {
                return Err(format!(
                    "Failed to send character '{}': expected 2 events, sent {}",
                    ch, sent
                ));
            }
        }
    }
    Ok(())
}

/// Activates the target window and waits for it to be ready
fn activate_window(hwnd: HWND) {
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        // Small delay to let the window activation complete
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Sends Unicode keystrokes to a window using SendInput.
/// This method works with virtual machines and remote desktop applications
/// like Amazon Workspaces and Azure Virtual Desktop because it uses
/// KEYEVENTF_UNICODE which injects keystrokes at the lowest level.
///
/// The window is brought to the foreground before sending input.
/// Newlines are converted to VK_RETURN key events for proper multiline support.
///
/// The `continue_flag` parameter should be set to `true` to continue typing.
/// Setting it to `false` will cause the operation to stop early.
///
/// The `pause_flag` parameter is checked periodically. When `true`, typing pauses
/// until it becomes `false` again. Additionally, typing automatically pauses
/// if the target window loses focus.
///
/// Reference: https://github.com/keepassxreboot/keepassxc
pub fn send_unicode_keystrokes(
    initial_hwnd: HWND,
    text: &str,
    config: &Arc<Mutex<TypingConfig>>,
    continue_flag: &Arc<AtomicBool>,
    pause_flag: &Arc<AtomicBool>,
    target_hwnd: &Arc<Mutex<Option<isize>>>,
) -> Result<(), String> {
    let rng = SimpleRng::new();
    let total_chars = text.chars().count();

    // Process each character, converting newlines to Enter key events
    let mut chars = text.chars().peekable();
    let mut char_index = 0;

    while let Some(ch) = chars.next() {
        // Check if we should continue typing
        if !continue_flag.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Get the current target window (may have changed during pause)
        let hwnd = if let Ok(target) = target_hwnd.lock() {
            HWND(target.unwrap_or(initial_hwnd.0) as _)
        } else {
            initial_hwnd
        };

        // Check if we should pause (manually paused or focus lost)
        while pause_flag.load(Ordering::SeqCst) || !is_window_focused(hwnd) {
            // Check if we should stop while paused
            if !continue_flag.load(Ordering::SeqCst) {
                return Ok(());
            }

            // Sleep briefly before checking again
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Activate window before each keystroke (in case it changed)
        activate_window(hwnd);

        // Handle newlines based on current mode
        if ch == '\r' {
            // If we see \r, check if it's followed by \n
            if chars.peek() == Some(&'\n') {
                chars.next(); // Consume the \n
            }
            send_enter_key()?;
        } else if ch == '\n' {
            // Check if we should send \r\n (Windows mode) or just \n (Unix mode)
            // For sending, we always send Enter once regardless of mode
            // The mode affects how we interpret the input text
            send_enter_key()?;
        } else {
            send_unicode_char(ch)?;
        }

        // Variable delay between characters based on typing configuration
        // Read config dynamically to allow changes during typing
        let delay = if let Ok(config_guard) = config.lock() {
            calculate_keystroke_delay(&config_guard, char_index, total_chars, &rng)
        } else {
            // Fallback to default if lock fails
            std::time::Duration::from_millis(50)
        };
        std::thread::sleep(delay);

        char_index += 1;
    }

    Ok(())
}
