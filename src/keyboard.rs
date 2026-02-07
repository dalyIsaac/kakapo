use crate::rng::SimpleRng;
use crate::typing::{calculate_keystroke_delay, TypingConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CHAR, WM_KEYDOWN, WM_KEYUP};

/// Sends the Enter key to a specific window using window messages
fn send_enter_key_to_window(hwnd: HWND) -> Result<(), String> {
    unsafe {
        // Send VK_RETURN key down and up
        SendMessageW(hwnd, WM_KEYDOWN, WPARAM(VK_RETURN.0 as usize), LPARAM(0));
        SendMessageW(hwnd, WM_KEYUP, WPARAM(VK_RETURN.0 as usize), LPARAM(0));
    }
    Ok(())
}

/// Sends a Unicode character to a specific window using WM_CHAR message
fn send_unicode_char_to_window(hwnd: HWND, ch: char) -> Result<(), String> {
    unsafe {
        let mut buf = [0u16; 2];
        let utf16_chars = ch.encode_utf16(&mut buf);
        
        for code_unit in utf16_chars {
            SendMessageW(hwnd, WM_CHAR, WPARAM(*code_unit as usize), LPARAM(0));
        }
    }
    Ok(())
}

/// Sends Unicode keystrokes to a window using window messages.
/// This method sends input directly to the target window without requiring
/// it to be focused or brought to the foreground.
///
/// Newlines are converted to VK_RETURN key events for proper multiline support.
/// 
/// The `continue_flag` parameter should be set to `true` to continue typing.
/// Setting it to `false` will cause the operation to stop early.
pub fn send_unicode_keystrokes(
    hwnd: HWND,
    text: &str,
    config: &TypingConfig,
    continue_flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    let rng = SimpleRng::new();
    let total_chars = text.chars().count();

    // Process each character, converting newlines to Enter key events
    for (char_index, ch) in text.chars().enumerate() {
        // Check if we should continue typing
        if !continue_flag.load(Ordering::SeqCst) {
            return Ok(());
        }
        
        if ch == '\n' || ch == '\r' {
            send_enter_key_to_window(hwnd)?;
        } else {
            send_unicode_char_to_window(hwnd, ch)?;
        }

        // Variable delay between characters based on typing configuration
        let delay = calculate_keystroke_delay(config, char_index, total_chars, &rng);
        std::thread::sleep(delay);
    }

    Ok(())
}
