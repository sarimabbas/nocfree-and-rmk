//! Presentation only. Discovery and backup decisions remain in the session.
use std::{
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use crate::flow_presentation::{self, FlowProgress};
use crate::{
    firmware_journey::{FirmwareJourney, View as FirmwareView},
    release::FirmwareRelease,
};
use crate::{
    recovery_journey::{Attempt, RecoveryJourney, State as RecoveryState},
    runtime_recovery::Role as RecoveryRole,
};
use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use rynk::rmk_types::battery::BatteryStatus;

use crate::{
    battery,
    device::{self, Role},
    home::{Home, UpdateAssessment},
    journey::Journey,
    recovery,
    session::View,
};
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, Image, ImageFormat, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task, Window, div, img,
    prelude::FluentBuilder, px,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme,
    button::{Button, ButtonVariants},
    sidebar::{Sidebar, SidebarGroup, SidebarMenuItem},
    spinner::Spinner,
    stepper::{Stepper, StepperItem},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Backups,
    Recovery,
    Firmware,
}

pub struct Companion {
    page: Page,
    appearance_subscription: Option<gpui::Subscription>,
    dongle_connected: bool,
    bluetooth_connected: bool,
    rescue: RecoveryJourney,
    rescue_cancel: Option<Arc<AtomicBool>>,
    backup_recovery: bool,
    session: Option<Journey>,
    view: View,
    role: Option<Role>,
    started: bool,
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
    firmware_versions: Vec<crate::firmware_version::Observation>,
    home: Home,
    firmware: Option<FirmwareJourney>,
    firmware_view: Option<FirmwareView>,
    recovery_locations: [Option<u64>; 3],
    focus_handle: FocusHandle,
    _poll: Task<()>,
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
        Theme::sync_system_appearance(None, cx);
        let session = Journey::backup();
        let view = session.view();
        let poll = cx.spawn(async move |this, cx| {
            let mut battery_checked = None;
            let mut version_checked: Option<Instant> = None;
            let mut bluetooth_checked: Option<Instant> = None;
            type BatteryConnection = (u64, Home, Vec<(u64, u64, u64, String)>);
            type BatteryQuery = (
                Instant,
                BatteryConnection,
                Task<Result<battery::Readings, String>>,
            );
            let mut battery_query: Option<BatteryQuery> = None;
            loop {
                if bluetooth_checked.is_none_or(|t| t.elapsed() >= Duration::from_secs(10)) {
                    let connected = cx
                        .background_executor()
                        .spawn(async { device::bluetooth_connected() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.bluetooth_connected = connected;
                        cx.notify();
                    });
                    bluetooth_checked = Some(Instant::now());
                }
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
                                    this.firmware_versions.clear();
                                    version_checked = None;
                                    this.device_key = key;
                                    this.device_generation = this.device_generation.wrapping_add(1);
                                }
                                this.dongle_connected =
                                    observation.as_ref().is_ok_and(|snapshot| {
                                        snapshot.devices.iter().any(|d| {
                                            d.vendor == 0x4c4b
                                                && ((d.product == 0x4643
                                                    && d.name == "NocFree AND RMK Receiver")
                                                    || (d.product == 0x4644
                                                        && d.name == "NocFree RMK Receiver"))
                                        })
                                    });
                                if this.page == Page::Firmware
                                    && !this.busy
                                    && let Some(journey) = this.firmware.as_mut()
                                {
                                    journey.observe(observation.clone());
                                    this.firmware_view = Some(journey.view());
                                }
                                let next = Home::observe(observation, UpdateAssessment::Unknown);
                                if next != this.home {
                                    this.battery_readings = None;
                                    this.battery_observed = None;
                                }
                                this.home = next;
                                this.advance_firmware(cx);
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                // A native HID write can stall inside the OS. Observe its task
                // without stopping discovery; late results cannot repopulate a
                // timed-out or replaced connection. Battery keeps one worker.
                if let Some((started, _, task)) = battery_query.as_mut() {
                    let result = tokio::select! {
                        biased;
                        result = task => Some(result),
                        _ = cx.background_executor().timer(Duration::ZERO) => None,
                    };
                    if let Some(result) =
                        battery::observation_result(*started, Instant::now(), result)
                    {
                        let (started, key, _) =
                            battery_query.take().expect("pending battery query");
                        if this
                            .update(cx, |this, cx| {
                                if this.started && !this.completed {
                                    return;
                                }
                                if key
                                    != (this.device_generation, this.home, this.device_key.clone())
                                {
                                    this.battery_readings = None;
                                    this.battery_observed = None;
                                    cx.notify();
                                    return;
                                }
                                match result {
                                    Ok(readings) => {
                                        this.battery_readings = Some(readings);
                                        this.battery_observed = Some(started);
                                        this.battery_error = None;
                                    }
                                    Err(error) => {
                                        this.battery_readings = None;
                                        this.battery_observed = None;
                                        this.battery_error = Some(error);
                                    }
                                }
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                let version_idle = this.update(cx, |this, _| {
                    (!this.started || this.completed)
                        && !this.busy
                        && this.page != Page::Firmware
                        && !matches!(
                            this.rescue.state(),
                            RecoveryState::Identify(_) | RecoveryState::Guiding(_, _)
                        )
                });
                if matches!(version_idle, Ok(true))
                    && version_checked.is_none_or(|t| t.elapsed() >= Duration::from_secs(30))
                    && let Ok((generation, key)) = this.update(cx, |this, _| {
                        (this.device_generation, this.device_key.clone())
                    })
                {
                    let versions = cx
                        .background_executor()
                        .spawn(async { crate::firmware_version::read() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.device_generation == generation
                            && this.device_key == key
                            && (!this.started || this.completed)
                            && !this.busy
                            && this.page != Page::Firmware
                            && !matches!(
                                this.rescue.state(),
                                RecoveryState::Identify(_) | RecoveryState::Guiding(_, _)
                            )
                        {
                            this.firmware_versions = versions
                                .into_iter()
                                .filter(|v| {
                                    key.iter()
                                        .any(|(location, _, _, _)| *location == v.location)
                                })
                                .collect();
                            cx.notify();
                        }
                    });
                    version_checked = Some(Instant::now());
                }
                let battery_idle = this.update(cx, |this, _| {
                    (!this.started || this.completed)
                        && this.page != Page::Firmware
                        && (matches!(this.home, Home::Rmk(_)) || this.dongle_connected)
                        && !matches!(
                            this.rescue.state(),
                            RecoveryState::Identify(_) | RecoveryState::Guiding(_, _)
                        )
                });
                if matches!(battery_idle, Ok(true))
                    && battery_query.is_none()
                    && battery_checked
                        .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(30))
                    && let Ok(key) = this.update(cx, |this, _| {
                        (this.device_generation, this.home, this.device_key.clone())
                    })
                {
                    let started = Instant::now();
                    battery_query = Some((
                        started,
                        key,
                        cx.background_executor().spawn(async { battery::read() }),
                    ));
                    battery_checked = Some(started);
                }
                let state = this.update(cx, |this, _| this.started && !this.completed);
                match state {
                    Err(_) => break,
                    Ok(false) => {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        continue;
                    }
                    Ok(true) => {}
                }
                let result = cx
                    .background_executor()
                    .spawn(async { device::discover() })
                    .await;
                let running = this.update(cx, |this, cx| {
                    if !this.started || this.completed {
                        return false;
                    }
                    // Status remains live while the backup state machine owns the guide.
                    let mut key = result
                        .as_ref()
                        .map(|snapshot| {
                            snapshot
                                .devices
                                .iter()
                                .map(|d| (d.location, d.vendor, d.product, d.name.clone()))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    key.sort();
                    if key != this.device_key {
                        this.firmware_versions.clear();
                        version_checked = None;
                        this.device_key = key;
                        this.device_generation = this.device_generation.wrapping_add(1);
                        this.battery_readings = None;
                        this.battery_observed = None;
                    }
                    if !this.backup_recovery
                        && let Some(session) = this.session.as_mut()
                    {
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
            page: Page::Backups,
            appearance_subscription: None,
            dongle_connected: false,
            bluetooth_connected: false,
            rescue: RecoveryJourney::new(),
            rescue_cancel: None,
            backup_recovery: false,
            session: Some(session),
            view,
            role: None,
            started: false,
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
            firmware_versions: Vec::new(),
            home: Home::default(),
            firmware: None,
            firmware_view: None,
            recovery_locations: [None; 3],
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    fn start_recovery(&mut self, role: RecoveryRole, cx: &mut Context<Self>) {
        if let Some(attempt) = self.rescue.start(role) {
            self.run_recovery(role, attempt, false, cx);
        }
    }

    fn run_recovery(
        &mut self,
        role: RecoveryRole,
        attempt: Attempt,
        backup: bool,
        cx: &mut Context<Self>,
    ) {
        self.backup_recovery = backup;
        self.stopped = false;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.rescue_cancel = Some(cancelled.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let worker_cancel = cancelled.clone();
            let (progress_sender, progress_receiver) = mpsc::channel();
            let mut worker = cx
                .background_executor()
                .spawn(async move { recovery::run(role, worker_cancel, progress_sender) });
            loop {
                tokio::select! {
                    result = &mut worker => {
                        let _ = this.update(cx, |this, cx| {
                            if !this.rescue_cancel.as_ref().is_some_and(|current| Arc::ptr_eq(current, &cancelled))
                                || cancelled.load(Ordering::Relaxed)
                            { return; }
                            let result = result.and_then(|session| {
                                if let Ok((_, location, _)) = session.recovery_binding() { this.recovery_locations[role_index(role)] = Some(location); }
                                if !backup { return Ok(()); }
                                if this.started && this.page == Page::Backups
                                    && this.session.as_mut().is_some_and(|journey| journey.accept_recovery(session))
                                { Ok(()) } else { Err("This recovery result no longer belongs to the active backup.".into()) }
                            });
                            if this.rescue.complete(attempt, role, result.clone()) {
                                this.rescue_cancel = None;
                                if backup {
                                    this.backup_recovery = result.is_err();
                                    if let Err(error) = result {
                                        this.stopped = true;
                                        this.message = Some(error);
                                    } else { this.advance(cx); }
                                }
                                cx.notify();
                            }
                        });
                        break;
                    }
                    _ = cx.background_executor().timer(Duration::from_millis(100)) => {
                        while let Ok(procedure) = progress_receiver.try_recv() {
                            let _ = this.update(cx, |this, cx| {
                                if this.rescue_cancel.as_ref().is_some_and(|current| Arc::ptr_eq(current, &cancelled))
                                    && !cancelled.load(Ordering::Relaxed)
                                    && this.rescue.observe(attempt, role, procedure)
                                {
                                    cx.notify();
                                }
                            });
                        }
                    }
                }
            }
        })
        .detach();
    }

    fn cancel_recovery(&mut self) {
        if let Some(cancelled) = self.rescue_cancel.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.rescue.cancel();
        self.backup_recovery = false;
    }

    fn recovery_waiting(&self, label: &'static str, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(waiting_indicator(label, cx))
            .child(self.footer(None, cx))
    }
    fn footer(&self, next: Option<Button>, cx: &mut Context<Self>) -> gpui::Div {
        let cancellable = !self.busy
            && match self.page {
                Page::Firmware => self.firmware_view.as_ref().is_none_or(|v| !v.complete),
                Page::Backups => self.started && !self.completed,
                Page::Recovery => matches!(
                    self.rescue.state(),
                    RecoveryState::Identify(_)
                        | RecoveryState::Guiding(_, _)
                        | RecoveryState::Failed(_, _)
                ),
                Page::Home => false,
            };
        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .when(cancellable, |row| {
                row.child(
                    Button::new("cancel-journey")
                        .label("Cancel")
                        .secondary()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.page == Page::Recovery {
                                this.cancel_recovery();
                                cx.notify();
                            } else {
                                this.navigate(Page::Home, cx);
                            }
                        })),
                )
            })
            .child(div().flex_1())
            .when_some(next, |row, next| row.child(next))
    }
    fn backup_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        let title = if self.completed {
            "Your firmware copy is saved".to_owned()
        } else if self.stopped {
            "Let’s reconnect".to_owned()
        } else if self.busy {
            "Saving a copy…".to_owned()
        } else {
            self.view.title.clone()
        };
        let instruction = if self.completed {
            "Your firmware is saved locally.".to_owned()
        } else if self.stopped {
            self.message.clone().unwrap_or_default()
        } else if self.busy {
            "Keep USB connected.".to_owned()
        } else {
            self.view.instruction.clone()
        };
        let mut screen = recovery_guide(
            self.role.map(|r| {
                if r == Role::Left {
                    RecoveryRole::Left
                } else {
                    RecoveryRole::Right
                }
            }),
            title,
            instruction,
            None,
            cx,
        );
        let next = if self.completed {
            Some(
                button("backup-done", "Next").on_click(cx.listener(|this, _, _, cx| {
                    this.started = false;
                    this.completed = false;
                    this.navigate(Page::Home, cx);
                })),
            )
        } else if self.stopped {
            Some(
                button("retry-backup", "Next")
                    .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
            )
        } else if self.view.needs_power_on_ack && !self.busy {
            Some(
                button("power-on", "Next").on_click(cx.listener(|this, _, _, cx| {
                    if let Some(journey) = this.session.as_mut() {
                        journey.confirm_power_on();
                        this.view = journey.view();
                        cx.notify();
                    }
                })),
            )
        } else {
            None
        };
        if next.is_none() {
            screen = screen.child(waiting_indicator(
                if self.busy {
                    "Saving your firmware copy…"
                } else {
                    "Waiting for the keyboard…"
                },
                cx,
            ));
        }
        screen.child(self.footer(next, cx))
    }

    fn recovery_card(
        &self,
        role: RecoveryRole,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        let artwork = match role {
            RecoveryRole::Left => img(keyboard_image(Role::Left))
                .w(px(140.))
                .h(px(100.))
                .into_any_element(),
            RecoveryRole::Right => img(keyboard_image(Role::Right))
                .w(px(140.))
                .h(px(100.))
                .into_any_element(),
            RecoveryRole::Receiver => div()
                .w(px(140.))
                .h(px(100.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(IconName::Usb)
                        .size(px(44.))
                        .text_color(cx.theme().muted_foreground),
                )
                .into_any_element(),
        };
        Button::new(match role {
            RecoveryRole::Left => "recover-left",
            RecoveryRole::Right => "recover-right",
            RecoveryRole::Receiver => "recover-receiver",
        })
        .secondary()
        .outline()
        .flex_1()
        .h(px(170.))
        .accessibility_label(label)
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(14.))
                .child(artwork)
                .child(div().font_weight(FontWeight::MEDIUM).child(label)),
        )
        .on_click(cx.listener(move |this, _, _, cx| this.start_recovery(role, cx)))
    }

    fn recovery_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        match self.rescue.state() {
            RecoveryState::Choose => div()
                .flex()
                .flex_col()
                .gap(px(24.))
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .text_color(cx.theme().muted_foreground)
                        .child("Choose the part you want to recover"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(12.))
                        .child(self.recovery_card(RecoveryRole::Left, "Left half", cx))
                        .child(self.recovery_card(RecoveryRole::Right, "Right half", cx))
                        .child(self.recovery_card(RecoveryRole::Receiver, "USB dongle", cx)),
                ),
            RecoveryState::Identify(role) => recovery_guide(
                Some(*role),
                "Connect your device",
                crate::recovery_journey::instruction(*role),
                Some(
                    self.recovery_waiting("Checking its firmware…", cx)
                        .into_any_element(),
                ),
                cx,
            ),
            RecoveryState::Guiding(role, procedure) => recovery_guide(
                Some(*role),
                "Open the recovery drive",
                procedure.instruction(*role),
                Some(
                    self.recovery_waiting("Waiting for the recovery drive…", cx)
                        .into_any_element(),
                ),
                cx,
            ),
            RecoveryState::Ready(role) => recovery_guide(
                Some(*role),
                "Recovery drive is ready",
                "The recovery drive is open. Your firmware hasn’t been changed.",
                Some(
                    (self.footer(
                        Some(button("recovery-done", "Next").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.cancel_recovery();
                                this.page = Page::Home;
                                cx.notify();
                            },
                        ))),
                        cx,
                    ))
                    .into_any_element(),
                ),
                cx,
            ),
            RecoveryState::Failed(role, error) => recovery_guide(
                Some(*role),
                "Let’s try again",
                error.clone(),
                Some(
                    self.footer(
                        Some(button("retry-recovery", "Next").on_click(cx.listener(
                            |this, _, _, cx| {
                                if this.page == Page::Firmware {
                                    this.cancel_recovery();
                                    this.message = None;
                                    this.advance_firmware(cx);
                                    return;
                                }
                                if let Some((role, attempt)) = this.rescue.retry() {
                                    this.run_recovery(role, attempt, this.backup_recovery, cx);
                                }
                            },
                        ))),
                        cx,
                    )
                    .into_any_element(),
                ),
                cx,
            ),
        }
    }

    fn start_firmware(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.navigate(Page::Firmware, cx);
        self.cancel_recovery();
        self.message = None;
        self.firmware = None;
        self.firmware_view = None;
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { FirmwareRelease::bundled() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(release) => {
                        let journey = FirmwareJourney::new(release);
                        this.firmware_view = Some(journey.view());
                        this.firmware = Some(journey);
                        this.advance_firmware(cx);
                    }
                    Err(error) => this.message = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn firmware_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&mut FirmwareJourney) -> Result<(), String> + Send + 'static,
    ) {
        let Some(mut journey) = self.firmware.take() else {
            return;
        };
        self.busy = true;
        self.message = None;
        cx.spawn(async move |this, cx| {
            let (journey, result) = cx
                .background_executor()
                .spawn(async move {
                    let result = work(&mut journey);
                    (journey, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.firmware_view = Some(journey.view());
                this.firmware = Some(journey);
                this.message = result.err();
                this.advance_firmware(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn advance_firmware(&mut self, cx: &mut Context<Self>) {
        if self.page != Page::Firmware
            || self.busy
            || self.message.is_some()
            || self.rescue_cancel.is_some()
        {
            return;
        }
        let Some(view) = &self.firmware_view else {
            return;
        };
        if !view.complete && view.needs_recovery {
            let role = view.role;
            let verification = view.verification;
            self.cancel_recovery();
            let Some(attempt) = self.rescue.start(role) else {
                return;
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            self.rescue_cancel = Some(cancelled.clone());
            cx.spawn(async move |this, cx| {
                let worker_cancel = cancelled.clone();
                let (send, receive) = mpsc::channel();
                let mut worker = cx.background_executor().spawn(async move { recovery::run(role, worker_cancel, send) });
                loop {
                    tokio::select! {
                        result = &mut worker => {
                            let _ = this.update(cx, |this, cx| {
                                if this.page != Page::Firmware || !this.rescue_cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancelled)) || cancelled.load(Ordering::Relaxed) { return; }
                                this.rescue_cancel = None;
                                match result {
                                    Ok(session) => {
                                        if let Ok((_, location, _)) = session.recovery_binding() { this.recovery_locations[role_index(role)] = Some(location); }
                                        this.rescue.complete(attempt, role, Ok(()));
                                        this.firmware_job(cx, move |journey| if verification { journey.accept_verification(session) } else { journey.accept_recovery(session) });
                                    }
                                    Err(error) => {
                                        this.rescue.complete(attempt, role, Err(error.clone()));
                                        this.message = Some(error);
                                    }
                                }
                                cx.notify();
                            });
                            break;
                        }
                        _ = cx.background_executor().timer(Duration::from_millis(100)) => {
                            while let Ok(procedure) = receive.try_recv() {
                                let _ = this.update(cx, |this, cx| {
                                    if this.rescue_cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancelled)) && !cancelled.load(Ordering::Relaxed) {
                                        this.rescue.observe(attempt, role, procedure);
                                        cx.notify();
                                    }
                                });
                            }
                        }
                    }
                }
            }).detach();
        }
    }

    fn firmware_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        if let Some(error) = &self.message {
            return recovery_guide(
                self.firmware_view.as_ref().map(|v| v.role),
                "Let’s reconnect",
                error.clone(),
                Some(
                    self.footer(
                        Some(
                            button("check-firmware-again", "Next")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.message = None;
                                    if this.firmware.is_none() {
                                        this.start_firmware(cx);
                                    } else {
                                        this.advance_firmware(cx);
                                    }
                                    cx.notify();
                                })),
                        ),
                        cx,
                    )
                    .into_any_element(),
                ),
                cx,
            );
        }
        if self.rescue_cancel.is_some() {
            return self.recovery_screen(cx);
        }
        let Some(view) = &self.firmware_view else {
            return recovery_guide(
                None,
                "Preparing your firmware",
                "Finding the verified release for your keyboard.",
                Some(waiting_indicator("Checking firmware…", cx).into_any_element()),
                cx,
            );
        };
        let mut screen = recovery_guide(
            Some(view.role),
            if self.busy {
                "Checking your firmware".to_owned()
            } else {
                view.title.clone()
            },
            if self.busy {
                "Keep USB connected. We’ll continue automatically.".to_owned()
            } else {
                view.instruction.clone()
            },
            None,
            cx,
        );
        if self.busy {
            screen = screen.child(waiting_indicator("Working…", cx));
        } else if view.can_transfer {
            screen = screen.child(self.footer(
                Some(
                    button("install-part", "Install firmware").on_click(cx.listener(
                        |this, _, _, cx| this.firmware_job(cx, |journey| journey.transfer()),
                    )),
                ),
                cx,
            ));
        } else if view.needs_power_on_ack {
            screen = screen.child(self.footer(
                Some(button("right-switched-on", "Next").on_click(cx.listener(
                    |this, _, _, cx| {
                        if let Some(journey) = this.firmware.as_mut() {
                            journey.confirm_power_on();
                            this.firmware_view = Some(journey.view());
                        }
                        cx.notify();
                    },
                ))),
                cx,
            ));
        } else if view.complete {
            screen = screen.child(
                self.footer(
                    Some(
                        button("firmware-done", "Next")
                            .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Home, cx))),
                    ),
                    cx,
                ),
            );
        } else {
            screen = screen
                .child(waiting_indicator("Waiting for your keyboard…", cx))
                .child(self.footer(None, cx));
        }
        screen
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
            self.advance(cx);
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
        self.advance(cx);
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
        if self
            .session
            .as_ref()
            .is_some_and(|j| j.state() == crate::journey::State::Guiding)
            && !self.backup_recovery
        {
            self.cancel_recovery();
            let role = if self.role == Some(Role::Right) {
                RecoveryRole::Right
            } else {
                RecoveryRole::Left
            };
            if let Some(attempt) = self.rescue.start(role) {
                self.run_recovery(role, attempt, true, cx);
            }
            return;
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
}

