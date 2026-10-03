//! Presentation only. Discovery and backup decisions remain in the session.
use std::{
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rynk::rmk_types::battery::{BatteryStatus, ChargeState};

use crate::{
    battery,
    device::{self, Role},
    home::{Home, UpdateAssessment},
    journey::Journey,
    recovery,
    session::View,
    trial::{Trial, TrialView},
};
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, Image, ImageFormat, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task, Window, div, img,
    prelude::FluentBuilder, px, rems, rgb,
};
use gpuikit::{
    a11y::FocusNavigation,
    elements::{button::button, separator::separator, sidebar::sidebar},
    icons::Icons,
    theme::{GlobalTheme, Theme, ThemeVariant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Keyboard,
    Backups,
    Developer,
}

pub struct Companion {
    page: Page,
    nav_focus: Vec<FocusHandle>,
    dongle_connected: bool,
    rescue: Option<RescueView>,
    rescue_cancel: Option<Arc<AtomicBool>>,
    session: Option<Journey>,
    view: View,
    role: Option<Role>,
    started: bool,
    trial_available: bool,
    trial: Option<Trial>,
    trial_view: Option<TrialView>,
    busy: bool,
    copies_folder: Option<PathBuf>,
    completed: bool,
    stopped: bool,
    message: Option<String>,
    battery_readings: Option<battery::Readings>,
    battery_observed: Option<Instant>,
    battery_error: Option<String>,
    device_key: Vec<(u64, u64, u64, String)>,
    device_generation: u64,
    home: Home,
    focus_handle: FocusHandle,
    _poll: Task<()>,
}

enum RescueView {
    Choose,
    Running(nocfree_companion::experimental_recovery::Role),
    Finished(Result<(), String>),
}

impl Drop for Companion {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.rescue_cancel {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

impl Companion {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut theme = Theme::new(
            "NocFree",
            ThemeVariant::Light,
            rgb(0xffffff).into(),
            rgb(0xffffff).into(),
            rgb(0xf5f5f4).into(),
            rgb(0xe3e6ea).into(),
            rgb(0x007aff).into(),
        );
        theme.controls.medium.height = rems(2.0);
        theme.controls.medium.padding_x = rems(0.875);
        theme.controls.medium.radius = rems(0.375);
        theme.controls.medium.text_size = rems(0.8125);
        theme.controls.medium.line_height = rems(1.125);
        theme.fg_muted_color = Some(rgb(0x626870).into());
        theme.button_bg_color = Some(rgb(0x007aff).into());
        theme.button_bg_hover_color = Some(rgb(0x0068da).into());
        theme.button_bg_active_color = Some(rgb(0x005bc2).into());
        cx.set_global(GlobalTheme(Arc::new(theme)));
        let session = Journey::backup();
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
                let idle = this.update(cx, |this, _| !this.started || this.completed);
                if matches!(idle, Ok(true)) {
                    let observation = cx
                        .background_executor()
                        .spawn(async { device::discover() })
                        .await;
                    if this
                        .update(cx, |this, cx| {
                            if !this.started || this.completed {
                                let mut key = observation
                                    .as_ref()
                                    .map(|s| {
                                        s.devices
                                            .iter()
                                            .map(|d| {
                                                (d.location, d.vendor, d.product, d.name.clone())
                                            })
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default();
                                key.sort();
                                if key != this.device_key {
                                    this.battery_readings = None;
                                    this.battery_observed = None;
                                    this.battery_error = None;
                                    this.device_key = key;
                                    this.device_generation = this.device_generation.wrapping_add(1);
                                }
                                this.dongle_connected =
                                    observation.as_ref().is_ok_and(|snapshot| {
                                        snapshot.devices.iter().any(|d| {
                                            d.vendor == 0x4c4b
                                                && d.product == 0x4643
                                                && d.name == "NocFree AND RMK Receiver"
                                        })
                                    });
                                let next = Home::observe(
                                    observation,
                                    UpdateAssessment::compare(None, None, false),
                                );
                                if next != this.home {
                                    this.battery_readings = None;
                                    this.battery_observed = None;
                                }
                                this.home = next;
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                let battery_idle = this.update(cx, |this, _| {
                    (!this.started || this.completed) && matches!(this.home, Home::Rmk(_))
                });
                if matches!(battery_idle, Ok(true))
                    && battery_checked
                        .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(30))
                {
                    let reading_key = this
                        .update(cx, |this, _| (this.device_generation, this.home))
                        .ok();
                    let reading_started = Instant::now();
                    let readings = cx
                        .background_executor()
                        .spawn(async { battery::read() })
                        .await;
                    let fresh_observation = cx
                        .background_executor()
                        .spawn(async { device::discover() })
                        .await;
                    let fresh_key = fresh_observation.as_ref().ok().map(|snapshot| {
                        let mut key = snapshot
                            .devices
                            .iter()
                            .map(|d| (d.location, d.vendor, d.product, d.name.clone()))
                            .collect::<Vec<_>>();
                        key.sort();
                        key
                    });
                    battery_checked = Some(Instant::now());
                    if this
                        .update(cx, |this, cx| {
                            if !this.started || this.completed {
                                if reading_key != Some((this.device_generation, this.home))
                                    || fresh_key.as_ref() != Some(&this.device_key)
                                {
                                    this.battery_readings = None;
                                    this.battery_observed = None;
                                    cx.notify();
                                    return;
                                }
                                match readings {
                                    Ok(readings) => {
                                        this.battery_readings = Some(readings);
                                        this.battery_observed = Some(reading_started);
                                        this.battery_error = None;
                                    }
                                    Err(error) => {
                                        this.battery_readings = None;
                                        this.battery_observed = None;
                                        this.battery_error = Some(error);
                                    }
                                }
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
                    Err(_) => break,
                    Ok((_, true, _, _)) => {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        continue;
                    }
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
                    if running.is_err() {
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
                if running.is_err() {
                    break;
                }
                let timer = cx.background_executor().timer(Duration::from_secs(1));
                timer.await;
            }
        });
        Self {
            page: Page::Keyboard,
            nav_focus: (0..3).map(|_| cx.focus_handle().tab_stop(true)).collect(),
            dongle_connected: false,
            rescue: None,
            rescue_cancel: None,
            session: Some(session),
            view,
            role: None,
            started: false,
            trial_available: false,
            trial: None,
            trial_view: None,
            busy: false,
            copies_folder: None,
            completed: false,
            stopped: false,
            message: None,
            battery_readings: None,
            battery_observed: None,
            battery_error: None,
            device_key: Vec::new(),
            device_generation: 0,
            home: Home::default(),
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    fn start_recovery(
        &mut self,
        role: nocfree_companion::experimental_recovery::Role,
        cx: &mut Context<Self>,
    ) {
        if !recovery::enabled() || matches!(self.rescue, Some(RescueView::Running(_))) {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.rescue_cancel = Some(cancelled.clone());
        self.rescue = Some(RescueView::Running(role));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let worker_cancel = cancelled.clone();
            let result = cx
                .background_executor()
                .spawn(async move { recovery::run(role, worker_cancel) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .rescue_cancel
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &cancelled))
                    && !cancelled.load(Ordering::Relaxed)
                {
                    this.rescue = Some(RescueView::Finished(result));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn recovery_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use nocfree_companion::experimental_recovery::Role as RecoveryRole;
        let (title, description) = match self.rescue.as_ref().unwrap() {
            RescueView::Choose => (
                "Recover a device",
                "Which part would you like to recover?".to_owned(),
            ),
            RescueView::Running(role) => (
                "Reconnect your device",
                recovery::instruction(*role).to_owned(),
            ),
            RescueView::Finished(Ok(())) => (
                "Recovery drive is ready",
                "Your firmware hasn’t been changed.".to_owned(),
            ),
            RescueView::Finished(Err(error)) => ("Let’s try again", error.clone()),
        };
        div()
            .size_full()
            .bg(rgb(0xffffff))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .max_w(px(420.))
                    .p(px(32.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(24.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(0x626870))
                            .child("Experimental recovery"),
                    )
                    .child(
                        div()
                            .text_size(px(28.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(div().text_center().line_height(px(24.)).child(description))
                    .when(matches!(self.rescue, Some(RescueView::Choose)), |column| {
                        column
                            .child(action_row(button("recover-left", "Left half").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.start_recovery(RecoveryRole::Left, cx)
                                }),
                            )))
                            .child(action_row(button("recover-right", "Right half").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.start_recovery(RecoveryRole::Right, cx)
                                }),
                            )))
                            .child(action_row(
                                button("recover-receiver", "USB receiver").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.start_recovery(RecoveryRole::Receiver, cx)
                                    },
                                )),
                            ))
                    })
                    .when(
                        matches!(self.rescue, Some(RescueView::Running(_))),
                        |column| {
                            column.child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(0x626870))
                                    .child("Waiting for the recovery drive…"),
                            )
                        },
                    )
                    .child(action_row(
                        button(
                            "close-recovery",
                            if matches!(self.rescue, Some(RescueView::Running(_))) {
                                "Cancel"
                            } else {
                                "Done"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(cancelled) = this.rescue_cancel.take() {
                                cancelled.store(true, Ordering::Relaxed);
                            }
                            this.rescue = None;
                            cx.notify();
                        })),
                    )),
            )
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
        if (self.started && !self.completed) || self.busy {
            return;
        }
        if let Some(journey) = self.session.as_mut()
            && journey.state() == crate::journey::State::Paused
        {
            journey.resume();
            self.view = journey.view();
            self.role = Some(journey.role());
            self.started = true;
            self.stopped = false;
            self.message = None;
            self.page = Page::Backups;
            cx.notify();
            return;
        }
        let session = Journey::backup();
        self.completed = false;
        self.stopped = false;
        self.message = None;
        self.view = session.view();
        self.session = Some(session);
        self.role = Some(Role::Left);
        self.started = true;
        self.page = Page::Backups;
        cx.notify();
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        if !self.started || self.busy || self.stopped || self.completed {
            return;
        }
        if let Some(journey) = self.session.as_ref() {
            self.role = Some(journey.role());
            self.completed = journey.is_complete();
            self.view = journey.view();
        }
        if self.view.can_save && !self.completed {
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
                this.role = Some(session.role());
                this.completed = session.is_complete();
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

impl Companion {
    fn battery_summary(&self, right: bool) -> String {
        if let (Some(readings), Some(observed)) = (&self.battery_readings, self.battery_observed)
            && observed.elapsed() < Duration::from_secs(45)
        {
            if right && !readings.right_connected {
                return "Not connected".into();
            }
            let status = if right { readings.right } else { readings.left };
            return match status {
                BatteryStatus::Available {
                    level,
                    charge_state,
                } => {
                    let mut text = level
                        .map(|v| format!("{v}% estimated"))
                        .unwrap_or_else(|| "Level unavailable".into());
                    if charge_state == ChargeState::Charging {
                        text.push_str(" · Charging");
                    }
                    text
                }
                BatteryStatus::Unavailable => "Battery unavailable".into(),
            };
        }
        if right {
            "Status unavailable".into()
        } else {
            self.home
                .connection_label()
                .unwrap_or("Not detected")
                .into()
        }
    }

    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.started && !self.completed && page != Page::Backups {
            if let Some(journey) = self.session.as_mut() {
                journey.pause();
            }
            self.started = false;
        }
        self.page = page;
        cx.notify();
    }

    fn nav_row(
        &self,
        id: &'static str,
        label: &'static str,
        page: Page,
        icon: gpui::Svg,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .track_focus(&self.nav_focus[page as usize])
            .tab_stop(true)
            .moves_focus_on_tab()
            .w_full()
            .h(px(36.))
            .px(px(10.))
            .rounded(px(7.))
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(13.))
            .bg(if self.page == page {
                rgb(0xe4eaf3)
            } else {
                rgb(0xf5f5f4)
            })
            .text_color(if self.page == page {
                rgb(0x164d91)
            } else {
                rgb(0x42464d)
            })
            .role(gpui::Role::Button)
            .aria_label(label)
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0xe8e9eb)))
            .focus(|s| s.bg(rgb(0xe4eaf3)).text_color(rgb(0x164d91)))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                    this.navigate(page, cx);
                    cx.stop_propagation();
                }
            }))
            .child(icon.size(px(17.)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx)))
    }
}

