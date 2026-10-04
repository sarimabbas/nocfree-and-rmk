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

use crate::backup_flow::{Event as BackupEvent, State as BackupState};
use crate::dongle_pairing::{self, Journey as PairingJourney, State as PairingState};
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
    ActiveTheme, Disableable, Sizable, Theme,
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
    Pairing,
    Firmware,
}

struct JourneyScreen {
    body: gpui::Div,
    actions: Option<gpui::Div>,
}

pub struct Companion {
    page: Page,
    appearance_subscription: Option<gpui::Subscription>,
    dongle_connected: bool,
    bluetooth_connected: bool,
    pairing: PairingJourney,
    pairing_observation: Option<dongle_pairing::Observation>,
    pairing_generation: u64,
    rescue: RecoveryJourney,
    rescue_cancel: Option<Arc<AtomicBool>>,
    backup_state: BackupState,
    session: Option<Journey>,
    view: View,
    role: Option<Role>,
    busy: bool,
    copies_folder: Option<PathBuf>,
    message: Option<String>,
    battery_levels: battery::Levels,
    left_mode: Option<crate::device_status::Mode>,
    telemetry: Option<battery::Telemetry>,
    telemetry_seen: Option<Instant>,
    right_link_connected: bool,
    right_link_known: bool,
    battery_error: Option<String>,
    device_key: Vec<(u64, u64, u64, String)>,
    device_generation: u64,
    battery_generation: u64,
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
        self.pairing.cancel();
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
            type BatteryConnection = (
                u64,
                crate::device_status::UsbKey,
                crate::device_status::UsbKey,
            );
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
                let idle = this.update(cx, |this, _| !this.backup_state.active());
                if matches!(idle, Ok(true)) {
                    let (observation, recovery_locations) = cx
                        .background_executor()
                        .spawn(async {
                            (
                                device::discover(),
                                crate::status_cache::recovery_locations(),
                            )
                        })
                        .await;
                    if this
                        .update(cx, |this, cx| {
                            if !this.backup_state.active() {
                                this.recovery_locations = recovery_locations;
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
                                if this.observe_device_key(key) {
                                    battery_checked = None;
                                    version_checked = None;
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
                        let (_, key, _) = battery_query.take().expect("pending battery query");
                        if this
                            .update(cx, |this, cx| {
                                if this.backup_state.active() {
                                    return;
                                }
                                if key.0 != this.battery_generation
                                    || key.1
                                        != crate::device_status::battery_source(&this.device_key)
                                {
                                    return;
                                }
                                match result {
                                    Ok(readings) => {
                                        let readings = readings
                                            .retain_for_usb_change(&key.2, &this.device_key)
                                            .expect("same battery producer");
                                        this.battery_levels.observe(readings);
                                        this.left_mode = readings.left_mode;
                                        this.telemetry = readings.telemetry;
                                        this.telemetry_seen = Some(Instant::now());
                                        let levels = this.battery_levels;
                                        cx.background_executor()
                                            .spawn(async move {
                                                crate::status_cache::save_levels(levels);
                                            })
                                            .detach();
                                        this.right_link_connected = readings.right_connected;
                                        this.right_link_known = readings.right_link_known;
                                        this.battery_error = None;
                                    }
                                    Err(error) => {
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
                    (!this.backup_state.active())
                        && !this.busy
                        && !matches!(this.page, Page::Firmware | Page::Pairing)
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
                            && (!this.backup_state.active())
                            && !this.busy
                            && !matches!(this.page, Page::Firmware | Page::Pairing)
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
                    (!this.backup_state.active())
                        && !matches!(this.page, Page::Firmware | Page::Pairing)
                        && crate::device_status::battery_available(
                            &this.device_key,
                            this.rescue.state(),
                        )
                });
                if matches!(battery_idle, Ok(true))
                    && battery_query.is_none()
                    && battery_checked
                        .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(3))
                    && let Ok(key) = this.update(cx, |this, _| {
                        (
                            this.battery_generation,
                            crate::device_status::battery_source(&this.device_key),
                            this.device_key.clone(),
                        )
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
                let state = this.update(cx, |this, _| this.backup_state.active());
                match state {
                    Err(_) => break,
                    Ok(false) => {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        continue;
                    }
                    Ok(true) => {}
                }
                let (result, recovery_locations) = cx
                    .background_executor()
                    .spawn(async {
                        (
                            device::discover(),
                            crate::status_cache::recovery_locations(),
                        )
                    })
                    .await;
                let running = this.update(cx, |this, cx| {
                    if !this.backup_state.active() {
                        return false;
                    }
                    // Status remains live while the backup state machine owns the guide.
                    this.recovery_locations = recovery_locations;
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
                    if this.observe_device_key(key) {
                        battery_checked = None;
                        version_checked = None;
                    }
                    if !this.backup_state.recovery()
                        && let Some(session) = this.session.as_mut()
                    {
                        session.observe(result);
                        this.view = session.view();
                        if let Some(error) = this.view.error.clone() {
                            this.backup_state
                                .transition(BackupEvent::Observed(crate::journey::State::Failed));
                            this.message = Some(error);
                        }
                        this.advance(cx);
                        cx.notify();
                    }
                    this.backup_state != BackupState::Complete
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
            pairing: PairingJourney::new(),
            pairing_observation: None,
            pairing_generation: 0,
            rescue: RecoveryJourney::new(),
            rescue_cancel: None,
            backup_state: BackupState::Intro,
            session: Some(session),
            view,
            role: None,
            busy: false,
            copies_folder: None,
            message: None,
            battery_levels: crate::status_cache::levels(),
            left_mode: None,
            telemetry: None,
            telemetry_seen: None,
            right_link_connected: false,
            right_link_known: false,
            battery_error: None,
            device_key: Vec::new(),
            device_generation: 0,
            battery_generation: 0,
            firmware_versions: Vec::new(),
            home: Home::default(),
            firmware: None,
            firmware_view: None,
            recovery_locations: [None; 3],
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    fn observe_device_key(&mut self, key: crate::device_status::UsbKey) -> bool {
        if key == self.device_key {
            return false;
        }
        let replaced = crate::device_status::battery_source(&key)
            != crate::device_status::battery_source(&self.device_key);
        if replaced {
            self.right_link_connected = false;
            self.right_link_known = false;
            self.telemetry = None;
            self.telemetry_seen = None;
            self.left_mode = None;
            self.battery_error = None;
            self.battery_generation = self.battery_generation.wrapping_add(1);
        }
        if crate::device_status::factory_left(&key) {
            self.right_link_connected = false;
            self.right_link_known = false;
        }
        self.firmware_versions.retain(|v| {
            key.iter().any(|(location, vendor, product, name)| {
                *location == v.location
                    && if v.factory {
                        crate::device_status::factory_left(&vec![(
                            *location,
                            *vendor,
                            *product,
                            name.clone(),
                        )])
                    } else {
                        *vendor == 0x4c4b
                            && *product == u64::from(v.role.product())
                            && name == v.role.name()
                    }
            })
        });
        self.device_key = key;
        self.device_generation = self.device_generation.wrapping_add(1);
        true
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
        if backup {
            self.backup_state
                .transition(if self.backup_state == BackupState::RecoveryFailed {
                    BackupEvent::Retry
                } else {
                    BackupEvent::RecoveryStarted
                });
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.rescue_cancel = Some(cancelled.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let worker_cancel = cancelled.clone();
            let (progress_sender, progress_receiver) = mpsc::channel();
            let mut worker = cx
                .background_executor()
                .spawn(async move {
                    if backup { recovery::run_backup(role, worker_cancel, progress_sender) }
                    else { recovery::run(role, worker_cancel, progress_sender) }
                });
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
                                if this.backup_state.active() && this.page == Page::Backups
                                    && this.session.as_mut().is_some_and(|journey| journey.accept_recovery(session))
                                { Ok(()) } else { Err("This recovery result no longer belongs to the active backup.".into()) }
                            });
                            if this.rescue.complete(attempt, role, result.clone()) {
                                this.rescue_cancel = None;
                                if backup {
                                    this.backup_state.transition(BackupEvent::RecoveryFinished(result.is_ok()));
                                    if let Err(error) = result {
                                        this.backup_state.transition(BackupEvent::Observed(crate::journey::State::Failed));
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
    }

    fn recovery_waiting(&self, label: &'static str, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(waiting_indicator(label, cx))
    }
    fn footer(&self, next: Option<Button>, cx: &mut Context<Self>) -> gpui::Div {
        let cancellable = !self.busy
            && match self.page {
                Page::Firmware => self.firmware_view.as_ref().is_none_or(|v| !v.complete),
                Page::Backups => self.backup_state.active(),
                Page::Recovery => matches!(
                    self.rescue.state(),
                    RecoveryState::Identify(_)
                        | RecoveryState::Guiding(_, _)
                        | RecoveryState::Failed(_, _)
                ),
                Page::Pairing => !matches!(
                    self.pairing.state(),
                    PairingState::Connected | PairingState::Cancelled
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
                        .h(px(40.))
                        .px(px(20.))
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
    fn backup_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        let title = if self.backup_state == BackupState::Complete {
            "Your firmware copy is saved".to_owned()
        } else if self.backup_state.failed() {
            "Let’s reconnect".to_owned()
        } else if self.busy {
            "Saving a copy…".to_owned()
        } else {
            self.view.title.clone()
        };
        let instruction = if self.backup_state == BackupState::Complete {
            "Your firmware is saved locally.".to_owned()
        } else if self.backup_state.failed() {
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
        let next = if self.backup_state == BackupState::Complete {
            Some(
                button("backup-done", "Next").on_click(cx.listener(|this, _, _, cx| {
                    this.backup_state.transition(BackupEvent::Finish);
                    this.navigate(Page::Home, cx);
                })),
            )
        } else if self.backup_state.failed() {
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
        JourneyScreen {
            body: screen,
            actions: Some(self.footer(next, cx)),
        }
    }

    fn recovery_card(
        &self,
        role: RecoveryRole,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        let artwork = match role {
            RecoveryRole::Left => img(peripheral_image(RecoveryRole::Left))
                .w(px(140.))
                .h(px(100.))
                .into_any_element(),
            RecoveryRole::Right => img(peripheral_image(RecoveryRole::Right))
                .w(px(140.))
                .h(px(100.))
                .into_any_element(),
            RecoveryRole::Receiver => img(peripheral_image(RecoveryRole::Receiver))
                .w(px(140.))
                .h(px(100.))
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

    fn recovery_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        let (body, next) = match self.rescue.state() {
            RecoveryState::Choose => (
                div()
                    .flex()
                    .flex_col()
                    .gap(px(24.))
                    .child(
                        div()
                            .w_full()
                            .text_center()
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
                None,
            ),
            RecoveryState::Identify(role) => (
                recovery_guide(
                    Some(*role),
                    "Connect your device",
                    crate::recovery_journey::instruction(*role),
                    Some(
                        self.recovery_waiting("Checking its firmware…", cx)
                            .into_any_element(),
                    ),
                    cx,
                ),
                None,
            ),
            RecoveryState::Guiding(role, procedure) => (
                recovery_guide(
                    Some(*role),
                    "Open the recovery drive",
                    procedure.instruction(*role),
                    Some(
                        self.recovery_waiting("Waiting for the recovery drive…", cx)
                            .into_any_element(),
                    ),
                    cx,
                ),
                None,
            ),
            RecoveryState::Ready(role) => (
                recovery_guide(
                    Some(*role),
                    "Recovery drive is ready",
                    "The recovery drive is open. Your firmware hasn’t been changed.",
                    None,
                    cx,
                ),
                Some(
                    button("recovery-done", "Next").on_click(cx.listener(|this, _, _, cx| {
                        this.cancel_recovery();
                        this.navigate(Page::Home, cx);
                    })),
                ),
            ),
            RecoveryState::Failed(role, error) => (
                recovery_guide(Some(*role), "Let’s try again", error.clone(), None, cx),
                Some(
                    button("retry-recovery", "Next").on_click(cx.listener(|this, _, _, cx| {
                        if this.page == Page::Firmware {
                            this.cancel_recovery();
                            this.message = None;
                            this.advance_firmware(cx);
                            return;
                        }
                        if let Some((role, attempt)) = this.rescue.retry() {
                            this.run_recovery(role, attempt, this.backup_state.recovery(), cx);
                        }
                    })),
                ),
            ),
        };
        JourneyScreen {
            body,
            actions: Some(self.footer(next, cx)),
        }
    }

    fn start_pairing(&mut self, cx: &mut Context<Self>) {
        self.pairing = PairingJourney::new();
        self.pairing_observation = None;
        self.pairing_generation += 1;
        let generation = self.pairing_generation;
        cx.spawn(async move |this, cx| {
            loop {
                let allowed = this.update(cx, |this, _| {
                    this.page == Page::Pairing
                        && this.pairing_generation == generation
                        && !matches!(
                            this.pairing.state(),
                            PairingState::Cancelled
                                | PairingState::Connected
                                | PairingState::Failed(_)
                        )
                });
                if !matches!(allowed, Ok(true)) {
                    break;
                }
                let busy = this.update(cx, |this, _| this.busy).unwrap_or(true);
                if !busy {
                    let result = cx
                        .background_executor()
                        .spawn(async { dongle_pairing::query() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.page != Page::Pairing
                            || this.pairing_generation != generation
                            || this.busy
                        {
                            return;
                        }
                        this.pairing_observation = result.as_ref().ok().cloned();
                        this.pairing.observe(result, Instant::now());
                        cx.notify();
                    });
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();
    }

    fn begin_pairing(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(observed) = &self.pairing_observation else {
            return;
        };
        let request = match self.pairing.begin(observed, Instant::now()) {
            Ok(request) => request,
            Err(error) => {
                self.pairing.observe(Err(error), Instant::now());
                cx.notify();
                return;
            }
        };
        let generation = self.pairing_generation;
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { dongle_pairing::begin(request) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.pairing_generation != generation || this.page != Page::Pairing {
                    return;
                }
                this.busy = false;
                this.pairing.accepted(result, Instant::now());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn pairing_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        let (instruction, waiting) = match self.pairing.state() {
            PairingState::Connect => (
                "Connect the left half by USB and turn the right half on. Plug in the dongle to check it too.",
                Some("Waiting for the keyboard…"),
            ),
            PairingState::TurnOnRight => (
                "Turn the right half on and keep it near the left half. They connect automatically.",
                Some("Waiting for the right half…"),
            ),
            PairingState::SwitchMode => (
                "Move the left switch to the top Dongle position. Keep USB connected.",
                Some("Waiting for Dongle mode…"),
            ),
            PairingState::Ready => (
                "Press Next to pair these two devices. Your other Bluetooth pairings stay unchanged.",
                None,
            ),
            PairingState::Pairing => (
                "Keep both USB connections in place. We’ll finish automatically.",
                Some("Pairing your dongle…"),
            ),
            PairingState::Connected => (
                if self
                    .pairing_observation
                    .as_ref()
                    .is_some_and(|o| o.dongle.is_some())
                {
                    "Both halves and your dongle are connected. No pairing is needed."
                } else {
                    "Both keyboard halves are connected. No pairing is needed."
                },
                None,
            ),
            PairingState::Failed(error) => (error.as_str(), None),
            PairingState::Cancelled => ("Pairing is closed.", None),
        };
        let artwork = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(24.))
            .child(
                img(peripheral_image(RecoveryRole::Left))
                    .w(px(180.))
                    .h(px(140.)),
            )
            .child(
                img(peripheral_image(RecoveryRole::Right))
                    .w(px(180.))
                    .h(px(140.)),
            )
            .when(
                self.pairing_observation
                    .as_ref()
                    .is_none_or(|o| o.dongle.is_some()),
                |row| {
                    row.child(
                        img(peripheral_image(RecoveryRole::Receiver))
                            .w(px(90.))
                            .h(px(140.)),
                    )
                },
            );
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(28.))
            .w_full()
            .child(artwork)
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(15.))
                    .line_height(px(23.))
                    .text_color(cx.theme().muted_foreground)
                    .child(instruction.to_owned()),
            );
        if let Some(label) = waiting {
            body = body.child(waiting_indicator(label, cx));
        }
        let next = match self.pairing.state() {
            PairingState::Ready => Some(
                button("begin-dongle-pairing", "Next")
                    .on_click(cx.listener(|this, _, _, cx| this.begin_pairing(cx))),
            ),
            PairingState::Connected => Some(
                button("pairing-done", "Next")
                    .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Home, cx))),
            ),
            _ => None,
        };
        JourneyScreen {
            body,
            actions: Some(self.footer(next, cx)),
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

    fn firmware_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        if let Some(error) = &self.message {
            return JourneyScreen {
                body: recovery_guide(
                    self.firmware_view.as_ref().map(|v| v.role),
                    "Let’s reconnect",
                    error.clone(),
                    None,
                    cx,
                ),
                actions: Some(
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
                    ),
                ),
            };
        }
        if self.rescue_cancel.is_some() {
            return self.recovery_screen(cx);
        }
        let Some(view) = &self.firmware_view else {
            return JourneyScreen {
                body: recovery_guide(
                    None,
                    "Preparing your firmware",
                    "Finding the verified release for your keyboard.",
                    Some(waiting_indicator("Checking firmware…", cx).into_any_element()),
                    cx,
                ),
                actions: None,
            };
        };
        let mut body = recovery_guide(
            Some(view.role),
            view.title.clone(),
            if self.busy {
                "Keep USB connected. We’ll continue automatically.".to_owned()
            } else {
                view.error
                    .clone()
                    .unwrap_or_else(|| view.instruction.clone())
            },
            None,
            cx,
        );
        let next = if self.busy {
            body = body.child(waiting_indicator("Working…", cx));
            None
        } else if view.can_transfer {
            Some(button("install-part", "Install firmware").on_click(
                cx.listener(|this, _, _, cx| this.firmware_job(cx, |journey| journey.transfer())),
            ))
        } else if view.needs_power_on_ack {
            Some(
                button("right-switched-on", "Next").on_click(cx.listener(|this, _, _, cx| {
                    if let Some(journey) = this.firmware.as_mut() {
                        journey.confirm_power_on();
                        this.firmware_view = Some(journey.view());
                    }
                    cx.notify();
                })),
            )
        } else if view.complete {
            Some(
                button("firmware-done", "Next")
                    .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Home, cx))),
            )
        } else if view.error.is_some() {
            None
        } else {
            body = body.child(waiting_indicator("Waiting for your keyboard…", cx));
            None
        };
        JourneyScreen {
            body,
            actions: Some(self.footer(next, cx)),
        }
    }

    fn start_copies(&mut self, cx: &mut Context<Self>) {
        if self.backup_state.active() || self.busy {
            return;
        }
        if let Some(journey) = self.session.as_mut()
            && journey.state() == crate::journey::State::Paused
        {
            journey.resume();
            self.view = journey.view();
            self.role = Some(journey.role());
            self.backup_state.transition(BackupEvent::Start);
            self.message = None;
            self.page = Page::Backups;
            self.advance(cx);
            cx.notify();
            return;
        }
        let session = Journey::backup();
        self.message = None;
        self.view = session.view();
        self.session = Some(session);
        self.role = Some(Role::Left);
        self.backup_state.transition(BackupEvent::Start);
        self.page = Page::Backups;
        self.advance(cx);
        cx.notify();
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        if !self.backup_state.active()
            || self.busy
            || self.backup_state.failed()
            || (self.backup_state == BackupState::Complete)
        {
            return;
        }
        if let Some(journey) = self.session.as_ref() {
            self.role = Some(journey.role());
            self.backup_state
                .transition(BackupEvent::Observed(journey.state()));
            self.view = journey.view();
        }
        if self
            .session
            .as_ref()
            .is_some_and(|j| j.state() == crate::journey::State::Guiding)
            && !self.backup_state.recovery()
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
        if self.view.can_save && self.backup_state != BackupState::Complete {
            self.save(cx);
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.backup_state == BackupState::RecoveryFailed {
            if let Some((role, attempt)) = self.rescue.retry() {
                self.run_recovery(role, attempt, true, cx);
            }
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.retry();
            self.backup_state.transition(BackupEvent::Retry);
            self.view = session.view();
            self.message = None;
            cx.notify();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.backup_state.active()
            || !self.view.can_save
            || self.busy
            || self.backup_state.failed()
        {
            return;
        }
        let Some(mut session) = self.session.take() else {
            return;
        };
        self.backup_state.transition(BackupEvent::SaveStarted);
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
                this.backup_state
                    .transition(BackupEvent::Observed(session.state()));
                this.session = Some(session);
                this.busy = false;
                match result {
                    Ok(path) => {
                        this.copies_folder = path.parent().map(PathBuf::from);
                        this.message = None;
                    }
                    Err(error) => {
                        this.backup_state
                            .transition(BackupEvent::Observed(crate::journey::State::Failed));
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
        let leaving_backup = self.backup_state.active() && page != Page::Backups;
        if self.backup_state.active() && page != Page::Backups {
            if let Some(journey) = self.session.as_mut() {
                journey.pause();
            }
            self.backup_state.transition(BackupEvent::Pause);
        }
        if (self.page == Page::Recovery && page != Page::Recovery)
            || leaving_backup
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
        if self.page == Page::Pairing && page != Page::Pairing {
            self.pairing.cancel();
            self.pairing_generation += 1;
        }
        self.page = page;
        if page == Page::Pairing {
            self.start_pairing(cx);
        }
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
            .mb(px(6.))
            .h(px(36.))
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
                    .mb(px(6.))
                    .h(px(36.))
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
            .when(firmware_action.is_some(), |nav| nav.child(tasks))
            .child(
                SidebarGroup::new("Additional utilities")
                    .child(self.nav_row(
                        "Backup firmware",
                        Page::Backups,
                        IconName::HardDriveDownload,
                        cx,
                    ))
                    .child(self.nav_row(
                        "Enter recovery mode",
                        Page::Recovery,
                        IconName::HeartPulse,
                        cx,
                    ))
                    .child(self.nav_row(
                        "Check pairing",
                        Page::Pairing,
                        IconName::SatelliteDish,
                        cx,
                    )),
            );

        let mut heading = div().flex().flex_col().gap(px(24.)).w_full().child(
            div()
                .text_size(px(23.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    (match self.page {
                        Page::Backups if self.backup_state == BackupState::Saving => {
                            "Saving your firmware copy"
                        }
                        Page::Backups if self.backup_state == BackupState::Returning => {
                            self.view.title.as_str()
                        }
                        Page::Backups if self.backup_state == BackupState::Complete => {
                            "Your firmware copy is saved"
                        }
                        Page::Backups | Page::Home => "Backup firmware",
                        Page::Recovery => "Enter recovery mode",
                        Page::Pairing => "Check pairing",
                        Page::Firmware => "RMK firmware",
                    })
                    .to_owned(),
                ),
        );
        match self.page {
            Page::Backups if self.backup_state.shown() => {
                if let Some(journey) = &self.session {
                    heading = heading.child(flow_indicator(
                        flow_presentation::backup(journey),
                        "backup-steps",
                        cx,
                    ));
                }
            }
            Page::Firmware => {
                heading = heading.child(flow_indicator(
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
            _ => {}
        }
        let screen = match self.page {
            Page::Backups if self.backup_state.recovery() => self.recovery_screen(cx),
            Page::Backups if self.backup_state.shown() => self.backup_screen(cx),
            Page::Backups | Page::Home => JourneyScreen {
                body: recovery_guide(
                    None,
                    "Backup firmware",
                    "Save a copy of the connected keyboard’s firmware locally.",
                    None,
                    cx,
                ),
                actions: Some(
                    self.footer(
                        Some(
                            button("start-backup", "Next")
                                .on_click(cx.listener(|this, _, _, cx| this.start_copies(cx))),
                        ),
                        cx,
                    ),
                ),
            },
            Page::Recovery => self.recovery_screen(cx),
            Page::Pairing => self.pairing_screen(cx),
            Page::Firmware => self.firmware_screen(cx),
        };
        let canvas = div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(32.))
                    .pt(px(28.))
                    .pb(px(12.))
                    .child(heading),
            )
            .child(
                div()
                    .id("journey-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .min_h_full()
                            .p(px(32.))
                            .child(screen.body.w_full().max_w(px(620.))),
                    ),
            )
            .when_some(screen.actions, |pane, actions| {
                pane.child(
                    div()
                        .flex_shrink_0()
                        .px(px(32.))
                        .pt(px(16.))
                        .pb(px(28.))
                        .child(actions),
                )
            });
        let firmware = if crate::device_status::factory_left(&self.device_key) {
            crate::firmware_version::label(true, &self.firmware_versions)
        } else if !crate::device_status::battery_source(&self.device_key).is_empty() {
            crate::firmware_version::label(false, &self.firmware_versions)
        } else {
            "Firmware not detected".to_owned()
        };
        let observed = crate::device_status::Observation {
            devices: &self.device_key,
            recovery_locations: self.recovery_locations,
            levels: self.battery_levels,
            left_mode: self.left_mode,
            bluetooth_connected: self.bluetooth_connected,
            dongle_connected: self.dongle_connected,
            right_link_connected: self.right_link_connected,
            right_link_known: self.right_link_known,
            telemetry: self.telemetry,
            links_fresh: self
                .telemetry_seen
                .is_some_and(|t| t.elapsed() < Duration::from_secs(45)),
        }
        .derive();
        let status = crate::status_strip::render(
            firmware,
            observed.connection,
            observed.left,
            observed.right,
            self.dongle_connected,
            observed.dongle_recovery,
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
                        .child(div().id("journey-canvas").flex_1().min_h_0().child(canvas)),
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
        Some(RecoveryRole::Left) => {
            peripheral_picture(Some(RecoveryRole::Left), cx).into_any_element()
        }
        Some(RecoveryRole::Right) => {
            peripheral_picture(Some(RecoveryRole::Right), cx).into_any_element()
        }
        Some(RecoveryRole::Receiver) => {
            peripheral_picture(Some(RecoveryRole::Receiver), cx).into_any_element()
        }
        None => div()
            .flex()
            .justify_center()
            .gap(px(16.))
            .py(px(12.))
            .child(
                img(peripheral_image(RecoveryRole::Left))
                    .w(px(180.))
                    .h(px(130.)),
            )
            .child(
                img(peripheral_image(RecoveryRole::Right))
                    .w(px(180.))
                    .h(px(130.)),
            )
            .child(
                img(peripheral_image(RecoveryRole::Receiver))
                    .w(px(100.))
                    .h(px(130.)),
            )
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .w_full()
        .gap(px(24.))
        .child(picture)
        .child(
            div()
                .w_full()
                .text_center()
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
    Button::new(id)
        .label(label)
        .primary()
        .h(px(40.))
        .px(px(20.))
}

// Photo-based orientation sketches; their appearance never represents device status.
fn peripheral_image(role: RecoveryRole) -> Arc<Image> {
    static LEFT: OnceLock<Arc<Image>> = OnceLock::new();
    static RIGHT: OnceLock<Arc<Image>> = OnceLock::new();
    static DONGLE: OnceLock<Arc<Image>> = OnceLock::new();
    let sketch = match role {
        RecoveryRole::Right => RIGHT.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../assets/nocfree-right.svg").to_vec(),
            ))
        }),
        RecoveryRole::Receiver => DONGLE.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../assets/nocfree-dongle.svg").to_vec(),
            ))
        }),
        RecoveryRole::Left => LEFT.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../assets/nocfree-left.svg").to_vec(),
            ))
        }),
    };
    sketch.clone()
}

fn peripheral_picture(role: Option<RecoveryRole>, cx: &App) -> impl IntoElement {
    let sketch = peripheral_image(role.unwrap_or(RecoveryRole::Left));
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
                    Some(RecoveryRole::Left) => "Left half",
                    Some(RecoveryRole::Right) => "Right half",
                    Some(RecoveryRole::Receiver) => "USB dongle",
                    None => "",
                }),
        )
}