impl Focusable for Companion {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Companion {
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
        if (self.page == Page::Recovery && page != Page::Recovery)
            || (self.backup_recovery && page != Page::Backups)
            || (self.page == Page::Firmware && page != Page::Firmware)
        {
            self.cancel_recovery();
        }
        if self.page == Page::Firmware
            && page != Page::Firmware
            && let Some(journey) = self.firmware.as_mut()
            && !journey.view().complete
        {
            journey.cancel();
        }
        self.page = page;
        if page == Page::Backups
            && self
                .session
                .as_ref()
                .is_some_and(|j| j.state() == crate::journey::State::Paused)
        {
            self.start_copies(cx);
        }
        cx.notify();
    }

    fn nav_row(
        &self,
        label: &'static str,
        page: Page,
        icon: IconName,
        cx: &mut Context<Self>,
    ) -> SidebarMenuItem {
        SidebarMenuItem::new(label)
            .icon(icon)
            .active(self.page == page || (page == Page::Backups && self.page == Page::Home))
            .disable(self.busy)
            .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx)))
    }
}

impl Render for Companion {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.appearance_subscription.is_none() {
            Theme::sync_system_appearance(Some(window), cx);
            self.appearance_subscription =
                Some(cx.observe_window_appearance(window, |_, window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    cx.notify();
                }));
        }
        let mut tasks = SidebarGroup::new("Tasks");
        let firmware_action = self.home.action();
        if let Some(label) = firmware_action {
            tasks = tasks.child(
                SidebarMenuItem::new(label)
                    .icon(IconName::Download)
                    .active(self.page == Page::Firmware)
                    .disable(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.start_firmware(cx))),
            );
        }
        if self.home.can_restore() {
            tasks = tasks.child(
                SidebarMenuItem::new("Restore factory")
                    .icon(IconName::Undo)
                    .disable(true),
            );
        }
        let navigation = Sidebar::new("navigation")
            .w(px(220.))
            .collapsible(false)
            .header(
                div()
                    .px(px(12.))
                    .py(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("NocFree RMK Companion"),
            )
            .child(tasks)
            .child(
                SidebarGroup::new("Additional utilities")
                    .child(self.nav_row(
                        "Backup firmware",
                        Page::Backups,
                        IconName::HardDriveDownload,
                        cx,
                    ))
                    .child(self.nav_row(
                        "Start Recovery Mode",
                        Page::Recovery,
                        IconName::HeartPulse,
                        cx,
                    )),
            );

        let mut canvas = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(620.))
            .gap(px(24.))
            .child(
                div()
                    .text_size(px(23.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match self.page {
                        Page::Backups | Page::Home => "Backup firmware",
                        Page::Recovery => "Start Recovery Mode",
                        Page::Firmware => "RMK firmware",
                    }),
            );
        match self.page {
            Page::Backups => {
                if let Some(journey) = &self.session {
                    canvas = canvas.child(flow_indicator(
                        flow_presentation::backup(journey),
                        "backup-steps",
                        cx,
                    ));
                }
            }
            Page::Recovery => {
                canvas = canvas.child(flow_indicator(
                    flow_presentation::recovery(self.rescue.state()),
                    "recovery-steps",
                    cx,
                ));
            }
            Page::Home => {}
            Page::Firmware => {
                canvas = canvas.child(flow_indicator(
                    FlowProgress {
                        labels: ["USB dongle", "Right half", "Left half"],
                        current: self
                            .firmware_view
                            .as_ref()
                            .map_or(0, |v| if v.complete { 3 } else { v.step }),
                    },
                    "firmware-steps",
                    cx,
                ));
            }
        }
        canvas = match self.page {
            Page::Backups if self.backup_recovery => canvas.child(self.recovery_screen(cx)),
            Page::Backups if self.started || self.completed => canvas.child(self.backup_screen(cx)),
            Page::Backups | Page::Home => canvas
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .text_color(cx.theme().muted_foreground)
                        .child("Save a copy of the connected keyboard’s firmware locally."),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(12.))
                        .py(px(16.))
                        .child(img(keyboard_image(Role::Left)).w(px(160.)).h(px(115.)))
                        .child(img(keyboard_image(Role::Right)).w(px(160.)).h(px(115.))),
                )
                .child(
                    self.footer(
                        Some(
                            button("start-backup", "Next")
                                .on_click(cx.listener(|this, _, _, cx| this.start_copies(cx))),
                        ),
                        cx,
                    ),
                ),
            Page::Recovery => canvas.child(self.recovery_screen(cx)),
            Page::Firmware => canvas.child(self.firmware_screen(cx)),
        };
        let firmware = match self.home {
            Home::Factory => crate::firmware_version::label(true, &self.firmware_versions),
            Home::Rmk(_) => crate::firmware_version::label(false, &self.firmware_versions),
            _ if self.dongle_connected => {
                crate::firmware_version::label(false, &self.firmware_versions)
            }
            _ => "Firmware not detected".to_owned(),
        };
        let current = self
            .battery_observed
            .is_some_and(|t| t.elapsed() < Duration::from_secs(45));
        let readings = self.battery_readings.filter(|_| current);
        let level = |status| match status {
            BatteryStatus::Available { level, .. } => level,
            _ => None,
        };
        let usb = |product| {
            self.device_key
                .iter()
                .any(|(_, vendor, p, _)| *vendor == 0x4c4b && *p == product)
        };
        let recovering = |role| {
            self.recovery_locations[role_index(role)].is_some_and(|location| {
                self.device_key
                    .iter()
                    .any(|(l, v, p, _)| *l == location && *v == 0x239a && *p == 0x0029)
            })
        };
        let left_usb =
            usb(0x4643) && matches!(self.home, Home::Rmk(_)) || self.home == Home::Factory;
        let connection = if left_usb {
            crate::status_strip::Connection::Usb
        } else if self.bluetooth_connected {
            crate::status_strip::Connection::Bluetooth
        } else if self.dongle_connected && readings.is_some() {
            crate::status_strip::Connection::Dongle
        } else {
            crate::status_strip::Connection::Disconnected
        };
        let status = crate::status_strip::render(
            firmware,
            connection,
            crate::status_strip::Peripheral {
                level: readings.and_then(|r| level(r.left)),
                usb_connected: left_usb || recovering(RecoveryRole::Left),
                recovery: recovering(RecoveryRole::Left),
            },
            crate::status_strip::Peripheral {
                level: readings
                    .filter(|r| r.right_connected)
                    .and_then(|r| level(r.right)),
                usb_connected: usb(0x4671)
                    || recovering(RecoveryRole::Right)
                    || self
                        .device_key
                        .iter()
                        .any(|(_, v, p, _)| *v == 0x239a && *p == 0x80d8),
                recovery: recovering(RecoveryRole::Right),
            },
            self.dongle_connected,
            recovering(RecoveryRole::Receiver),
            cx,
        );
        div()
            .id("companion")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family(cx.theme().font_family.clone())
            .text_size(px(14.))
            .child(
                div().flex().flex_1().min_h_0().child(navigation).child(
                    div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("journey-canvas")
                                .flex_1()
                                .overflow_y_scroll()
                                .p(px(32.))
                                .child(canvas),
                        ),
                ),
            )
            .child(status)
    }
}

