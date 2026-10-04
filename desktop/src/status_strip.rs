//! Compact, per-part status presentation. Callers supply observed device state.
use gpui::{App, FontWeight, Hsla, ParentElement, Styled, div, px};
use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    separator::Separator,
    status_bar::StatusBar,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Connection {
    Usb,
    Bluetooth,
    Dongle,
    #[default]
    Disconnected,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Peripheral {
    pub level: Option<u8>,
    pub(crate) mode: Option<crate::device_status::Mode>,
    pub usb_connected: bool,
    pub recovery: bool,
}

fn segment(
    id: &'static str,
    label: &'static str,
    connection: Option<(IconName, Hsla, &'static str)>,
    battery: Option<Peripheral>,
    recovery: bool,
    cx: &App,
) -> Button {
    let mut tooltip = label.to_owned();
    if label == "Left" {
        tooltip.push_str(". ");
        tooltip.push_str(
            battery
                .and_then(|state| state.mode)
                .map(|mode| mode.label())
                .unwrap_or("Mode unavailable"),
        );
    }
    let mut content = div()
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(12.))
        .font_weight(FontWeight::NORMAL)
        .child(div().text_color(cx.theme().muted_foreground).child(label));
    if let Some((icon, color, description)) = connection {
        tooltip.push_str(&format!(". {description}"));
        content = content.child(Icon::new(icon).size(px(14.)).text_color(color));
    }
    if let Some(state) = battery {
        let level = state
            .level
            .map(|v| format!("{v}%"))
            .unwrap_or_else(|| "—".into());
        if state.usb_connected && connection.is_none() {
            tooltip.push_str(". USB connected");
        }
        tooltip.push_str(
            &state
                .level
                .map(|v| format!(". Battery {v}%"))
                .unwrap_or_else(|| ". Battery unavailable".into()),
        );
        content = content
            .child(
                Icon::new(if state.usb_connected {
                    IconName::BatteryCharging
                } else {
                    IconName::Battery
                })
                .size(px(14.))
                .text_color(cx.theme().foreground),
            )
            .child(div().text_color(cx.theme().foreground).child(level));
    }
    if recovery {
        tooltip.push_str(". Recovery mode");
        content = content.child(Icon::new(IconName::HeartPulse).size(px(14.)).text_color(
            gpui::rgb(if cx.theme().is_dark() {
                0x5eead4
            } else {
                0x0f766e
            }),
        ));
    }
    Button::new(id)
        .ghost()
        .small()
        .compact()
        .accessibility_label(tooltip.clone())
        .tooltip(tooltip)
        .child(content)
}

/// Recovery belongs to the individual peripheral, never the firmware label.
pub fn render(
    firmware: String,
    left_connection: Connection,
    left: Peripheral,
    right: Peripheral,
    dongle_connected: bool,
    dongle_recovery: bool,
    cx: &App,
) -> StatusBar {
    let plain = cx.theme().muted_foreground;
    let blue = gpui::rgb(if cx.theme().is_dark() {
        0x60a5fa
    } else {
        0x2563eb
    })
    .into();
    let green = gpui::rgb(if cx.theme().is_dark() {
        0x86efac
    } else {
        0x15803d
    })
    .into();
    let (icon, color, connection) = match left_connection {
        Connection::Usb => (IconName::Plug, plain, "USB connected"),
        Connection::Bluetooth => (IconName::Bluetooth, blue, "Bluetooth connected"),
        Connection::Dongle => (IconName::SatelliteDish, green, "Connected through dongle"),
        Connection::Disconnected => (IconName::Unplug, plain, "Not connected"),
    };
    let (icon, color) = match left.mode {
        Some(crate::device_status::Mode::Wired) => (IconName::Plug, plain),
        Some(crate::device_status::Mode::Bluetooth) => (IconName::Bluetooth, blue),
        Some(crate::device_status::Mode::Dongle) => (IconName::SatelliteDish, green),
        None => (icon, color),
    };
    let left_segment = segment(
        "left-status",
        "Left",
        Some((icon, color, connection)),
        Some(left),
        left.recovery,
        cx,
    );
    let right_segment = segment(
        "right-status",
        "Right",
        None,
        Some(right),
        right.recovery,
        cx,
    );
    let dongle_segment = segment(
        "dongle-status",
        "Dongle",
        Some((
            if dongle_connected {
                IconName::Plug
            } else {
                IconName::Unplug
            },
            if dongle_connected { green } else { plain },
            if dongle_connected {
                "USB connected"
            } else {
                "Not connected"
            },
        )),
        None,
        dongle_recovery,
        cx,
    );
    StatusBar::new()
        .h(px(36.))
        .px(px(16.))
        .flex_none()
        .left(firmware)
        .right(left_segment)
        .right(Separator::vertical().h(px(16.)).mx(px(6.)))
        .right(right_segment)
        .right(Separator::vertical().h(px(16.)).mx(px(6.)))
        .right(dongle_segment)
}
