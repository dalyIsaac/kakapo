use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Entity, FocusHandle,
    Focusable, Window, WindowBounds, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    v_flex, Disableable, Root,
};
use gpui_component_assets::Assets;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, IsWindowVisible, SetForegroundWindow,
};

// Use a simple random number generator for jitter
use std::cell::Cell;

// Constants for typing configuration
const DEFAULT_CHARS_PER_MINUTE: f64 = 300.0; // ~60 WPM (assuming 5 chars per word)
const MAX_CHARS_PER_MINUTE: f64 = 10000.0;
const MIN_DELAY_MS: f64 = 5.0;
const MIN_SPURT_SIZE: usize = 5;
const MAX_SPURT_SIZE: usize = 15;

/// A simple linear congruential generator for pseudo-random numbers
struct SimpleRng {
    state: Cell<u64>,
}

impl SimpleRng {
    fn new() -> Self {
        // Seed with current time
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self {
            state: Cell::new(seed),
        }
    }

    /// Generate a random f64 between 0.0 and 1.0
    fn next_f64(&self) -> f64 {
        let state = self.state.get();
        let next = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state.set(next);
        (next >> 32) as f64 / u32::MAX as f64
    }
}

/// Configuration for typing variability
#[derive(Clone, Debug)]
struct TypingConfig {
    /// Characters per minute
    chars_per_minute: f64,
    /// Whether to enable jitter
    enable_jitter: bool,
}

impl Default for TypingConfig {
    fn default() -> Self {
        Self {
            chars_per_minute: DEFAULT_CHARS_PER_MINUTE,
            enable_jitter: true,
        }
    }
}

#[derive(Clone, Debug)]
struct WindowInfo {
    title: String,
    hwnd: isize,
}

fn get_system_windows() -> Vec<WindowInfo> {
    let windows = Arc::new(Mutex::new(Vec::new()));

    unsafe {
        let windows_ptr = Arc::as_ptr(&windows);
        let _ = EnumWindows(Some(enum_windows_callback), LPARAM(windows_ptr as isize));
    }

    Arc::try_unwrap(windows).unwrap().into_inner().unwrap()
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows_ptr = lparam.0 as *const Mutex<Vec<WindowInfo>>;
    let windows = unsafe { &*windows_ptr };

    // Only include visible windows
    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        let mut title = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut title) };

        if len > 0 {
            let title_str = String::from_utf16_lossy(&title[..len as usize]);
            // Filter out empty titles and some system windows
            if !title_str.is_empty() {
                windows.lock().unwrap().push(WindowInfo {
                    title: title_str,
                    hwnd: hwnd.0,
                });
            }
        }
    }

    true.into()
}

/// Calculate delay between keystrokes based on typing configuration
/// Implements "spurt" pattern where typing speed varies in bursts
fn calculate_keystroke_delay(
    config: &TypingConfig,
    char_index: usize,
    _total_chars: usize,
    rng: &SimpleRng,
) -> Duration {
    // Base delay from characters per minute
    let base_delay_ms = 60_000.0 / config.chars_per_minute;

    if !config.enable_jitter {
        return Duration::from_millis(base_delay_ms as u64);
    }

    // Create "spurts" - alternating periods of faster and slower typing
    // Each spurt is roughly 5-15 characters
    // Use deterministic spurt size based on spurt number to keep consistent spurt lengths
    let spurt_number = char_index / MIN_SPURT_SIZE;
    let spurt_size = MIN_SPURT_SIZE + ((spurt_number as f64 * 0.618033988749895) % (MAX_SPURT_SIZE - MIN_SPURT_SIZE) as f64) as usize;
    
    // Alternate between fast and slow spurts
    let spurt_multiplier = if (char_index / spurt_size) % 2 == 0 {
        0.7 + rng.next_f64() * 0.3 // Fast spurt: 70-100% of base speed
    } else {
        1.2 + rng.next_f64() * 0.6 // Slow spurt: 120-180% of base speed
    };

    // Add random jitter within the current spurt
    let jitter = 0.8 + rng.next_f64() * 0.4; // 80-120% variation

    // Apply both spurt pattern and jitter
    let final_delay_ms = base_delay_ms * spurt_multiplier * jitter;
    
    // Ensure minimum delay for reliability
    let final_delay_ms = final_delay_ms.max(MIN_DELAY_MS);

    Duration::from_millis(final_delay_ms as u64)
}

