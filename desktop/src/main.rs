use nocfree_companion::ui;

use gpui_kit::{
    App, AppContext, Bounds, Focusable, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, actions, px, size,
};

actions!(nocfree_companion, [Quit]);

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.set_menus([Menu::new("Nocfree RMK Companion")
                .items([MenuItem::action("Quit Nocfree RMK Companion", Quit)])]);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(960.), px(660.)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Nocfree RMK Companion".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(800.), px(560.))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let view = cx.new(ui::Companion::new);
                    window.focus(&view.focus_handle(cx), cx);
                    view
                },
            )
            .expect("could not open Nocfree RMK Companion window");
            cx.activate(true);
        });
}
