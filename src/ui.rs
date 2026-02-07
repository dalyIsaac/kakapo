use crate::keyboard::send_unicode_keystrokes;
use crate::typing::{TypingConfig, DEFAULT_WORDS_PER_MINUTE};
use crate::window_manager::{get_system_windows, WindowInfo};
use gpui::{div, prelude::*, px, rgb, App, Context, Entity, FocusHandle, Focusable, Window};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    v_flex, Disableable,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::HWND;

pub struct WindowList {
    selected_window: Option<WindowInfo>,
    input_state: Entity<InputState>,
    focus_handle: FocusHandle,
    cached_windows: Arc<Mutex<Arc<Vec<WindowInfo>>>>,
    typing_config: TypingConfig,
    words_per_minute_input: Entity<InputState>,
}

impl WindowList {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type text to send...")
                .multi_line(true)
        });

        let typing_config = TypingConfig::default();
        let words_per_minute_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(DEFAULT_WORDS_PER_MINUTE.to_string())
                .default_value(typing_config.words_per_minute.to_string())
        });

        let cached_windows = Arc::new(Mutex::new(Arc::new(get_system_windows())));
        
        // Start background thread to periodically refresh window list
        // This keeps the expensive Windows API call off the UI thread
        let windows_clone = Arc::clone(&cached_windows);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let windows = Arc::new(get_system_windows());
                if let Ok(mut cached) = windows_clone.lock() {
                    *cached = windows;
                }
            }
        });

        Self {
            selected_window: None,
            input_state,
            focus_handle: cx.focus_handle(),
            cached_windows,
            typing_config,
            words_per_minute_input,
        }
    }

    pub fn input_state(&self) -> &Entity<InputState> {
        &self.input_state
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
        let value = self.words_per_minute_input.read(cx).value();
        if let Ok(words_per_minute) = value.parse::<f64>() {
            if words_per_minute > 0.0 && words_per_minute <= TypingConfig::max_words_per_minute() {
                self.typing_config.words_per_minute = words_per_minute;
                cx.notify();
            }
        }
    }

    fn toggle_jitter(&mut self, cx: &mut Context<Self>) {
        self.typing_config.enable_jitter = !self.typing_config.enable_jitter;
        cx.notify();
    }

    /// Render the typing speed configuration controls
    fn render_typing_speed_controls(
        &self,
        cx: &Context<Self>,
        jitter_enabled: bool,
    ) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .items_center()
            .mb_2()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xcccccc))
                    .child("Typing Speed (words/min):"),
            )
            .child(
                div()
                    .w(px(100.))
                    .child(Input::new(&self.words_per_minute_input)),
            )
            .child(
                Button::new("toggle_jitter")
                    .label(if jitter_enabled {
                        "✓ Jitter"
                    } else {
                        "Jitter"
                    })
                    .on_click(cx.listener(|view, _event, _window, cx| {
                        view.toggle_jitter(cx);
                    })),
            )
    }

    /// Render the text input and send button
    fn render_text_input(&self, cx: &Context<Self>, has_selection: bool) -> impl IntoElement {
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
                    .disabled(!has_selection)
                    .on_click(cx.listener(Self::handle_send_click)),
            )
    }

    /// Render the status messages (selection status and validation)
    fn render_status_messages(
        &self,
        selected_title: Option<String>,
        has_selection: bool,
    ) -> impl IntoElement {
        div().flex().flex_col().gap_1().child(
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
        )
    }

    /// Render the input section with controls
    fn render_input_section(
        &self,
        cx: &Context<Self>,
        jitter_enabled: bool,
        has_selection: bool,
        selected_title: Option<String>,
    ) -> impl IntoElement {
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
                    .child("Kakapo: Send Keystrokes"),
            )
            .child(self.render_typing_speed_controls(cx, jitter_enabled))
            .child(self.render_text_input(cx, has_selection))
            .child(self.render_status_messages(selected_title, has_selection))
    }

    /// Render a single window item in the list
    fn render_window_item(
        &self,
        cx: &Context<Self>,
        window_info: &WindowInfo,
        index: usize,
        is_selected: bool,
    ) -> impl IntoElement {
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
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |view, _event, _window, cx| {
                    view.select_window(window_clone.clone(), cx);
                }),
            )
            .child(
                div()
                    .text_color(rgb(0x888888))
                    .flex_shrink_0()
                    .child(format!("{}. ", index + 1)),
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
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child(format!("HWND: 0x{:X}", window_info.hwnd)),
                    ),
            )
    }

    /// Render the window list section
    fn render_window_list_section(
        &self,
        cx: &Context<Self>,
        windows: &[WindowInfo],
        selected_hwnd: Option<isize>,
    ) -> impl IntoElement {
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
                        self.render_window_item(cx, window_info, i, is_selected)
                    })),
            )
    }
}

impl Render for WindowList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Read the cached windows from the background thread
        // Clone only the Arc pointer, not the actual vector
        let windows = self
            .cached_windows
            .lock()
            .expect("Window cache mutex should not be poisoned")
            .clone();
        let selected_hwnd = self.selected_window.as_ref().map(|w| w.hwnd);
        let has_selection = self.selected_window.is_some();
        let selected_title = self.selected_window.as_ref().map(|w| w.title.clone());
        let jitter_enabled = self.typing_config.enable_jitter;

        v_flex()
            .gap_3()
            .bg(rgb(0x2d2d2d))
            .size_full()
            .p_4()
            .child(self.render_input_section(cx, jitter_enabled, has_selection, selected_title))
            .child(self.render_window_list_section(cx, &windows, selected_hwnd))
    }
}

impl Focusable for WindowList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