/// Sends Unicode keystrokes to a window using SendInput.
/// This method works with virtual machines and remote desktop applications
/// like Amazon Workspaces and Azure Virtual Desktop because it uses
/// KEYEVENTF_UNICODE which injects keystrokes at the lowest level.
///
/// Newlines are converted to VK_RETURN key events for proper multiline support.
///
/// Reference: https://github.com/keepassxreboot/keepassxc
fn send_unicode_keystrokes(hwnd: HWND, text: &str, config: &TypingConfig) -> Result<(), String> {
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

struct WindowList {
    selected_window: Option<WindowInfo>,
    input_state: Entity<InputState>,
    focus_handle: FocusHandle,
    cached_windows: Vec<WindowInfo>,
    last_refresh: Instant,
    typing_config: TypingConfig,
    chars_per_minute_input: Entity<InputState>,
}

impl WindowList {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx)
            .placeholder("Type text to send...")
            .multi_line(true));

        let typing_config = TypingConfig::default();
        let chars_per_minute_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("300")
                .default_value(typing_config.chars_per_minute.to_string())
        });

        Self {
            selected_window: None,
            input_state,
            focus_handle: cx.focus_handle(),
            cached_windows: get_system_windows(),
            last_refresh: Instant::now(),
            typing_config,
            chars_per_minute_input,
        }
    }

    fn refresh_windows_if_needed(&mut self, cx: &mut Context<Self>) {
        // Refresh window list if it's been more than 1 second since last refresh
        // This will catch window focus changes
        if self.last_refresh.elapsed() > Duration::from_secs(1) {
            self.cached_windows = get_system_windows();
            self.last_refresh = Instant::now();
            cx.notify();
        }
    }

    fn select_window(&mut self, window_info: WindowInfo, cx: &mut Context<Self>) {
        self.selected_window = Some(window_info);
        cx.notify();
    }

    fn send_keystrokes(&mut self, cx: &mut Context<Self>) {
        if let Some(ref window) = self.selected_window {
            let text = self.input_state.read(cx).value();
            if !text.is_empty() {
                let hwnd = window.hwnd;
                let config = self.typing_config.clone();
                // Spawn background thread to avoid blocking UI
                std::thread::spawn(move || {
                    if let Err(e) = send_unicode_keystrokes(HWND(hwnd as _), &text, &config) {
                        eprintln!("Error sending keystrokes: {}", e);
                    }
                });
            }
        }
    }

    fn handle_send_click(
        &mut self,
        _event: &gpui::ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Update typing speed from input before sending
        self.update_typing_speed(cx);
        self.send_keystrokes(cx);
    }

    fn update_typing_speed(&mut self, cx: &mut Context<Self>) {
        let value = self.chars_per_minute_input.read(cx).value();
        if let Ok(chars_per_minute) = value.parse::<f64>() {
            if chars_per_minute > 0.0 && chars_per_minute <= MAX_CHARS_PER_MINUTE {
                self.typing_config.chars_per_minute = chars_per_minute;
                cx.notify();
            }
        }
    }

    fn toggle_jitter(&mut self, cx: &mut Context<Self>) {
        self.typing_config.enable_jitter = !self.typing_config.enable_jitter;
        cx.notify();
    }

    fn get_invalid_chars(&self, cx: &App) -> Vec<char> {
        let _text = self.input_state.read(cx).value();
        // For now, all characters can be sent via Unicode SendInput
        // In the future, we might add validation logic here
        vec![]
    }
}

