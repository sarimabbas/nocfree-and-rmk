mod battery;
mod device;
mod home;
mod recovery;
mod session;
mod trial;
mod ui;

use gpui::{
    App, AppContext, Application, Bounds, Focusable, KeyBinding, Menu, MenuItem, TitlebarOptions,
    WindowAppearance, WindowBounds, WindowOptions, actions, px, size,
};

actions!(nocfree_companion, [Quit]);

fn main() {
    Application::with_platform(gpui_platform::current_platform(false))
        .with_assets(gpuikit::assets())
        .run(|cx: &mut App| {
            gpuikit::init(cx);
            cx.set_window_appearance(Some(WindowAppearance::Light));
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.set_menus([Menu::new("NocFree Companion")
                .items([MenuItem::action("Quit NocFree Companion", Quit)])]);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(640.), px(580.)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("NocFree Companion".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(560.), px(540.))),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(ui::Companion::new);
                    window.focus(&view.focus_handle(cx), cx);
                    view
                },
            )
            .expect("could not open NocFree Companion window");
            cx.activate(true);
        });
}
