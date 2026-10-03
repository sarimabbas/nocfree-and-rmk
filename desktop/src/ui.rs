//! Presentation only. Discovery and backup decisions remain in the session.
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crate::{
    battery,
    device::{self, Role},
    session::{Session, View},
    trial::{Trial, TrialView},
};
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, Image, ImageFormat, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task, Window, div, img,
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
    started: bool,
    trial_available: bool,
    trial: Option<Trial>,
    trial_view: Option<TrialView>,
    busy: bool,
    left_backup: Option<PathBuf>,
    copies_folder: Option<PathBuf>,
    completed: bool,
    stopped: bool,
    message: Option<String>,
    battery_text: Option<String>,
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
        let session = Session::new();
        let view = session.view();
        let poll = cx.spawn(async move |this, cx| {
            let available = cx
                .background_executor()
                .spawn(async { Trial::available() })
                .await;
            if this
                .update(cx, |this, cx| {
                    this.trial_available = available;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            let mut battery_checked = None;
            loop {
                let idle = this.update(cx, |this, _| !this.started);
                if matches!(idle, Ok(true))
                    && battery_checked
                        .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(30))
                {
                    let readings = cx
                        .background_executor()
                        .spawn(async { battery::read() })
                        .await;
                    battery_checked = Some(Instant::now());
                    if this
                        .update(cx, |this, cx| {
                            if !this.started {
                                this.battery_text = readings.ok().map(|r| {
                                    let level = |v: Option<u8>| {
                                        v.map_or("Unavailable".into(), |v| format!("{v}%"))
                                    };
                                    let right = if r.right_connected {
                                        level(r.right)
                                    } else {
                                        "Disconnected".into()
                                    };
                                    format!(
                                        "Estimated battery · Left {} · Right {right}",
                                        level(r.left)
                                    )
                                });
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                let state = this.update(cx, |this, cx| {
                    let trial = this.trial.take();
                    if trial.is_some() {
                        cx.notify();
                    }
                    (
                        this.started,
                        this.completed,
                        this.trial_view.is_some(),
                        trial,
                    )
                });
                let trial = match state {
                    Ok((_, true, _, _)) | Err(_) => break,
                    Ok((false, false, _, _)) | Ok((true, false, true, None)) => {
                        let timer = cx.background_executor().timer(Duration::from_secs(1));
                        timer.await;
                        continue;
                    }
                    Ok((true, false, _, trial)) => trial,
                };
                if let Some(mut trial) = trial {
                    let trial = cx
                        .background_executor()
                        .spawn(async move {
                            trial.observe(device::discover());
                            trial
                        })
                        .await;
                    let running = this.update(cx, |this, cx| {
                        this.accept_trial(trial);
                        cx.notify();
                        !this.completed
                    });
                    if !matches!(running, Ok(true)) {
                        break;
                    }
                    let timer = cx.background_executor().timer(Duration::from_secs(1));
                    timer.await;
                    continue;
                }
                let result = cx
                    .background_executor()
                    .spawn(async { device::discover() })
                    .await;
                let running = this.update(cx, |this, cx| {
                    if !this.started || this.completed {
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
            role: None,
            started: false,
            trial_available: false,
            trial: None,
            trial_view: None,
            busy: false,
            left_backup: None,
            copies_folder: None,
            completed: false,
            stopped: false,
            message: None,
            battery_text: None,
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    fn accept_trial(&mut self, trial: Trial) {
        let view = trial.view();
        self.completed = view.finished;
        self.trial_view = Some(view);
        self.trial = Some(trial);
    }

    fn start_trial(&mut self, cx: &mut Context<Self>) {
        if self.started || self.busy {
            return;
        }
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async { Trial::new() }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(trial) => {
                        this.accept_trial(trial);
                        this.role = Some(Role::Left);
                        this.started = true;
                        this.message = None;
                    }
                    Err(error) => this.message = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn acknowledge_trial(&mut self, cx: &mut Context<Self>) {
        let Some(mut trial) = self.trial.take() else {
            return;
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let trial = cx
                .background_executor()
                .spawn(async move {
                    trial.acknowledge();
                    trial
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.accept_trial(trial);
                cx.notify();
            });
        })
        .detach();
    }

    fn start_copies(&mut self, cx: &mut Context<Self>) {
        if self.started || self.busy {
            return;
        }
        let mut session = Session::new();
        session.select(Role::Left);
        self.view = session.view();
        self.session = Some(session);
        self.role = Some(Role::Left);
        self.started = true;
        cx.notify();
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        if !self.started || self.busy || self.stopped || self.completed {
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
        if !self.started || !self.view.can_save || self.busy || self.stopped {
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
        let trial_view = self.trial_view.as_ref().map(|view| TrialView {
            title: view.title.clone(),
            instruction: view.instruction.clone(),
            ack_label: view.ack_label,
            finished: view.finished,
        });
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
            .when(!self.started, |root| {
                root.child(
                    div()
                        .w_full()
                        .max_w(px(500.))
                        .p(px(20.))
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .child(div().text_size(px(13.)).text_color(rgb(0x626870)).child(
                            self.battery_text.clone().unwrap_or_else(|| "Connect the left half by USB to see battery levels.".into())
                        ))
                        .child(
                            div()
                                .flex()
                                .justify_center()
                                .gap(px(12.))
                                .child(img(keyboard_image(Role::Left)).w(px(140.)).h(px(100.)))
                                .child(img(keyboard_image(Role::Right)).w(px(140.)).h(px(100.))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(20.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Back up your firmware"),
                                )
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .line_height(px(21.))
                                        .child("Save the firmware currently on each half."),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(rgb(0x626870))
                                        .child("Copies may include custom firmware. Restore is not available yet."),
                                )
                                .child(div().flex().pt(px(4.)).child(
                                    button("start-copies", "Save firmware copies").disabled(self.busy).on_click(
                                        cx.listener(|this, _, _, cx| this.start_copies(cx)),
                                    ),
                                )),
                        )
                        .child(
                            div()
                                .border_t_1()
                                .border_color(rgb(0xe3e6ea))
                                .pt(px(16.))
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(20.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Install RMK firmware"),
                                )
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .line_height(px(21.))
                                        .child("A factory backup comes first."),
                                )
                                .child(div().flex().pt(px(4.)).child(
                                    button("install-unavailable", "Coming soon").disabled(true),
                                ))
                                .when(self.trial_available, |section| section.child(
                                    div().pt(px(12.)).flex().flex_col().gap(px(8.))
                                        .child(div().text_size(px(13.)).text_color(rgb(0x626870))
                                            .child("Developer startup test"))
                                        .child(div().flex().child(
                                            button("start-trial", "Open test guide").disabled(self.busy).on_click(
                                                cx.listener(|this, _, _, cx| this.start_trial(cx)),
                                            ),
                                        )),
                                )),
                        )
                        .when_some(self.message.clone(), |column, message| column.child(
                            div().text_size(px(13.)).line_height(px(19.)).child(message)))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(0x626870))
                                .child("Saving copies won’t change your keyboard."),
                        ),
                )
            })
            .when_some(trial_view, |root, view| root.child(
                div().w_full().max_w(px(500.)).p(px(32.)).flex().flex_col()
                    .items_center().gap(px(16.))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x626870)).child("RMK startup test"))
                    .child(keyboard_picture(Some(Role::Left)))
                    .child(div().w_full().text_center().text_size(px(27.))
                        .font_weight(FontWeight::SEMIBOLD).child(view.title))
                    .child(div().w_full().text_center().line_height(px(23.)).child(view.instruction))
                    .when_some(view.ack_label, |column, label| column.child(
                        button("trial-acknowledge", label).disabled(self.trial.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.acknowledge_trial(cx)))))))
            .when(self.started && self.trial_view.is_none(), |root| {
                root.child(
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
            })
    }
}

// Photo-based orientation sketches; their appearance never represents device status.
fn keyboard_image(role: Role) -> Arc<Image> {
    static LEFT: OnceLock<Arc<Image>> = OnceLock::new();
    static RIGHT: OnceLock<Arc<Image>> = OnceLock::new();
    let sketch = match role {
        Role::Right => RIGHT.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../assets/nocfree-right.svg").to_vec(),
            ))
        }),
        _ => LEFT.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../assets/nocfree-left.svg").to_vec(),
            ))
        }),
    };
    sketch.clone()
}

fn keyboard_picture(role: Option<Role>) -> impl IntoElement {
    let sketch = keyboard_image(role.unwrap_or(Role::Left));
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .py(px(8.))
        .child(img(sketch.clone()).w(px(320.)).h(px(230.)))
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
