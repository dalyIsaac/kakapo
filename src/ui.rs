use crate::keyboard::send_unicode_keystrokes;
use crate::typing::{TypingConfig, DEFAULT_WORDS_PER_MINUTE};
use crate::window_manager::{get_system_windows, WindowInfo};
use gpui::{div, prelude::*, px, rgb, App, Context, Entity, FocusHandle, Focusable, Window};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    v_flex, Disableable,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::HWND;

pub struct WindowList {
    selected_window: Option<WindowInfo>,
    input_state: Entity<InputState>,
    focus_handle: FocusHandle,
    cached_windows: Arc<Mutex<Arc<Vec<WindowInfo>>>>,
    cached_windows_local: Arc<Vec<WindowInfo>>,
    typing_config: Arc<Mutex<TypingConfig>>,
    words_per_minute_input: Entity<InputState>,
    is_typing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    last_text: String,
    last_wpm_input: String,
    current_target_hwnd: Arc<Mutex<Option<isize>>>,
}

impl WindowList {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type text to send...")
                .multi_line(true)
        });

        let typing_config = Arc::new(Mutex::new(TypingConfig::default()));
        let words_per_minute_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(DEFAULT_WORDS_PER_MINUTE.to_string())
                .default_value(typing_config.lock().unwrap().words_per_minute.to_string())
        });

        let initial_windows = Arc::new(get_system_windows());
        let cached_windows = Arc::new(Mutex::new(Arc::clone(&initial_windows)));

        // Start background thread to periodically refresh window list
        // This keeps the expensive Windows API call off the UI thread
        // Refresh every 2 seconds to minimize overhead
        let windows_clone = Arc::clone(&cached_windows);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(2));
            let windows = Arc::new(get_system_windows());
            if let Ok(mut cached) = windows_clone.lock() {
                *cached = windows;
            }
        });

        Self {
            selected_window: None,
            input_state,
            focus_handle: cx.focus_handle(),
            cached_windows,
            cached_windows_local: initial_windows,
            typing_config,
            words_per_minute_input,
            is_typing: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            last_text: String::new(),
            last_wpm_input: String::new(),
            current_target_hwnd: Arc::new(Mutex::new(None)),
        }
    }

    pub fn input_state(&self) -> &Entity<InputState> {
        &self.input_state
    }

    fn select_window(&mut self, window_info: WindowInfo, cx: &mut Context<Self>) {
        self.selected_window = Some(window_info.clone());

        // If currently typing, update the target window
        if self.is_typing.load(Ordering::SeqCst) {
            if let Ok(mut target) = self.current_target_hwnd.lock() {
                *target = Some(window_info.hwnd);
            }
        }

        cx.notify();
    }

    fn send_keystrokes(&mut self, cx: &mut Context<Self>) {
        if let Some(ref window) = self.selected_window {
            let text = self.input_state.read(cx).value();
            if !text.is_empty() {
                let hwnd = window.hwnd;
                let config = self.typing_config.clone();
                let is_typing = self.is_typing.clone();
                let is_paused = self.is_paused.clone();
                let target_hwnd = self.current_target_hwnd.clone();

                // Mark that we're starting to type and not paused
                is_typing.store(true, Ordering::SeqCst);
                is_paused.store(false, Ordering::SeqCst);

                // Save the text we're sending and set the target window
                self.last_text = text.to_string();
                if let Ok(mut target) = target_hwnd.lock() {
                    *target = Some(hwnd);
                }

                cx.notify();

                // Spawn background thread to avoid blocking UI
                std::thread::spawn(move || {
                    if let Err(e) = send_unicode_keystrokes(
                        HWND(hwnd as _),
                        &text,
                        &config,
                        &is_typing,
                        &is_paused,
                        &target_hwnd,
                    ) {
                        eprintln!("Error sending keystrokes: {}", e);
                    }
                    // Mark that we've stopped typing
                    is_typing.store(false, Ordering::SeqCst);
                    is_paused.store(false, Ordering::SeqCst);
                    // Clear the target window
                    if let Ok(mut target) = target_hwnd.lock() {
                        *target = None;
                    }
                });
            }
        }
    }

    fn stop_typing(&mut self, cx: &mut Context<Self>) {
        self.is_typing.store(false, Ordering::SeqCst);
        self.is_paused.store(false, Ordering::SeqCst);
        self.last_text.clear();
        cx.notify();
    }

    fn check_text_changed(&mut self, cx: &mut Context<Self>) -> bool {
        let current_text = self.input_state.read(cx).value();
        let changed = current_text != self.last_text;

        // If text changed while typing, stop the typing
        if changed && self.is_typing.load(Ordering::SeqCst) {
            self.is_typing.store(false, Ordering::SeqCst);
            self.is_paused.store(false, Ordering::SeqCst);
            self.last_text.clear();
        }

        changed
    }

    fn check_and_update_typing_speed(&mut self, cx: &mut Context<Self>) {
        let current_wpm = self.words_per_minute_input.read(cx).value();

        // Only update if the value has changed
        if current_wpm != self.last_wpm_input {
            self.last_wpm_input = current_wpm.to_string();
            self.update_typing_speed(cx);
        }
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        let current = self.is_paused.load(Ordering::SeqCst);

        // Simple toggle - don't check focus here to avoid immediate resume on clicking
        self.is_paused.store(!current, Ordering::SeqCst);

        // If resuming (was paused, now not paused), refocus the window
        if current {
            if let Some(ref window) = self.selected_window {
                let hwnd = HWND(window.hwnd as _);
                // Refocus the window in a separate thread to avoid blocking UI
                std::thread::spawn(move || unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
                    let _ = SetForegroundWindow(hwnd);
                });
            }
        }

        cx.notify();
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
                if let Ok(mut config) = self.typing_config.lock() {
                    config.words_per_minute = words_per_minute;
                }
                cx.notify();
            }
        }
    }

    fn toggle_newline_mode(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut config) = self.typing_config.lock() {
            config.use_windows_newlines = !config.use_windows_newlines;
        }
        cx.notify();
    }

    /// Render the typing speed configuration controls
    fn render_typing_speed_controls(&self, cx: &Context<Self>) -> impl IntoElement {
        let use_windows_newlines = self
            .typing_config
            .lock()
            .map(|c| c.use_windows_newlines)
            .unwrap_or(false);

        div()
            .flex()
            .gap_4() // Changed from flex_col to horizontal layout
            .items_center()
            .mb_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
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
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x333333))
                            .child("Newline Mode:"),
                    )
                    .child(
                        Button::new("newline_toggle")
                            .label(if use_windows_newlines {
                                "CRLF (Windows)"
                            } else {
                                "LF (Unix)"
                            })
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                view.toggle_newline_mode(cx);
                            })),
                    ),
            )
    }

    /// Render the status messages (selection status and validation)
    fn render_status_messages(
        &self,
        selected_title: Option<String>,
        has_selection: bool,
        is_paused: bool,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
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
            )
            // Always show pause message area to prevent button shifting
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xffcc66))
                    .h(px(20.)) // Reserve fixed height
                    .when(is_paused, |div| {
                        div.child("⏸ Paused (click Resume to continue)")
                    }),
            )
    }

    /// Render the input section with controls
    fn render_input_section(
        &self,
        cx: &Context<Self>,
        has_selection: bool,
        selected_title: Option<String>,
        _is_typing: bool,
        is_paused: bool,
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
            // Textbox
            .child(
                div()
                    .flex()
                    .child(Input::new(&self.input_state).h(px(200.)).w_full()),
            )
            // Typing speed controls
            .child(self.render_typing_speed_controls(cx))
            // Status messages
            .child(self.render_status_messages(selected_title, has_selection, is_paused))
    }

    /// Render just the buttons section (separated to place after window list)
    fn render_buttons_standalone(
        &self,
        cx: &Context<Self>,
        has_selection: bool,
        is_typing: bool,
        is_paused: bool,
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
            .child(self.render_buttons(cx, has_selection, is_typing, is_paused))
    }

    /// Render just the buttons section
    fn render_buttons(
        &self,
        cx: &Context<Self>,
        has_selection: bool,
        is_typing: bool,
        is_paused: bool,
    ) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .items_center()
            .child(
                Button::new("send")
                    .primary()
                    .label("Send")
                    .disabled(!has_selection || is_typing)
                    .on_click(cx.listener(Self::handle_send_click)),
            )
            .child(
                Button::new("pause")
                    .label(if is_paused { "Resume" } else { "Pause" })
                    .disabled(!is_typing)
                    .on_click(cx.listener(|view, _event, _window, cx| {
                        view.toggle_pause(cx);
                    })),
            )
            .child(
                Button::new("stop")
                    .label("Stop")
                    .disabled(!is_typing)
                    .on_click(cx.listener(|view, _event, _window, cx| {
                        view.stop_typing(cx);
                    })),
            )
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
            .min_h(px(200.)) // Ensure minimum height
            .overflow_hidden()
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(0xffffff))
                    .mb_2()
                    .flex_shrink_0()
                    .child("Window List"),
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
        // Check if text has changed while typing - if so, reset state
        self.check_text_changed(cx);

        // Check if typing speed has changed and update config
        self.check_and_update_typing_speed(cx);

        // Try to update local cache from background thread without blocking
        // This avoids locking on every render frame
        if let Ok(cached) = self.cached_windows.try_lock() {
            self.cached_windows_local = Arc::clone(&cached);
        }

        // Use the local cached copy for rendering - no lock needed
        let windows = &self.cached_windows_local;
        let selected_hwnd = self.selected_window.as_ref().map(|w| w.hwnd);
        let has_selection = self.selected_window.is_some();
        let selected_title = self.selected_window.as_ref().map(|w| w.title.clone());
        let is_typing = self.is_typing.load(Ordering::SeqCst);
        let mut is_paused = self.is_paused.load(Ordering::SeqCst);

        // Requirement 2: Check if target window has lost focus while typing
        // If so, treat it as paused for UI purposes (button shows "Resume")
        let target_has_focus = if is_typing && has_selection {
            if let Some(ref window) = self.selected_window {
                use crate::window_manager::is_window_focused;
                is_window_focused(HWND(window.hwnd as _))
            } else {
                true
            }
        } else {
            true
        };

        // If target lost focus while typing, show as paused in UI
        if is_typing && !target_has_focus {
            is_paused = true;
        }

        v_flex()
            .gap_3()
            .bg(rgb(0x2d2d2d))
            .size_full()
            .p_4()
            // 1. Header, 2. Textbox, 3. Typing speed, 4. Messages (all in render_input_section)
            .child(self.render_input_section(
                cx,
                has_selection,
                selected_title,
                is_typing,
                is_paused,
            ))
            // 5. Window list
            .child(self.render_window_list_section(cx, windows, selected_hwnd))
            // 6. Buttons
            .child(self.render_buttons_standalone(cx, has_selection, is_typing, is_paused))
    }
}

impl Focusable for WindowList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
