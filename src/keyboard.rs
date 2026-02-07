use crate::rng::SimpleRng;
use crate::typing::{calculate_keystroke_delay, TypingConfig};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

/// Sends Unicode keystrokes to a window using SendInput.
/// This method works with virtual machines and remote desktop applications
/// like Amazon Workspaces and Azure Virtual Desktop because it uses
/// KEYEVENTF_UNICODE which injects keystrokes at the lowest level.
///
/// Newlines are converted to VK_RETURN key events for proper multiline support.
///
/// Reference: https://github.com/keepassxreboot/keepassxc
pub fn send_unicode_keystrokes(hwnd: HWND, text: &str, config: &TypingConfig) -> Result<(), String> {
    unsafe {
        // Bring the target window to the foreground
        let _ = SetForegroundWindow(hwnd);

        // Small delay to let the window activation complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        let rng = SimpleRng::new();
        let total_chars = text.chars().count();

        // Process each character, converting newlines to Enter key events
        for (char_index, ch) in text.chars().enumerate() {
            if ch == '\n' || ch == '\r' {
                // Send Enter key (VK_RETURN) for newlines
                let mut inputs = Vec::new();

                // Key down event for Enter
                let input_down = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_RETURN,
                            wScan: 0,
                            dwFlags: Default::default(),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                inputs.push(input_down);

                // Key up event for Enter
                let input_up = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_RETURN,
                            wScan: 0,
                            dwFlags: KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                inputs.push(input_up);

                // Send the input events and check for errors
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent != 2 {
                    return Err(format!(
                        "Failed to send Enter key: expected 2 events, sent {}",
                        sent
                    ));
                }
            } else {
                // Send as Unicode for all other characters
                let utf16_chars: Vec<u16> = ch.encode_utf16(&mut [0u16; 2]).to_vec();
                
                for &code_unit in &utf16_chars {
                    let mut inputs = Vec::new();

                    // Key down event
                    let input_down = INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: VIRTUAL_KEY(0),
                                wScan: code_unit,
                                dwFlags: KEYEVENTF_UNICODE,
                                time: 0,
                                dwExtraInfo: 0,
                            },
                        },
                    };
                    inputs.push(input_down);

                    // Key up event
                    let input_up = INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: VIRTUAL_KEY(0),
                                wScan: code_unit,
                                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                                time: 0,
                                dwExtraInfo: 0,
                            },
                        },
                    };
                    inputs.push(input_up);

                    // Send the input events and check for errors
                    let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                    if sent != 2 {
                        return Err(format!(
                            "Failed to send character '{}': expected 2 events, sent {}",
                            ch, sent
                        ));
                    }
                }
            }

            // Variable delay between characters based on typing configuration
            let delay = calculate_keystroke_delay(config, char_index, total_chars, &rng);
            std::thread::sleep(delay);
        }
    }

    Ok(())
}
