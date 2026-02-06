use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, FocusHandle, KeyDownEvent,
    MouseDownEvent, SharedString, Window, WindowBounds, WindowOptions,
};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, IsWindowVisible, SetForegroundWindow,
};

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
                    hwnd: hwnd.0 as isize,
                });
            }
        }
    }

    true.into()
}

/// Sends Unicode keystrokes to a window using SendInput.
/// This method works with virtual machines and remote desktop applications
/// like Amazon Workspaces and Azure Virtual Desktop because it uses
/// KEYEVENTF_UNICODE which injects keystrokes at the lowest level.
///
/// Reference: https://github.com/keepassxreboot/keepassxc
fn send_unicode_keystrokes(hwnd: HWND, text: &str) {
    unsafe {
        // Bring the target window to the foreground
        let _ = SetForegroundWindow(hwnd);

        // Small delay to let the window activation complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Send each character as a Unicode keystroke
        for ch in text.chars() {
            let mut inputs = Vec::new();

            // Key down event
            let mut input_down = INPUT::default();
            input_down.r#type = INPUT_KEYBOARD;
            input_down.Anonymous.ki = KEYBDINPUT {
                wVk: Default::default(),
                wScan: ch as u16,
                dwFlags: KEYEVENTF_UNICODE,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs.push(input_down);

            // Key up event
            let mut input_up = INPUT::default();
            input_up.r#type = INPUT_KEYBOARD;
            input_up.Anonymous.ki = KEYBDINPUT {
                wVk: Default::default(),
                wScan: ch as u16,
                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs.push(input_up);

            // Send the input events
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);

            // Small delay between characters for reliability
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

struct WindowList {
    selected_window: Option<WindowInfo>,
    input_text: SharedString,
    focus_handle: FocusHandle,
}

impl WindowList {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            selected_window: None,
            input_text: SharedString::from(""),
            focus_handle: cx.focus_handle(),
        }
    }

    fn select_window(&mut self, window_info: WindowInfo, cx: &mut Context<Self>) {
        self.selected_window = Some(window_info);
        cx.notify();
    }

    fn update_input(&mut self, text: String, cx: &mut Context<Self>) {
        self.input_text = SharedString::from(text);
        cx.notify();
    }

    fn send_keystrokes(&self) {
        if let Some(ref window) = self.selected_window {
            if !self.input_text.is_empty() {
                send_unicode_keystrokes(HWND(window.hwnd as _), self.input_text.as_ref());
            }
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "backspace" {
            let mut text = self.input_text.to_string();
            text.pop();
            self.update_input(text, cx);
        } else if event.keystroke.key == "enter" {
            self.send_keystrokes();
        } else if event.keystroke.key.chars().count() == 1
            && !event.keystroke.key.chars().next().unwrap().is_control()
        {
            // Only add printable single characters
            let mut text = self.input_text.to_string();
            text.push_str(&event.keystroke.key);
            self.update_input(text, cx);
        }
    }

    fn handle_send_click(
        &mut self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.selected_window.is_some() && !self.input_text.is_empty() {
            self.send_keystrokes();
        }
    }
}

impl Render for WindowList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let windows = get_system_windows();
        let window_count = windows.len();
        let selected_hwnd = self.selected_window.as_ref().map(|w| w.hwnd);
        let input_text_str = self.input_text.to_string();
        let input_empty = input_text_str.is_empty();
        let has_selection = self.selected_window.is_some();
        let selected_title = self.selected_window.as_ref().map(|w| w.title.clone());

        div()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(0x2d2d2d))
            .size_full()
            .p_4()
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
                            .text_xl()
                            .text_color(rgb(0xffffff))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(format!("System Windows ({})", window_count)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xaaaaaa))
                            .child("All visible windows currently open on your system"),
                    ),
            )
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
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .p_2()
                                    .bg(rgb(0x2d2d2d))
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(0x505050))
                                    .track_focus(&self.focus_handle)
                                    .on_key_down(cx.listener(Self::handle_key_down))
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|view, _event, window, _cx| {
                                        window.focus(&view.focus_handle);
                                    }))
                                    .child(
                                        div()
                                            .text_color(if input_empty {
                                                rgb(0x666666)
                                            } else {
                                                rgb(0xffffff)
                                            })
                                            .child(if input_empty {
                                                "Type text to send...".to_string()
                                            } else {
                                                input_text_str
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .bg(if has_selection && !input_empty {
                                        rgb(0x0066cc)
                                    } else {
                                        rgb(0x505050)
                                    })
                                    .rounded_md()
                                    .text_color(rgb(0xffffff))
                                    .cursor_pointer()
                                    .when(
                                        has_selection && !input_empty,
                                        |style| style.hover(|s| s.bg(rgb(0x0080ff))),
                                    )
                                    .child("Send")
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::handle_send_click)),
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
                            .child(if let Some(title) = selected_title {
                                format!("✓ Selected: {}", title)
                            } else {
                                "✗ No window selected. Click a window below to select it.".to_string()
                            }),
                    ),
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
                            .children(windows.into_iter().enumerate().map(|(i, window_info)| {
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
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_4()
                    .bg(rgb(0x3d3d3d))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x505050))
                    .child(
                        div().text_sm().text_color(rgb(0x888888)).child(
                            "Uses KEYEVENTF_UNICODE for compatibility with VMs and remote desktop apps (Amazon Workspaces, Azure Virtual Desktop)",
                        ),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.0), px(700.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(500.0), px(500.0))),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| WindowList::new(cx)),
        )
        .unwrap();
    });
}