impl Render for WindowList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Refresh window list if needed (detects window focus changes)
        self.refresh_windows_if_needed(cx);
        
        let windows = &self.cached_windows;
        let selected_hwnd = self.selected_window.as_ref().map(|w| w.hwnd);
        let has_selection = self.selected_window.is_some();
        let selected_title = self.selected_window.as_ref().map(|w| w.title.clone());
        let invalid_chars = self.get_invalid_chars(cx);
        let text = self.input_state.read(cx).value();
        let text_empty = text.is_empty();
        let jitter_enabled = self.typing_config.enable_jitter;

        v_flex()
            .gap_3()
            .bg(rgb(0x2d2d2d))
            .size_full()
            .p_4()
            // Input section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(0x3d3d3d))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x505050))
                    .child(
                        div()
                            .text_lg()
                            .text_color(rgb(0xffffff))
                            .mb_2()
                            .child("Send Keystrokes"),
                    )
                    // Typing configuration controls
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .mb_2()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xcccccc))
                                    .child("Typing Speed (chars/min):"),
                            )
                            .child(
                                div()
                                    .w(px(100.))
                                    .child(Input::new(&self.chars_per_minute_input)),
                            )
                            .child(
                                Button::new("toggle_jitter")
                                    .label(if jitter_enabled { "✓ Jitter" } else { "Jitter" })
                                    .on_click(cx.listener(|view, _event, _window, cx| {
                                        view.toggle_jitter(cx);
                                    })),
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .child(Input::new(&self.input_state).h(px(100.))),
                            )
                            .child(
                                Button::new("send")
                                    .primary()
                                    .label("Send")
                                    .disabled(!has_selection || text_empty)
                                    .on_click(cx.listener(Self::handle_send_click)),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(if has_selection {
                                rgb(0x66cc66)
                            } else {
                                rgb(0xcc6666)
                            })
                            .child(if let Some(title) = selected_title.clone() {
                                format!("✓ Selected: {}", title)
                            } else {
                                "✗ No window selected. Click a window below to select it.".to_string()
                            }),
                    )
                    .when(!invalid_chars.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xff6666))
                                .child(format!(
                                    "⚠ Invalid characters that cannot be sent: {:?}",
                                    invalid_chars
                                )),
                        )
                    })
                    .when(invalid_chars.is_empty() && !text_empty, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x66cc66))
                                .child("✓ All characters can be sent"),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(0x3d3d3d))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x505050))
                    .flex_grow()
                    .flex_shrink()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_lg()
                            .text_color(rgb(0xffffff))
                            .mb_2()
                            .flex_shrink_0()
                            .child("Window List:"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .id("window_list")
                            .overflow_y_scroll()
                            .flex_grow()
                            .flex_shrink()
                            .min_h(px(0.))
                            .children(windows.iter().enumerate().map(|(i, window_info)| {
                                let is_selected = selected_hwnd == Some(window_info.hwnd);
                                let window_clone = window_info.clone();
                                div()
                                    .flex()
                                    .gap_2()
                                    .p_2()
                                    .rounded_md()
                                    .bg(if is_selected {
                                        rgb(0x0066cc)
                                    } else {
                                        rgb(0x353535)
                                    })
                                    .border_1()
                                    .border_color(if is_selected {
                                        rgb(0x0080ff)
                                    } else {
                                        rgb(0x454545)
                                    })
                                    .hover(|style| {
                                        style.bg(if is_selected {
                                            rgb(0x0080ff)
                                        } else {
                                            rgb(0x404040)
                                        })
                                    })
                                    .cursor_pointer()
                                    .min_w_0()
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |view, _event, _window, cx| {
                                        view.select_window(window_clone.clone(), cx);
                                    }))
                                    .child(
                                        div()
                                            .text_color(rgb(0x888888))
                                            .flex_shrink_0()
                                            .child(format!("{}. ", i + 1)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .min_w_0()
                                            .flex_grow()
                                            .child(
                                                div()
                                                    .text_color(rgb(0xcccccc))
                                                    .w_full()
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .whitespace_nowrap()
                                                    .child(window_info.title.clone()),
                                            )
                                            .child(
                                                div().text_xs().text_color(rgb(0x666666)).child(
                                                    format!("HWND: 0x{:X}", window_info.hwnd),
                                                ),
                                            ),
                                    )
                            })),
                    ),
            )
    }
}

impl Focusable for WindowList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn main() {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx: &mut App| {
        // Initialize gpui-component
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(700.0), px(700.0)), cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(500.0), px(500.0))),
            ..Default::default()
        };

        // Open window directly without detached spawn
        match cx.open_window(window_options, |window, cx| {
            let view = cx.new(|cx| WindowList::new(window, cx));
            
            // Focus the input on window open
            let focus_handle = view.read(cx).input_state.focus_handle(cx);
            window.focus(&focus_handle);
            
            // Wrap the view in Root as required by gpui-component
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("Failed to create window: {:?}", e);
            }
        }
    });
}
