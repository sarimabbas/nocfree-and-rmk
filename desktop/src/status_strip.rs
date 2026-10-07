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
    Unknown,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Peripheral {
    pub level: Option<u8>,
    pub(crate) mode: Option<crate::device_status::Mode>,
    pub usb_connected: bool,
    pub recovery: bool,
    pub(crate) link_connected: Option<bool>,
}

/// Text and icon semantics share the same immutable per-peripheral snapshot.
fn tooltip(
    label: &str,
    connection: Option<Connection>,
    battery: Option<Peripheral>,
    recovery: bool,
) -> String {
    let usb_attachment = !recovery
        && battery.is_some_and(|b| b.usb_connected)
        && (connection == Some(Connection::Unknown)
            || connection.is_none() && battery.is_some_and(|b| b.link_connected.is_none()));
    let mut facts = vec![label.to_owned()];
    if recovery {
        facts.push("Recovery mode".into());
    } else if let Some(route) = connection {
        let mode_matches = battery.and_then(|b| b.mode).is_some_and(|mode| {
            matches!(
                (mode, route),
                (crate::device_status::Mode::Wired, Connection::Usb)
                    | (crate::device_status::Mode::Bluetooth, Connection::Bluetooth)
                    | (crate::device_status::Mode::Dongle, Connection::Dongle)
            )
        });
        if !mode_matches && let Some(mode) = battery.and_then(|b| b.mode) {
            facts.push(mode.label().into());
        }
        if route != Connection::Unknown || usb_attachment {
            facts.push(
                match route {
                    Connection::Usb if label == "Dongle" => "USB connected",
                    Connection::Usb => "Wired connected",
                    Connection::Bluetooth => "Bluetooth connected",
                    Connection::Dongle => "Connected through dongle",
                    Connection::Disconnected => "Not connected",
                    Connection::Unknown if usb_attachment => "USB connected",
                    Connection::Unknown => unreachable!(),
                }
                .into(),
            );
        }
    } else if let Some(state) = battery {
        if let Some(status) = match state.link_connected {
            Some(true) => Some("Connected to left"),
            Some(false) => Some("Not connected"),
            None if usb_attachment => Some("USB connected"),
            None => None,
        } {
            facts.push(status.into());
        }
    }
    if let Some(state) = battery {
        if state.usb_connected
            && !usb_attachment
            && (recovery || connection != Some(Connection::Usb))
        {
            facts.push("USB power connected".into());
        }
        if state.level.is_some()
            || state.usb_connected
            || state.link_connected == Some(true)
            || matches!(
                connection,
                Some(Connection::Usb | Connection::Bluetooth | Connection::Dongle)
            )
        {
            facts.push(
                state
                    .level
                    .map(|v| format!("Battery {v}%"))
                    .unwrap_or_else(|| "Battery unavailable".into()),
            );
        }
    } else if recovery {
        facts.push("USB connected".into());
    }
    facts.join(" · ")
}
fn segment(
    id: &'static str,
    label: &'static str,
    connection: Option<(Connection, IconName, Hsla)>,
    battery: Option<Peripheral>,
    recovery: bool,
    cx: &App,
) -> Button {
    let tooltip = tooltip(label, connection.map(|c| c.0), battery, recovery);
    let mut content = div()
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(12.))
        .font_weight(FontWeight::NORMAL)
        .child(div().text_color(cx.theme().muted_foreground).child(label));
    if !recovery && let Some((_, icon, color)) = connection {
        content = content.child(Icon::new(icon).size(px(14.)).text_color(color));
    }
    if let Some(state) = battery {
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
            .child(
                div().text_color(cx.theme().foreground).child(
                    state
                        .level
                        .map(|v| format!("{v}%"))
                        .unwrap_or_else(|| "—".into()),
                ),
            );
    }
    if recovery {
        content = content.child(Icon::new(IconName::HeartPulse).size(px(14.)).text_color(
            gpui::rgb(if cx.theme().is_dark() {
                0x5eead4
            } else {
                0x0f766e
            }),
        ));
    }
    Button::new(id)
        .cursor_pointer()
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
    let (icon, color) = match left_connection {
        Connection::Usb => (IconName::Plug, plain),
        Connection::Bluetooth => (IconName::Bluetooth, blue),
        Connection::Dongle => (IconName::SatelliteDish, green),
        Connection::Disconnected => (IconName::Unplug, plain),
        Connection::Unknown if left.usb_connected => (IconName::Plug, plain),
        Connection::Unknown => (IconName::CircleDashed, plain),
    };
    let left_segment = segment(
        "left-status",
        "Left",
        Some((left_connection, icon, color)),
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
            if dongle_connected || dongle_recovery {
                Connection::Usb
            } else {
                Connection::Disconnected
            },
            if dongle_connected || dongle_recovery {
                IconName::Plug
            } else {
                IconName::Unplug
            },
            if dongle_connected || dongle_recovery {
                green
            } else {
                plain
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_status::Mode;
    fn half(mode: Option<Mode>, usb: bool, recovery: bool) -> Peripheral {
        Peripheral {
            mode,
            usb_connected: usb,
            recovery,
            level: Some(100),
            link_connected: Some(true),
        }
    }
    #[test]
    fn usb_attachment_does_not_report_an_unavailable_connection() {
        let half = Peripheral {
            usb_connected: true,
            ..Default::default()
        };
        for (label, route) in [("Left", Some(Connection::Unknown)), ("Right", None)] {
            let text = tooltip(label, route, Some(half), false);
            assert!(text.contains("USB connected"), "{text}");
            assert!(!text.contains("Connection unavailable"), "{text}");
            assert!(!text.contains("Wired connected"), "{text}");
        }
    }
    #[test]
    fn tooltip_matrix_keeps_route_power_and_recovery_independent() {
        for mode in [
            None,
            Some(Mode::Wired),
            Some(Mode::Bluetooth),
            Some(Mode::Dongle),
        ] {
            for usb in [false, true] {
                for recovery in [false, true] {
                    for route in [
                        Connection::Usb,
                        Connection::Bluetooth,
                        Connection::Dongle,
                        Connection::Disconnected,
                        Connection::Unknown,
                    ] {
                        let text = tooltip(
                            "Left",
                            Some(route),
                            Some(half(mode, usb, recovery)),
                            recovery,
                        );
                        assert_eq!(text.matches("Battery 100%").count(), 1);
                        assert_eq!(
                            text.matches("USB power connected").count(),
                            usize::from(
                                usb && (recovery
                                    || !matches!(route, Connection::Usb | Connection::Unknown))
                            )
                        );
                        if recovery {
                            assert_eq!(
                                text,
                                format!(
                                    "Left · Recovery mode{}{} · Battery 100%",
                                    if usb { " · USB power connected" } else { "" },
                                    ""
                                )
                            );
                        }
                        if !recovery
                            && matches!(
                                (mode, route),
                                (Some(Mode::Bluetooth), Connection::Bluetooth)
                                    | (Some(Mode::Dongle), Connection::Dongle)
                                    | (Some(Mode::Wired), Connection::Usb)
                            )
                        {
                            assert!(!text.contains("mode"));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn disconnected_unknown_right_and_recovery_dongle_do_not_lie() {
        let right = Peripheral {
            link_connected: Some(false),
            ..Default::default()
        };
        assert_eq!(
            tooltip("Right", None, Some(right), false),
            "Right · Not connected"
        );
        assert_eq!(
            tooltip("Right", None, Some(Peripheral::default()), false),
            "Right"
        );
        assert_eq!(
            tooltip("Dongle", Some(Connection::Disconnected), None, true),
            "Dongle · Recovery mode · USB connected"
        );
        assert_eq!(
            tooltip(
                "Left",
                Some(Connection::Bluetooth),
                Some(half(Some(Mode::Bluetooth), true, false)),
                false
            ),
            "Left · Bluetooth connected · USB power connected · Battery 100%"
        );
    }
}
