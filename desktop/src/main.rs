use nocfree_companion::ui;

use gpui_kit::{
    App, AppContext, Bounds, Focusable, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, actions, px, size,
};

actions!(nocfree_companion, [Quit]);

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if !arguments.is_empty() {
        match arguments.as_slice() {
            [flag, directory] if flag == "--check-firmware" => {
                match nocfree_companion::release::FirmwareRelease::load_from(std::path::Path::new(
                    directory,
                )) {
                    Ok(release) => println!("Verified firmware package: {}", release.id()),
                    Err(error) => {
                        eprintln!("Firmware package: {error}");
                        std::process::exit(1);
                    }
                }
            }
            [flag] if flag == "--help" || flag == "-h" => {
                println!(
                    "NocFree RMK Companion\nUsage: nocfree-companion [--check-firmware DIRECTORY | --help | --version]"
                );
            }
            [flag] if flag == "--version" || flag == "-v" => {
                println!("NocFree RMK Companion {}", env!("CARGO_PKG_VERSION"));
            }
            _ => {
                eprintln!("Unknown arguments. Use --help.");
                std::process::exit(2);
            }
        }
        return;
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.set_menus([Menu::new("NocFree RMK Companion")
                .items([MenuItem::action("Quit NocFree RMK Companion", Quit)])]);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(960.), px(660.)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("NocFree RMK Companion".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(800.), px(560.))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    set_window_max_size(window);
                    let view = cx.new(ui::Companion::new);
                    window.focus(&view.focus_handle(cx), cx);
                    view
                },
            )
            .expect("could not open NocFree RMK Companion window");
            cx.activate(true);
        });
}

#[cfg(target_os = "macos")]
fn set_window_max_size(window: &gpui_kit::Window) {
    use objc2_app_kit::NSView;
    use objc2_foundation::NSSize;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = HasWindowHandle::window_handle(window).expect("native window handle");
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        unreachable!("macOS window must expose an AppKit view");
    };
    // GPUI owns this NSView; its handle stays valid for this call on the UI thread.
    // AppKit enforces the content limit during interactive resize and zoom.
    unsafe {
        let view = &*handle.ns_view.as_ptr().cast::<NSView>();
        view.window()
            .expect("GPUI view must be attached to its window")
            .setContentMaxSize(NSSize::new(1200., 800.));
    }
}

#[cfg(not(target_os = "macos"))]
fn set_window_max_size(_window: &gpui_kit::Window) {}