impl Render for Companion {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page_title = match self.page {
            Page::Keyboard => "Keyboard status",
            Page::Backups => "Backup firmware",
            Page::Developer => "Developer tools",
        };
        let navigation = sidebar("navigation")
            .label("NocFree Companion navigation")
            .width(rems(13.75))
            .never_overlay()
            .child(
                div()
                    .pt(px(18.))
                    .pb(px(24.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(Icons::keyboard().size(px(21.)).text_color(rgb(0x32629b)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("NocFree"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x747982))
                                    .child("Companion"),
                            ),
                    ),
            )
            .child(sidebar_label("Tasks"))
            .when_some(self.home.action(), |nav, label| {
                nav.child(unavailable_task(
                    label,
                    if self.home == Home::Rmk(UpdateAssessment::Available) {
                        Icons::update()
                    } else {
                        Icons::download()
                    },
                ))
            })
            .when(self.home.can_restore(), |nav| {
                nav.child(unavailable_task("Restore factory", Icons::reset()))
            })
            .child(div().h(px(18.)))
            .child(sidebar_label("Additional utilities"))
            .child(self.nav_row(
                "nav-backups",
                "Backup firmware",
                Page::Backups,
                Icons::archive(),
                cx,
            ))
            .child(self.nav_row(
                "nav-keyboard",
                "Keyboard status",
                Page::Keyboard,
                Icons::dashboard(),
                cx,
            ))
            .child(div().flex_1())
            .when(recovery::enabled(), |nav| {
                nav.child(self.nav_row(
                    "nav-developer",
                    "Developer tools",
                    Page::Developer,
                    Icons::gear(),
                    cx,
                ))
            })
            .child(
                div()
                    .px(px(10.))
                    .pb(px(10.))
                    .text_size(px(11.))
                    .text_color(rgb(0x737881))
                    .child(match self.home {
                        Home::Factory => "Running factory firmware",
                        Home::Rmk(_) => "Running RMK",
                        Home::Recovery => "Recovery mode",
                        Home::Connect => "Keyboard not detected",
                    }),
            );

        let mut canvas = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(620.))
            .gap(px(24.));
        if self.rescue.is_some() {
            canvas = canvas.child(self.recovery_screen(cx));
        } else if self.trial_view.is_some() {
            let view = self.trial_view.as_ref().unwrap();
            let label = view.ack_label;
            canvas = canvas
                .child(keyboard_picture(Some(Role::Left)))
                .child(
                    div()
                        .text_size(px(23.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(view.title.clone()),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .line_height(px(23.))
                        .child(view.instruction.clone()),
                )
                .when_some(label, |c, label| {
                    c.child(action_row(
                        button("trial-acknowledge", label)
                            .disabled(self.trial.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.acknowledge_trial(cx))),
                    ))
                });
        } else {
            canvas = match self.page {
                Page::Keyboard => canvas
                    .child(div().text_size(px(23.)).font_weight(FontWeight::SEMIBOLD).child(self.home.title()))
                    .child(div().text_size(px(14.)).text_color(rgb(0x626870)).line_height(px(22.)).child(self.home.description()))
                    .child(div().flex().gap(px(32.)).py(px(16.))
                        .child(half_status(Role::Left, self.battery_summary(false), match self.home { Home::Rmk(_) => "RMK firmware", Home::Factory => "Factory firmware", _ => "Firmware unknown" }))
                        .child(half_status(Role::Right, self.battery_summary(true), if self.battery_readings.as_ref().is_some_and(|r| r.right_connected) { "RMK firmware" } else { "Firmware unknown" })))
                    .child(separator())
                    .child(status_row("USB receiver", if self.dongle_connected { "RMK · Connected by USB" } else { "Not detected" }))
                    .when_some(self.battery_error.clone(), |c, text| c.child(div().text_size(px(12.)).line_height(px(18.)).text_color(rgb(0x626870)).child(text)))
                    .child(div().text_size(px(12.)).text_color(rgb(0x737881)).child("Battery levels are estimates.")),
                Page::Backups if self.started || self.completed => {
                    let title = if self.completed { if self.session.as_ref().is_some_and(|j| j.archives().len() == 1) { "Your copy is saved".into() } else { "Your copies are saved".into() } } else if self.stopped { "Let’s reconnect".into() } else if self.busy { "Saving a copy…".into() } else { self.view.title.clone() };
                    let instruction = if self.completed { "Your firmware copies are saved privately on this Mac.".into() } else if self.stopped { self.message.clone().unwrap_or_default() } else if self.busy { "Keep the USB cable connected.".into() } else { self.view.instruction.clone() };
                    canvas.child(keyboard_picture(self.role))
                        .child(div().text_size(px(23.)).font_weight(FontWeight::SEMIBOLD).child(title))
                        .child(div().text_size(px(15.)).line_height(px(23.)).text_color(rgb(0x626870)).child(instruction))
                        .when(self.view.needs_power_on_ack && !self.stopped && !self.completed, |c| c.child(action_row(button("power-on", "It’s switched on").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| {
                            if let Some(journey) = this.session.as_mut() { journey.confirm_power_on(); this.view = journey.view(); cx.notify(); }
                        })))))
                        .when(self.stopped, |c| c.child(action_row(button("retry", "Try again").on_click(cx.listener(|this, _, _, cx| this.retry(cx))))))
                        .when(self.completed, |c| c.when_some(self.copies_folder.clone(), |c, path| c.child(action_row(button("open-copies", "Show in Finder").on_click(cx.listener(move |this, _, _, cx| this.open_folder(path.clone(), cx)))))))
                        .when(self.completed, |c| c.child(action_row(button("new-backup", "Save new copies").on_click(cx.listener(|this, _, _, cx| this.start_copies(cx))))))
                        .when(!self.completed, |c| c.child(action_row(button("pause-backup", "Pause").disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Keyboard, cx))))))
                },
                Page::Backups => canvas
                    .child(div().text_size(px(23.)).font_weight(FontWeight::SEMIBOLD).child("Keep a copy of your firmware"))
                    .child(div().text_size(px(14.)).line_height(px(22.)).text_color(rgb(0x626870)).child("Save a private copy of the connected keyboard’s firmware before making changes."))
                    .child(div().flex().gap(px(12.)).py(px(16.)).child(img(keyboard_image(Role::Left)).w(px(160.)).h(px(115.))).child(img(keyboard_image(Role::Right)).w(px(160.)).h(px(115.))))
                    .child(action_row(button("start-backup", if self.session.as_ref().is_some_and(|j| j.state() == crate::journey::State::Paused) { "Resume backup" } else { "Save firmware copies" }).on_click(cx.listener(|this, _, _, cx| this.start_copies(cx)))))
                    .when_some(self.copies_folder.clone(), |c, path| c.child(action_row(button("browse-backups", "Show saved copies").on_click(cx.listener(move |this, _, _, cx| this.open_folder(path.clone(), cx))))))
                    .child(div().text_size(px(12.)).text_color(rgb(0x737881)).child("Saving a copy doesn’t change your keyboard.")),
                Page::Developer => canvas
                    .child(div().text_size(px(23.)).font_weight(FontWeight::SEMIBOLD).child("Developer tools"))
                    .child(div().text_size(px(14.)).text_color(rgb(0x626870)).child("Experimental recovery is available for this development session."))
                    .when(recovery::enabled(), |c| c.child(action_row(button("experimental-recovery", "Recover a device").on_click(cx.listener(|this, _, _, cx| { this.rescue = Some(RescueView::Choose); cx.notify(); })))))
                    .when(self.trial_available, |c| c.child(action_row(button("start-trial", "Open startup test").on_click(cx.listener(|this, _, _, cx| this.start_trial(cx)))))),
            };
        }
        div()
            .id("companion")
            .track_focus(&self.focus_handle)
            .moves_focus_on_tab()
            .size_full()
            .flex()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x23262b))
            .font_family(".AppleSystemUIFont")
            .text_size(px(14.))
            .child(navigation)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(64.))
                            .flex_none()
                            .px(px(32.))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(rgb(0xeeeeef))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(page_title),
                            ),
                    )
                    .child(
                        div()
                            .id("journey-canvas")
                            .flex_1()
                            .overflow_y_scroll()
                            .p(px(32.))
                            .child(canvas),
                    ),
            )
            .into_any_element()
    }
}

