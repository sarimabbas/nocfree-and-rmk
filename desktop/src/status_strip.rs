//! Compact, per-part status presentation. Callers supply observed device state.
use gpui::{App, Hsla, ParentElement, Styled, div, px};
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
    // The USB-only discovery backend cannot observe direct Bluetooth yet.
    #[allow(dead_code)]
    Bluetooth,
    Dongle,
    #[default]
    Disconnected,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Peripheral {
    pub level: Option<u8>,
    pub usb_connected: bool,
    pub recovery: bool,
}

fn indicator(id: &'static str, icon: IconName, color: Hsla, tooltip: String) -> Button {
    Button::new(id)
        .icon(Icon::new(icon).text_color(color))
        .ghost()
        .small()
        .compact()
        .tooltip(tooltip)
}

fn battery(id: &'static str, part: &'static str, state: Peripheral, cx: &App) -> Button {
    let level = state.level.map(|v| format!("{v}%"));
    let tooltip = match &level {
        Some(level) => format!("{part} battery: {level}"),
        None => format!("{part} battery is unavailable"),
    };
    indicator(
        id,
        if state.usb_connected {
            IconName::BatteryCharging
        } else {
            IconName::Battery
        },
        cx.theme().foreground,
        if state.usb_connected {
            format!("{tooltip} · USB connected")
        } else {
            tooltip
        },
    )
    .label(level.unwrap_or_else(|| "—".into()))
}

fn recovery(id: &'static str, part: &'static str, cx: &App) -> Button {
    indicator(
        id,
        IconName::HeartPulse,
        gpui::rgb(if cx.theme().is_dark() {
            0x5eead4
        } else {
            0x0f766e
        })
        .into(),
        format!("{part} is in recovery mode"),
    )
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
        Connection::Usb => (IconName::Plug, plain, "Left half connected by USB"),
        Connection::Bluetooth => (
            IconName::Bluetooth,
            blue,
            "Left half connected by Bluetooth",
        ),
        Connection::Dongle => (
            IconName::SatelliteDish,
            green,
            "Left half connected through the dongle",
        ),
        Connection::Disconnected => (IconName::Unplug, plain, "Left half is not connected"),
    };
    let mut left_segment = div()
        .flex()
        .items_center()
        .gap(px(3.))
        .child("Left")
        .child(indicator("left-connection", icon, color, connection.into()))
        .child(battery("left-battery", "Left half", left, cx));
    if left.recovery {
        left_segment = left_segment.child(recovery("left-recovery", "Left half", cx));
    }
    let mut right_segment = div()
        .flex()
        .items_center()
        .gap(px(3.))
        .child("Right")
        .child(battery("right-battery", "Right half", right, cx));
    if right.recovery {
        right_segment = right_segment.child(recovery("right-recovery", "Right half", cx));
    }
    let mut dongle_segment = div()
        .flex()
        .items_center()
        .gap(px(3.))
        .child("Dongle")
        .child(indicator(
            "dongle-connection",
            if dongle_connected {
                IconName::Plug
            } else {
                IconName::Unplug
            },
            if dongle_connected { green } else { plain },
            if dongle_connected {
                "Dongle connected by USB"
            } else {
                "Dongle is not connected"
            }
            .into(),
        ));
    if dongle_recovery {
        dongle_segment = dongle_segment.child(recovery("dongle-recovery", "Dongle", cx));
    }
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
