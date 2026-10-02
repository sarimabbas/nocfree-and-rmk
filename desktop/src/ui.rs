//! Presentation only. Discovery and backup decisions remain in the session.
use std::{path::PathBuf, sync::Arc, time::Duration};

use crate::{
    device::{self, Role},
    session::{Session, View},
};
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Task, Window, div,
    prelude::FluentBuilder, px, rgb,
};
use gpuikit::{
    a11y::FocusNavigation,
    elements::button::button,
    theme::{GlobalTheme, Theme, ThemeVariant},
};

pub struct Companion {
    session: Option<Session>,
    view: View,
    role: Option<Role>,
    busy: bool,
    left_backup: Option<PathBuf>,
    copies_folder: Option<PathBuf>,
    completed: bool,
    stopped: bool,
    message: Option<String>,
    focus_handle: FocusHandle,
    _poll: Task<()>,
}

impl Companion {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut theme = Theme::new(
            "NocFree",
            ThemeVariant::Light,
            rgb(0xffffff).into(),
            rgb(0xffffff).into(),
            rgb(0xf1f3f5).into(),
            rgb(0xe3e6ea).into(),
            rgb(0x007aff).into(),
        );
        theme.fg_muted_color = Some(rgb(0x626870).into());
        theme.button_bg_color = Some(rgb(0x007aff).into());
        theme.button_bg_hover_color = Some(rgb(0x0068da).into());
        theme.button_bg_active_color = Some(rgb(0x005bc2).into());
        cx.set_global(GlobalTheme(Arc::new(theme)));
        let mut session = Session::new();
        session.select(Role::Left);
        let view = session.view();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let result = cx
                    .background_executor()
                    .spawn(async { device::discover() })
                    .await;
                let running = this.update(cx, |this, cx| {
                    if this.completed {
                        return false;
                    }
                    if let Some(session) = this.session.as_mut() {
                        session.observe(result);
                        this.view = session.view();
                        if let Some(error) = this.view.error.clone() {
                            this.stopped = true;
                            this.message = Some(error);
                        }
                        this.advance(cx);
                        cx.notify();
                    }
                    !this.completed
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
                let timer = cx.background_executor().timer(Duration::from_secs(1));
                timer.await;
            }
        });
        Self {
            session: Some(session),
            view,
            role: Some(Role::Left),
            busy: false,
            left_backup: None,
            copies_folder: None,
            completed: false,
            stopped: false,
            message: None,
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.stopped || self.completed {
            return;
        }
        if self.view.backup_path.is_some() && self.view.return_complete {
            if self.role == Some(Role::Left) {
                self.left_backup = self.view.backup_path.clone();
                if let Some(session) = self.session.as_mut() {
                    session.select(Role::Right);
                    self.view = session.view();
                    self.role = Some(Role::Right);
                }
            } else if self.left_backup.is_some() {
                self.completed = true;
            }
        } else if self.view.can_save {
            self.save(cx);
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.retry();
            self.view = session.view();
            self.stopped = false;
            self.message = None;
            cx.notify();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.view.can_save || self.busy || self.stopped {
            return;
        }
        let Some(mut session) = self.session.take() else {
            return;
        };
        self.busy = true;
        self.message = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (session, result) = cx
                .background_executor()
                .spawn(async move {
                    let result = session.save_backup();
                    (session, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.view = session.view();
                this.session = Some(session);
                this.busy = false;
                match result {
                    Ok(path) => {
                        this.copies_folder = path.parent().map(PathBuf::from);
                        this.message = None;
                    }
                    Err(error) => {
                        this.stopped = true;
                        this.message =
                            Some(format!("Your firmware copy could not be saved. {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if !path.is_dir() {
                        return Err("The saved backup folder is unavailable.".to_string());
                    }
                    std::process::Command::new("/usr/bin/open")
                        .arg(&path)
                        .status()
                        .map_err(|e| e.to_string())
                        .and_then(|status| {
                            if status.success() {
                                Ok(())
                            } else {
                                Err("Finder could not open the backup folder.".into())
                            }
                        })
                })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.message = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Focusable for Companion {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Companion {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if self.completed {
            "Your firmware copies are saved".into()
        } else if self.stopped {
            "Let’s reconnect your keyboard".into()
        } else if self.busy {
            "Saving your firmware copy".into()
        } else {
            self.view.title.clone()
        };
        let instruction = if self.completed {
            "Both firmware copies are saved privately on this Mac.".into()
        } else if self.stopped {
            self.message.clone().unwrap_or_default()
        } else if self.busy {
            "Keep the cable connected for a moment.".into()
        } else {
            self.view.instruction.clone()
        };
        div()
            .id("companion")
            .track_focus(&self.focus_handle)
            .moves_focus_on_tab()
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x23262b))
            .font_family(".AppleSystemUIFont")
            .text_size(px(15.))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(500.))
                    .p(px(32.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(16.))
                    .child(keyboard_picture(self.role))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(27.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .line_height(px(23.))
                            .child(instruction),
                    )
                    .when(self.view.needs_power_on_ack && !self.stopped, |column| {
                        column.child(
                            button("power-on", "It’s switched on")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(session) = this.session.as_mut() {
                                        session.confirm_power_on();
                                        this.view = session.view();
                                        cx.notify();
                                    }
                                })),
                        )
                    })
                    .when(self.stopped, |column| {
                        column.child(
                            button("retry", "Try again")
                                .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                        )
                    })
                    .when(self.completed, |column| {
                        column.when_some(self.copies_folder.clone(), |column, path| {
                            column.child(button("open-copies", "Open copies folder").on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.open_folder(path.clone(), cx)
                                }),
                            ))
                        })
                    })
                    .when(self.completed, |column| {
                        column.when_some(self.message.clone(), |column, message| {
                            column.child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(19.))
                                    .text_center()
                                    .child(message),
                            )
                        })
                    })
                    .child(
                        div()
                            .pt(px(20.))
                            .text_size(px(12.))
                            .text_color(rgb(0x626870))
                            .child("Your keyboard won’t be changed."),
                    ),
            )
    }
}

// An orientation drawing, deliberately separate from any simulated device status.
fn keyboard_picture(role: Option<Role>) -> impl IntoElement {
    let rows: &[&[&str]] = match role {
        Some(Role::Right) => &[
            &["F7", "F8", "F9", "F10", "F11", "F12"],
            &["7", "8", "9", "0", "−", "="],
            &["Y", "U", "I", "O", "P", "["],
            &["H", "J", "K", "L", ";", "Enter"],
            &["N", "M", ",", ".", "/", "Shift"],
        ],
        _ => &[
            &["Esc", "F1", "F2", "F3", "F4", "F5"],
            &["`", "1", "2", "3", "4", "5"],
            &["Tab", "Q", "W", "E", "R", "T"],
            &["Caps", "A", "S", "D", "F", "G"],
            &["Shift", "Z", "X", "C", "V", "B"],
        ],
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .py(px(12.))
        .child(
            div()
                .bg(rgb(0xf1f3f5))
                .rounded(px(14.))
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .children(rows.iter().map(|row| {
                    div().flex().gap(px(5.)).children(row.iter().map(|key| {
                        div()
                            .w(px(36.))
                            .h(px(25.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0xe3e6ea))
                            .rounded(px(5.))
                            .text_size(px(11.))
                            .child(*key)
                    }))
                })),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(0x626870))
                .child(match role {
                    Some(Role::Left) => "Left half",
                    Some(Role::Right) => "Right half",
                    None => "",
                }),
        )
}