fn action_row(control: impl IntoElement) -> impl IntoElement {
    div().flex().items_center().gap(px(8.)).child(control)
}

fn sidebar_label(label: &'static str) -> impl IntoElement {
    div()
        .px(px(10.))
        .pb(px(6.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0x626870))
        .child(label)
}

fn unavailable_task(label: &'static str, icon: gpui::Svg) -> impl IntoElement {
    div()
        .h(px(42.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_color(rgb(0x626870))
        .child(icon.size(px(17.)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).child(label))
                .child(div().text_size(px(10.)).child("Not available yet")),
        )
}

fn half_status(role: Role, battery: String, firmware: &'static str) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(7.))
        .child(img(keyboard_image(role)).w(px(200.)).h(px(144.)))
        .child(
            div()
                .mt(px(6.))
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(if role == Role::Left {
                    "Left half"
                } else {
                    "Right half"
                }),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(rgb(0x626870))
                .child(battery),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(0x626870))
                .child(firmware),
        )
}

fn status_row(label: &'static str, value: impl Into<gpui::SharedString>) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .justify_between()
        .items_center()
        .py(px(3.))
        .child(div().text_size(px(14.)).child(label))
        .child(
            div()
                .text_size(px(13.))
                .text_color(rgb(0x626870))
                .child(value.into()),
        )
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
        .child(img(sketch.clone()).w(px(240.)).h(px(173.)))
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
