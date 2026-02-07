use crate::rng::SimpleRng;
use crate::typing::{calculate_keystroke_delay, TypingConfig};
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
        let utf16_chars: Vec<u16> = ch.encode_utf16(&mut [0u16; 2]).to_vec();

        for &code_unit in &utf16_chars {
            let inputs = vec![
                create_key_input(code_unit, false, false), // Key down
                create_key_input(code_unit, true, false),  // Key up
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
/// Newlines are converted to VK_RETURN key events for proper multiline support.
///
/// Reference: https://github.com/keepassxreboot/keepassxc
pub fn send_unicode_keystrokes(
    hwnd: HWND,
    text: &str,
    config: &TypingConfig,
) -> Result<(), String> {
    activate_window(hwnd);

    let rng = SimpleRng::new();
    let total_chars = text.chars().count();

    // Process each character, converting newlines to Enter key events
    for (char_index, ch) in text.chars().enumerate() {
        if ch == '\n' || ch == '\r' {
            send_enter_key()?;
        } else {
            send_unicode_char(ch)?;
        }

        // Variable delay between characters based on typing configuration
        let delay = calculate_keystroke_delay(config, char_index, total_chars, &rng);
        std::thread::sleep(delay);
    }

    Ok(())
}
