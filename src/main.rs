use gpui::{
    App, Application, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    rgb, size,
};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, IsWindowVisible};
use windows::core::BOOL;

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

struct WindowList {}

impl Render for WindowList {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let windows = get_system_windows();
        let window_count = windows.len();

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
                            .child("Window List:"),
                    )
                    .children(windows.into_iter().enumerate().map(|(i, window_info)| {
                        div()
                            .flex()
                            .gap_2()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(0x353535))
                            .border_1()
                            .border_color(rgb(0x454545))
                            .hover(|style| style.bg(rgb(0x404040)))
                            .child(
                                div()
                                    .text_color(rgb(0x888888))
                                    .child(format!("{}. ", i + 1)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(0xcccccc))
                                            .child(window_info.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x666666))
                                            .child(format!("HWND: 0x{:X}", window_info.hwnd)),
                                    ),
                            )
                    })),
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
                            "Showing all visible windows with titles from your Windows system",
                        ),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.0), px(600.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(500.0), px(400.0))),
                ..Default::default()
            },
            |_window, cx| cx.new(|_| WindowList {}),
        )
        .unwrap();
    });
}