fn role_index(role: RecoveryRole) -> usize {
    match role {
        RecoveryRole::Left => 0,
        RecoveryRole::Right => 1,
        RecoveryRole::Receiver => 2,
    }
}

// Shared presentation for standalone recovery and recovery prerequisites in
// the backup journey. Callers supply instructions from their verified model.
fn recovery_guide(
    role: Option<RecoveryRole>,
    _title: impl Into<gpui::SharedString>,
    instruction: impl Into<gpui::SharedString>,
    controls: Option<gpui::AnyElement>,
    cx: &App,
) -> gpui::Div {
    let picture = match role {
        Some(RecoveryRole::Left) => keyboard_picture(Some(Role::Left), cx).into_any_element(),
        Some(RecoveryRole::Right) => keyboard_picture(Some(Role::Right), cx).into_any_element(),
        Some(RecoveryRole::Receiver) => div()
            .h(px(190.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(
                Icon::new(IconName::Usb)
                    .size(px(64.))
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child("USB dongle"),
            )
            .into_any_element(),
        None => div()
            .flex()
            .justify_center()
            .gap(px(16.))
            .py(px(12.))
            .child(img(keyboard_image(Role::Left)).w(px(180.)).h(px(130.)))
            .child(img(keyboard_image(Role::Right)).w(px(180.)).h(px(130.)))
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(24.))
        .child(picture)
        .child(
            div()
                .text_size(px(15.))
                .line_height(px(23.))
                .text_color(cx.theme().muted_foreground)
                .child(instruction.into()),
        )
        .when_some(controls, |guide, controls| guide.child(controls))
}

fn flow_indicator(flow: FlowProgress, id: &'static str, _cx: &App) -> gpui::Div {
    div().flex().flex_col().gap(px(12.)).child(
        Stepper::new(id)
            .small()
            .selected_index(flow.current)
            .disabled(true)
            .items(
                flow.labels
                    .into_iter()
                    .map(|label| StepperItem::new().child(label)),
            ),
    )
}

fn waiting_indicator(label: &'static str, cx: &App) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(13.))
        .text_color(cx.theme().muted_foreground)
        .child(Spinner::new().small())
        .child(label)
}

fn button(id: &'static str, label: impl Into<gpui::SharedString>) -> Button {
    Button::new(id).label(label).primary().small()
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

fn keyboard_picture(role: Option<Role>, cx: &App) -> impl IntoElement {
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
                .text_color(cx.theme().muted_foreground)
                .child(match role {
                    Some(Role::Left) => "Left half",
                    Some(Role::Right) => "Right half",
                    None => "",
                }),
        )
}
