use gpui::{
    actions, div, point, prelude::*, px, rgb, size, App, Application, Bounds, ClipboardItem, Context,
    CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, Window, WindowBounds, WindowOptions,
};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use unicode_segmentation::*;
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

        // Convert to UTF-16 to properly handle emoji and non-BMP characters (surrogate pairs)
        let utf16_chars: Vec<u16> = text.encode_utf16().collect();
        
        // Send each UTF-16 code unit as a keystroke
        for &code_unit in &utf16_chars {
            let mut inputs = Vec::new();

            // Key down event
            let mut input_down = INPUT::default();
            input_down.r#type = INPUT_KEYBOARD;
            input_down.Anonymous.ki = KEYBDINPUT {
                wVk: Default::default(),
                wScan: code_unit,
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
                wScan: code_unit,
                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs.push(input_up);

            // Send the input events
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);

            // Small delay between code units for reliability
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

// Actions for text input
actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Copy,
        Cut,
    ]
);

// TextInput component based on gpui example
struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "Type text to send...".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace("\n", " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(Some(self.selected_range.clone()))
            .unwrap();

        let mut content = self.content.to_string();
        content.replace_range(range.clone(), new_text);
        self.content = content.into();

        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.marked_range.take();
        cx.notify();
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx)
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(Some(self.selected_range.clone()))
            .unwrap();

        let mut content = self.content.to_string();
        content.replace_range(range.clone(), new_text);
        self.content = content.into();

        let new_range_start = range.start;
        let new_range_end = range.start + new_text.len();
        let new_range = new_range_start..new_range_end;
        
        self.marked_range = Some(new_range.clone());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .unwrap_or(new_range.end..new_range.end);
        
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let Some(line) = self.last_layout.as_ref() else {
            return None;
        };

        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                element_bounds.left() + line.x_for_index(range.start),
                element_bounds.top(),
            ),
            point(
                element_bounds.left() + line.x_for_index(range.end),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// TextElement for rendering the input
struct TextElement {
    input: Entity<TextInput>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = px(30.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = &input.content;
        let selected_range = &input.selected_range;

        let display_text = if content.is_empty() {
            input.placeholder.clone()
        } else {
            content.clone()
        };

        let text_color = if content.is_empty() {
            rgb(0x666666)
        } else {
            rgb(0xffffff)
        };

        let cursor = input.cursor_offset();
        let style = window.text_style();
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(gpui::fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(gpui::fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    gpui::rgba(0x3311ff30),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .line_height(px(30.))
            .child(
                div()
                    .h(px(30. + 4. * 2.))
                    .w_full()
                    .p(px(4.))
                    .bg(rgb(0x2d2d2d))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x505050))
                    .child(TextElement { input: cx.entity() }),
            )
    }
}

struct WindowList {
    selected_window: Option<WindowInfo>,
    text_input: Entity<TextInput>,
    focus_handle: FocusHandle,
    cached_windows: Vec<WindowInfo>,
}

impl WindowList {
    fn new(cx: &mut Context<Self>) -> Self {
        let text_input = cx.new(|cx| TextInput::new(cx));
        Self {
            selected_window: None,
            text_input,
            focus_handle: cx.focus_handle(),
            cached_windows: get_system_windows(),
        }
    }

    fn select_window(&mut self, window_info: WindowInfo, cx: &mut Context<Self>) {
        self.selected_window = Some(window_info);
        cx.notify();
    }

    fn send_keystrokes(&mut self, cx: &mut Context<Self>) {
        if let Some(ref window) = self.selected_window {
            let text = self.text_input.read(cx).content.to_string();
            if !text.is_empty() {
                let hwnd = window.hwnd;
                // Spawn background thread to avoid blocking UI
                std::thread::spawn(move || {
                    send_unicode_keystrokes(HWND(hwnd as _), &text);
                });
            }
        }
    }

    fn handle_send_click(
        &mut self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_keystrokes(cx);
    }

    fn get_invalid_chars(&self, cx: &App) -> Vec<char> {
        let _text = self.text_input.read(cx).content.to_string();
        // For now, all characters can be sent via Unicode SendInput
        // In the future, we might add validation logic here
        vec![]
    }
}

impl Render for WindowList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let windows = &self.cached_windows;
        let selected_hwnd = self.selected_window.as_ref().map(|w| w.hwnd);
        let has_selection = self.selected_window.is_some();
        let selected_title = self.selected_window.as_ref().map(|w| w.title.clone());
        let invalid_chars = self.get_invalid_chars(cx);
        let text = self.text_input.read(cx).content.to_string();
        let text_empty = text.is_empty();

        div()
            .flex()
            .flex_col()
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
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .child(self.text_input.clone()),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .bg(if has_selection && !text_empty {
                                        rgb(0x0066cc)
                                    } else {
                                        rgb(0x505050)
                                    })
                                    .rounded_md()
                                    .text_color(rgb(0xffffff))
                                    .cursor_pointer()
                                    .when(
                                        has_selection && !text_empty,
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
    }
}

impl Focusable for WindowList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.0), px(700.0)), cx);
        
        // Bind keys for text input
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            KeyBinding::new("cmd-v", Paste, None),
            KeyBinding::new("cmd-c", Copy, None),
            KeyBinding::new("cmd-x", Cut, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
        ]);

        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(500.0), px(500.0))),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| WindowList::new(cx)),
        )
        .unwrap();
        
        // Focus the text input on startup
        let focus_handle = window
            .update(cx, |view, _window, cx| view.text_input.focus_handle(cx))
            .unwrap();
        window
            .update(cx, |_view, window, _cx| {
                window.focus(&focus_handle);
            })
            .unwrap();
    });
}
