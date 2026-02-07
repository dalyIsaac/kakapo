mod keyboard;
mod rng;
mod typing;
mod ui;
mod window_manager;

use gpui::{
    px, size, App, AppContext, Application, Bounds, Focusable, WindowBounds, WindowOptions,
};
use gpui_component::Root;
use gpui_component_assets::Assets;
use ui::WindowList;

fn main() {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx: &mut App| {
        // Initialize gpui-component
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(700.0), px(700.0)), cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(800.0), px(500.0))), // Increased from 500 to 800
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Kakapo".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };

        // Open window directly without detached spawn
        match cx.open_window(window_options, |window, cx| {
            let view = cx.new(|cx| WindowList::new(window, cx));

            // Focus the input on window open
            let input_state = view.read(cx).input_state().clone();
            let focus_handle = input_state.read(cx).focus_handle(cx);
            window.focus(&focus_handle);

            // Wrap the view in Root as required by gpui-component
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to create window: {:?}", e);
            }
        }
    });
}
