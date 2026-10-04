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

use crate::backup_flow::{Event as BackupEvent, Machine as BackupMachine, State as BackupState};
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
    device::{self},
    home::{Home, UpdateAssessment},
    journey::Journey,
    recovery,
    session::View,
};
use gpui::{
    App, AppContext, Context, FocusHandle, Focusable, FontWeight, Image, ImageFormat,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Task, Window, div, img, prelude::FluentBuilder, px,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    sidebar::{Sidebar, SidebarGroup, SidebarMenuItem},
    spinner::Spinner,
    stepper::{Stepper, StepperItem},
};

use crate::factory_release::FactoryRelease;
use crate::install_journey::{self, Machine as InstallMachine, Stage as InstallStage};
use crate::navigation::{Navigation, Page, Start};
use crate::scope::Scope;
use gpui_kit::component::input::{Input, InputEvent, InputState};

struct JourneyScreen {
    body: gpui::Div,
    actions: Option<gpui::Div>,
}

pub struct Companion {
    navigation: Navigation,
    peripherals: Option<crate::peripheral_journey::Machine>,
    usb_identification: crate::scope_presence::Identifier,
    appearance_subscription: Option<gpui::Subscription>,
    dongle_connected: bool,
    bluetooth_connected: bool,
    factory_bluetooth_connected: bool,
    factory_release: Option<FactoryRelease>,
    factory_source: crate::factory_source::Machine,
    bundled_version: Option<String>,
    pairing: PairingJourney,
    pairing_observation: Option<dongle_pairing::Observation>,
    pairing_generation: u64,
    rescue: RecoveryJourney,
    rescue_cancel: Option<Arc<AtomicBool>>,
    manual_advance: bool,
    pending_recovery: Option<(Attempt, RecoveryRole, crate::session::Session)>,
    pending_procedure: Option<(Attempt, RecoveryRole, crate::recovery_journey::Procedure)>,
    backup_state: BackupMachine,
    session: Option<Journey>,
    operation: crate::operation::Operation,
    copies_folder: Option<PathBuf>,
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
    versions_seen: Option<Instant>,
    version_refresh: bool,
    home: Home,
    firmware: Option<FirmwareJourney>,
    pending_firmware: Option<FirmwareJourney>,
    pending_backup: Option<Journey>,
    install: Option<InstallMachine>,
    typing_input: Option<(
        install_journey::Ticket,
        gpui::Entity<InputState>,
        gpui::Subscription,
    )>,
    discovery_seen: Option<Instant>,
    latest_discovery: Option<device::Snapshot>,
    bluetooth_seen: Option<Instant>,
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
        let poll = cx.spawn(async move |this, cx| {
            let bundled = cx
                .background_executor()
                .spawn(async {
                    FirmwareRelease::bundled()
                        .ok()
                        .map(|r| r.version().to_owned())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.bundled_version = bundled;
                this.observe_preflight();
                cx.notify();
            });
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
                let bluetooth_interval = this
                    .update(cx, |this, _| {
                        if this.install_checks_active() {
                            Duration::from_secs(3)
                        } else {
                            Duration::from_secs(10)
                        }
                    })
                    .unwrap_or(Duration::from_secs(10));
                if bluetooth_checked.is_none_or(|t| t.elapsed() >= bluetooth_interval) {
                    let connected = cx
                        .background_executor()
                        .spawn(async { device::bluetooth_links() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some((rmk, factory)) = connected {
                            this.bluetooth_connected = rmk;
                            this.factory_bluetooth_connected = factory;
                            this.bluetooth_seen = Some(Instant::now());
                        } else {
                            this.bluetooth_seen = None;
                        }
                        cx.notify();
                    });
                    bluetooth_checked = Some(Instant::now());
                }
                let idle = this.update(cx, |this, _| !this.backup_state.active());
                if matches!(idle, Ok(true)) {
                    let identification_ticket = this
                        .update(cx, |this, _| this.usb_identification.ticket())
                        .ok();
                    let peripheral_ticket = this
                        .update(cx, |this, _| {
                            this.peripherals.as_ref().map(|batch| batch.ticket())
                        })
                        .ok()
                        .flatten();
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
                                this.discovery_seen =
                                    observation.as_ref().ok().map(|_| Instant::now());
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
                                if let Some(ticket) = identification_ticket {
                                    this.usb_identification.observe(
                                        ticket,
                                        &key,
                                        observation.is_ok(),
                                    );
                                }
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
                                this.latest_discovery = observation.as_ref().ok().cloned();
                                let mode = this.fresh_left_mode();
                                if this.firmware_page()
                                    && !this.navigation.setup()
                                    && !this.operation.busy()
                                    && let Some(journey) = this.firmware.as_mut()
                                {
                                    journey.observe_with_mode(observation.clone(), mode);
                                }
                                this.advance_recovery_batch(&observation, peripheral_ticket, cx);
                                let next = Home::observe(observation, UpdateAssessment::Unknown);
                                this.home = next;
                                this.refresh_scope_presence();
                                this.observe_preflight();
                                this.advance_firmware(cx);
                                this.advance(cx);
                                this.observe_install();
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
                        && !this.operation.busy()
                        && (this.navigation.setup()
                            || this.install_checks_active()
                            || !matches!(
                                this.navigation.page(),
                                Page::Firmware | Page::Restore | Page::Pairing
                            ))
                        && !matches!(
                            &this.rescue.state(),
                            RecoveryState::Identify(_) | RecoveryState::Guiding(_, _)
                        )
                });
                if matches!(version_idle, Ok(true))
                    && (matches!(this.update(cx, |this, _| this.version_refresh), Ok(true))
                        || version_checked.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)))
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
                            && !this.operation.busy()
                            && (this.navigation.setup()
                                || !matches!(
                                    this.navigation.page(),
                                    Page::Firmware | Page::Restore | Page::Pairing
                                ))
                            && !matches!(
                                &this.rescue.state(),
                                RecoveryState::Identify(_) | RecoveryState::Guiding(_, _)
                            )
                        {
                            this.versions_seen = Some(Instant::now());
                            this.version_refresh = false;
                            this.firmware_versions = versions
                                .into_iter()
                                .filter(|v| {
                                    key.iter()
                                        .any(|(location, _, _, _)| *location == v.location)
                                })
                                .collect();
                            this.observe_preflight();
                            cx.notify();
                        }
                    });
                    version_checked = Some(Instant::now());
                }
                let battery_idle = this.update(cx, |this, _| {
                    !this.operation.busy()
                        && (this.navigation.setup()
                            || this.install_checks_active()
                            || this.backup_state.state() == BackupState::Returning
                            || this.firmware_view().is_some_and(|view| {
                                !view.needs_recovery && !view.can_transfer && !view.complete
                            })
                            || !matches!(
                                this.navigation.page(),
                                Page::Firmware | Page::Restore | Page::Pairing
                            ))
                        && crate::device_status::battery_available(
                            &this.device_key,
                            &this.rescue.state(),
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
                    let mode = this.fresh_left_mode();
                    if !this.backup_state.recovery()
                        && let Some(session) = this.session.as_mut()
                    {
                        session.observe_with_mode(result, mode);

                        if let Some(error) = session.view().error.clone() {
                            this.backup_state
                                .transition(BackupEvent::Observed(crate::journey::State::Failed));
                            this.operation.fail(error);
                        }
                        this.advance(cx);
                        cx.notify();
                    }
                    this.backup_state.state() != BackupState::Complete
                });
                if running.is_err() {
                    break;
                }
                let timer = cx.background_executor().timer(Duration::from_secs(1));
                timer.await;
            }
        });
        Self {
            navigation: Navigation::default(),
            peripherals: None,
            usb_identification: crate::scope_presence::Identifier::default(),
            appearance_subscription: None,
            dongle_connected: false,
            bluetooth_connected: false,
            factory_bluetooth_connected: false,
            factory_release: None,
            factory_source: crate::factory_source::Machine::default(),
            bundled_version: None,
            pairing: PairingJourney::new(),
            pairing_observation: None,
            pairing_generation: 0,
            rescue: RecoveryJourney::new(),
            rescue_cancel: None,
            manual_advance: false,
            pending_recovery: None,
            pending_procedure: None,
            backup_state: BackupMachine::new(),
            session: Some(session),
            operation: Default::default(),
            copies_folder: None,
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
            versions_seen: None,
            version_refresh: false,
            home: Home::default(),
            firmware: None,
            pending_firmware: None,
            pending_backup: None,
            install: None,
            typing_input: None,
            discovery_seen: None,
            latest_discovery: None,
            bluetooth_seen: None,
            recovery_locations: [None; 3],
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    fn fresh_left_mode(&self) -> Option<crate::device_status::Mode> {
        self.left_mode.filter(|_| {
            self.telemetry_seen
                .is_some_and(|seen| seen.elapsed() <= Duration::from_secs(5))
        })
    }

    fn backup_component(&self) -> Option<RecoveryRole> {
        self.session
            .as_ref()
            .map(Journey::component)
            .or_else(|| self.operation.role(crate::operation::Kind::Backup))
    }
    fn backup_view(&self) -> Option<View> {
        self.session.as_ref().map(Journey::view)
    }
    fn firmware_view(&self) -> Option<FirmwareView> {
        self.firmware
            .as_ref()
            .map(FirmwareJourney::view)
            .or_else(|| self.operation.firmware_view())
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
        self.versions_seen = None;
        self.device_key = key;
        self.device_generation = self.device_generation.wrapping_add(1);
        true
    }

    fn firmware_page(&self) -> bool {
        matches!(self.navigation.page(), Page::Firmware | Page::Restore)
    }
    fn load_factory_sources(
        &mut self,
        selected: Option<(RecoveryRole, PathBuf)>,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.page() != Page::Restore
            || !self.navigation.setup()
            || self.operation.busy()
        {
            return;
        }
        let Some(ticket) =
            self.operation
                .begin(crate::operation::Kind::PrepareFirmware, None, None)
        else {
            return;
        };
        let existing = self.factory_release.clone();
        let source_ticket = self.factory_source.ticket();
        let accepted = selected.clone();
        let supplied = self.factory_source.source() == crate::factory_source::Source::Supplied;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut release = if selected.is_none() && !supplied { FactoryRelease::discover()? } else { match existing { Some(r) => r, None => FactoryRelease::discover()? } };
                    if let Some((role, path)) = selected {
                        if !supplied && std::fs::metadata(&path).map_err(|_| "Could not read factory backup.")?.len() != 1728 * 512 { return Err("Choose a complete saved factory backup, or select Supply UF2 files.".into()); }
                        release.import(role, &path)?;
                    }
                    Ok::<_, String>(release)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.operation.complete(ticket) {
                    return;
                }
                match result {
                    Ok(release) if this.factory_source.ticket() == source_ticket => {
                        this.factory_release = Some(release);
                        if let Some((role, path)) = accepted { this.factory_source.accept(source_ticket, role, path); }
                    },
                    Ok(_) => {},
                    Err(error) => this.operation.fail(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn choose_factory_file(&mut self, role: RecoveryRole, cx: &mut Context<Self>) {
        let source_ticket = self.factory_source.ticket();
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose factory firmware".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && paths.len() == 1
            {
                let _ = this.update(cx, |this, cx| {
                    if this.factory_source.ticket() == source_ticket {
                        this.load_factory_sources(Some((role, paths[0].clone())), cx);
                    }
                });
            }
        })
        .detach();
    }
    fn select_factory_source(
        &mut self,
        source: crate::factory_source::Source,
        cx: &mut Context<Self>,
    ) {
        if self.operation.busy() || self.factory_source.source() == source {
            return;
        }
        self.factory_source.select(source);
        self.factory_release = None;
        self.load_factory_sources(None, cx);
        cx.notify();
    }
    fn factory_sources_screen(&self, cx: &mut Context<Self>) -> gpui::Div {
        let supplied = self.factory_source.source() == crate::factory_source::Source::Supplied;
        let mut choices = div().flex().gap(px(12.)).w_full();
        for (source, label, id) in [
            (
                crate::factory_source::Source::Backups,
                "Use saved backups",
                "factory-use-backups",
            ),
            (
                crate::factory_source::Source::Supplied,
                "Supply UF2 files",
                "factory-supply-files",
            ),
        ] {
            choices = choices.child(
                Button::new(id)
                    .label(label)
                    .outline()
                    .flex_1()
                    .h(px(64.))
                    .cursor_pointer()
                    .disabled(self.operation.busy())
                    .when(self.factory_source.source() == source, |b| b.primary())
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_factory_source(source, cx)),
                    ),
            );
        }
        let mut cards = div().flex().gap(px(12.)).w_full();
        for (role, label, id) in [
            (RecoveryRole::Left, "Left half", "factory-left"),
            (RecoveryRole::Right, "Right half", "factory-right"),
            (RecoveryRole::Receiver, "USB dongle", "factory-dongle"),
        ] {
            if !self
                .navigation
                .scope()
                .is_some_and(|scope| scope.roles().contains(&role))
            {
                continue;
            }
            let ready = if supplied {
                self.factory_source.file(role).is_some()
            } else {
                self.factory_release
                    .as_ref()
                    .is_some_and(|release| release.has(role))
            };
            let caption = if supplied {
                self.factory_source
                    .file(role)
                    .and_then(|p| p.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Choose or drop a UF2".into())
            } else if ready {
                "Factory backup ready".into()
            } else {
                "Choose a factory backup".into()
            };
            let content = div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(18.))
                .child(img(peripheral_image(role)).w(px(140.)).h(px(110.)))
                .child(label)
                .child(
                    div()
                        .text_size(px(12.))
                        .max_w(px(160.))
                        .overflow_hidden()
                        .text_color(cx.theme().muted_foreground)
                        .child(caption.clone()),
                );
            cards = cards.child(
                div()
                    .id(id)
                    .flex_1()
                    .on_drop(
                        cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                            if (supplied || !ready) && paths.0.len() == 1 {
                                this.load_factory_sources(Some((role, paths.0[0].clone())), cx);
                            }
                        }),
                    )
                    .child(
                        Button::new(id)
                            .accessibility_label(format!("{label}: {caption}"))
                            .outline()
                            .w_full()
                            .h(px(225.))
                            .cursor_pointer()
                            .disabled(self.operation.busy() || (!supplied && ready))
                            .when(!supplied && ready, |b| b.cursor_default())
                            .child(content)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if supplied || !ready {
                                    this.choose_factory_file(role, cx);
                                }
                            })),
                    ),
            );
        }
        let mut body = div().flex().flex_col().gap(px(24.)).child(div().text_center().font_weight(FontWeight::MEDIUM).child(self.navigation.scope().map_or("", Scope::label))).child(choices).child(div().text_center().text_color(cx.theme().muted_foreground).child(if supplied { "Choose one factory UF2 for each part. Your saved factory backup supplies anything else needed." } else { "Restore your saved factory firmware. Your current RMK firmware will be backed up first." })).child(cards);
        if self.operation.busy() {
            body = body.child(waiting_indicator("Checking factory backups…", cx));
        }
        if let Some(error) = self.operation.error() {
            body = body.child(
                div()
                    .text_center()
                    .text_color(cx.theme().danger)
                    .child(error.to_owned()),
            );
        }
        body
    }
    fn observe_preflight(&mut self) {
        let ready = crate::firmware_preflight::assess_scoped(
            self.navigation.page(),
            &self.device_key,
            &self.firmware_versions,
            self.bundled_version.as_deref().filter(|_| {
                self.versions_seen
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(15))
            }),
            self.discovery_seen
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5)),
            self.navigation.scope(),
        );
        self.navigation.observe(ready);
    }
    fn start_selected_journey(&mut self, cx: &mut Context<Self>) {
        if self.operation.busy() {
            return;
        }
        self.refresh_scope_presence();
        self.observe_preflight();
        if self.navigation.page() == Page::Restore
            && (self.operation.error().is_some()
                || !self
                    .factory_release
                    .as_ref()
                    .is_some_and(|r| self.factory_ready(r)))
        {
            return;
        }
        match self.navigation.next() {
            Some(Start::Backup(scope)) => {
                self.peripherals = Some(crate::peripheral_journey::Machine::new(scope));
                self.start_copies(scope.roles()[0], cx);
            }
            Some(Start::Recovery(scope)) => {
                self.peripherals = Some(crate::peripheral_journey::Machine::new(scope));
                self.start_recovery(scope.roles()[0], cx);
            }
            Some(Start::Pairing) => self.start_pairing(cx),
            Some(Start::Firmware(_) | Start::Restore(_)) => self.start_firmware(cx),
            None => return,
        }
        cx.notify();
    }
    fn factory_ready(&self, release: &FactoryRelease) -> bool {
        self.navigation.scope().is_some_and(|scope| {
            self.factory_source
                .ready_for(scope.roles().iter().all(|role| release.has(*role)), scope)
        })
    }
    fn setup_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        if self.navigation.choosing() {
            use crate::scope_presence::Identification;
            let identification = self.usb_identification.state();
            if let Identification::Disconnect(role)
            | Identification::Connect(role)
            | Identification::Complete(role) = identification
            {
                let ticket = self.usb_identification.ticket();
                let done = matches!(identification, Identification::Complete(_));
                let message = match identification {
                    Identification::Disconnect(_) => {
                        "Unplug the unidentified left half and dongle. Leave any parts you already identified connected."
                    }
                    Identification::Connect(RecoveryRole::Left) => {
                        "Connect only the LEFT half by USB in middle WIRED. Leave already identified parts connected."
                    }
                    Identification::Connect(RecoveryRole::Receiver) => {
                        "Connect only the USB dongle. Leave already identified parts connected."
                    }
                    Identification::Complete(_) => "USB connection identified.",
                    _ => "Connect the selected part by USB.",
                };
                return JourneyScreen {
                    body: recovery_guide_status(
                        Some(role),
                        "Identify USB connection",
                        message,
                        if done {
                            None
                        } else {
                            Some(
                                self.recovery_waiting("Watching USB connections…", cx)
                                    .into_any_element(),
                            )
                        },
                        done || self.usb_identification.can_next(),
                        cx,
                    ),
                    actions: Some(
                        div()
                            .flex()
                            .justify_between()
                            .w_full()
                            .child(
                                Button::new("cancel-identify")
                                    .label("Cancel")
                                    .secondary()
                                    .cursor_pointer()
                                    .h(px(40.))
                                    .px(px(20.))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.usb_identification.ticket() == ticket {
                                            this.usb_identification.cancel();
                                            this.refresh_scope_presence();
                                            cx.notify();
                                        }
                                    })),
                            )
                            .when(true, |row| {
                                row.child(
                                    button("identify-next", "Next")
                                        .disabled(!done && !self.usb_identification.can_next())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if this.usb_identification.ticket() == ticket {
                                                if done {
                                                    this.usb_identification.cancel();
                                                } else {
                                                    this.usb_identification.next(ticket);
                                                }
                                                this.refresh_scope_presence();
                                                cx.notify();
                                            }
                                        })),
                                )
                            }),
                    ),
                };
            }
            let instruction = match self.navigation.page() {
                Page::Recovery => "Choose the part you want to put into recovery mode",
                Page::Firmware => "Choose the part you want to install RMK on",
                Page::Restore => "Choose the part you want to restore",
                _ => "Choose the part you want to back up",
            };
            return JourneyScreen {
                body: self.peripheral_picker(instruction, cx),
                actions: Some(
                    self.footer(
                        Some(
                            button("choose-scope-next", "Next")
                                .disabled(self.navigation.draft_scope().is_none())
                                .when(self.navigation.draft_scope().is_none(), |button| {
                                    button.cursor_default()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.refresh_scope_presence();
                                    if this.navigation.choose_next() {
                                        this.observe_preflight();
                                    }
                                    cx.notify();
                                })),
                        ),
                        cx,
                    ),
                ),
            };
        }
        let scope = self.navigation.scope().unwrap_or(Scope::Whole);
        let already = match self.navigation.readiness() {
            crate::navigation::Readiness::AlreadyLatest => Some("Already latest version"),
            crate::navigation::Readiness::AlreadyFactory => Some("Already on factory firmware"),
            _ => None,
        };
        if let Some(label) = already {
            return JourneyScreen {
                body: scope_guide(scope, label, cx),
                actions: Some(self.footer(None, cx)),
            };
        }
        let body = match self.navigation.page() {
            Page::Restore => self.factory_sources_screen(cx).when(scope != Scope::Whole, |body| {
                body.child(
                    div()
                        .text_center()
                        .text_color(cx.theme().muted_foreground)
                        .child("Restore the selected parts. Whole-keyboard restoration also checks typing."),
                )
            }),
            Page::Backups | Page::Home => {
                scope_guide(scope, "Save a copy of the selected firmware locally.", cx)
            }
            Page::Recovery => scope_guide(
                scope,
                "Open recovery for the selected parts, one at a time.",
                cx,
            ),
            Page::Pairing => recovery_guide(
                None,
                "Check pairing",
                "Check the connection between your keyboard halves and dongle.",
                None,
                cx,
            ),
            Page::Firmware => scope_guide(
                scope,
                if scope == Scope::Whole {
                    "Install RMK on your dongle and both halves, then check pairing and typing. Your current firmware will be backed up automatically."
                } else {
                    "Install RMK on the selected parts and save their current firmware. Whole-keyboard installation also checks pairing and typing."
                },
                cx,
            ),
        };
        let enabled = self.navigation.can_start()
            && !self.operation.busy()
            && (self.navigation.page() != Page::Restore
                || (self
                    .factory_release
                    .as_ref()
                    .is_some_and(|r| self.factory_ready(r))
                    && self.operation.error().is_none()));
        let next = button("start-journey", "Next")
            .disabled(!enabled)
            .when(!enabled, |button| button.cursor_default())
            .on_click(cx.listener(|this, _, _, cx| this.start_selected_journey(cx)));
        let body = if self.navigation.page() != Page::Pairing
            && scope
                .roles()
                .iter()
                .any(|role| !self.navigation.available(*role))
        {
            body.child(
                div()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("Connect the selected parts by USB to continue."),
            )
        } else {
            body
        };
        JourneyScreen {
            body,
            actions: Some(self.footer(Some(next), cx)),
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
        if backup {
            self.backup_state.transition(
                if self.backup_state.state() == BackupState::RecoveryFailed {
                    BackupEvent::Retry
                } else {
                    BackupEvent::RecoveryStarted
                },
            );
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
                            this.rescue_cancel = None;
                            match result {
                                Ok(session) => this.pending_recovery = Some((attempt, role, session)),
                                Err(error) => {
                                    this.rescue.complete(attempt, role, Err(error.clone()));
                                    if backup { this.backup_state.transition(BackupEvent::RecoveryFinished(false)); }
                                    this.operation.fail(error);
                                }
                            }
                            cx.notify();
                        });
                        break;
                    }
                    _ = cx.background_executor().timer(Duration::from_millis(100)) => {
                        while let Ok(procedure) = progress_receiver.try_recv() {
                            let _ = this.update(cx, |this, cx| {
                                if this.rescue_cancel.as_ref().is_some_and(|current| Arc::ptr_eq(current, &cancelled))
                                    && !cancelled.load(Ordering::Relaxed)
                                {
                                    if this.rescue.state() != RecoveryState::Guiding(role, procedure) { this.pending_procedure = Some((attempt, role, procedure)); }
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

    fn recovery_proof_ready(&self) -> bool {
        if self.pending_recovery.is_none() {
            return self.pending_procedure.is_some();
        }
        if !self
            .discovery_seen
            .is_some_and(|seen| seen.elapsed() < Duration::from_secs(5))
        {
            return false;
        }
        let Some(snapshot) = &self.latest_discovery else {
            return false;
        };
        let (_, role, session) = self.pending_recovery.as_ref().unwrap();
        if let Ok((bound_role, location, mount)) = session.recovery_binding() {
            bound_role == *role
                && snapshot
                    .devices
                    .iter()
                    .any(|d| d.location == location && d.bootloader())
                && snapshot.mounts.contains(&mount)
        } else {
            self.navigation.page() == Page::Backups
                && session.view().can_save
                && session.recovery_observation_matches(snapshot)
                && snapshot.mounts.len() == 1
                && snapshot.devices.iter().filter(|d| d.bootloader()).count() == 1
        }
    }
    fn recovery_next(&mut self, cx: &mut Context<Self>) {
        if !self.recovery_proof_ready() {
            return;
        }
        if let Some((attempt, role, session)) = self.pending_recovery.take() {
            self.pending_procedure = None;
            if let Ok((_, location, _)) = session.recovery_binding() {
                self.recovery_locations[role_index(role)] = Some(location);
            }
            if self.firmware_page() {
                let verification = self.firmware_view().is_some_and(|v| v.verification);
                self.rescue.cancel();
                self.firmware_job(cx, move |journey| {
                    if verification {
                        journey.accept_verification(session)
                    } else {
                        journey.accept_recovery(session)
                    }
                });
            } else if self.navigation.page() == Page::Backups {
                let accepted = self
                    .session
                    .as_mut()
                    .is_some_and(|journey| journey.accept_recovery(session));
                self.rescue.complete(
                    attempt,
                    role,
                    if accepted {
                        Ok(())
                    } else {
                        Err("Reconnect this part and try again.".into())
                    },
                );
                self.backup_state
                    .transition(BackupEvent::RecoveryFinished(accepted));
            } else {
                self.rescue.complete(attempt, role, Ok(()));
            }
        } else if let Some((attempt, role, procedure)) = self.pending_procedure.take() {
            self.rescue.observe(attempt, role, procedure);
        }
        cx.notify();
    }
    fn manual_next(&mut self, cx: &mut Context<Self>) {
        if self.operation.busy() {
            return;
        }
        if let Some(journey) = self.pending_firmware.take() {
            self.firmware = Some(journey);
        } else if let Some(journey) = self.pending_backup.take() {
            self.backup_state
                .transition(BackupEvent::Observed(journey.state()));
            self.session = Some(journey);
        } else if self.pending_recovery.is_some() || self.pending_procedure.is_some() {
            self.recovery_next(cx);
        } else if self.firmware_page() {
            if self.firmware.as_mut().is_some_and(FirmwareJourney::next) {
                cx.notify();
                return;
            }
            self.manual_advance = true;
            self.advance_firmware(cx);
        } else if self.navigation.page() == Page::Backups {
            if self.backup_state.next() {
                cx.notify();
                return;
            }
            if self.session.as_mut().is_some_and(Journey::next_return) {
                cx.notify();
                return;
            }
            self.manual_advance = true;
            self.advance(cx);
        }
        cx.notify();
    }

    fn cancel_recovery(&mut self) {
        self.pending_recovery = None;
        self.pending_procedure = None;
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
        let cancellable = !self.navigation.setup()
            && !self.operation.busy()
            && match self.navigation.page() {
                Page::Firmware | Page::Restore => self
                    .install
                    .as_ref()
                    .is_none_or(|m| m.stage() != InstallStage::Complete),
                Page::Backups => self.backup_state.active(),
                Page::Recovery => {
                    self.peripherals
                        .as_ref()
                        .is_some_and(|b| b.waiting_detach())
                        || matches!(
                            self.rescue.state(),
                            RecoveryState::Identify(_)
                                | RecoveryState::Guiding(_, _)
                                | RecoveryState::Failed(_, _)
                        )
                }
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
            .when(
                self.navigation.setup()
                    && !self.navigation.choosing()
                    && self.navigation.page() != Page::Pairing
                    && !self.operation.busy(),
                |row| {
                    row.child(
                        Button::new("previous-scope")
                            .label("Previous")
                            .secondary()
                            .cursor_pointer()
                            .h(px(40.))
                            .px(px(20.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.navigation.previous();
                                this.observe_preflight();
                                cx.notify();
                            })),
                    )
                },
            )
            .when(cancellable, |row| {
                row.child(
                    Button::new("cancel-journey")
                        .cursor_pointer()
                        .label("Cancel")
                        .secondary()
                        .h(px(40.))
                        .px(px(20.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(batch) = this.peripherals.as_mut() {
                                batch.cancel();
                            }
                            if this.navigation.page() == Page::Recovery {
                                this.cancel_recovery();
                                this.navigation.reset();
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
    fn completed_operation_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        JourneyScreen {
            body: recovery_guide_status(
                None,
                "Step complete",
                "This step is complete. Click Next to continue.",
                None,
                true,
                cx,
            ),
            actions: Some(
                self.footer(
                    Some(
                        button("acknowledge-operation", "Next")
                            .on_click(cx.listener(|this, _, _, cx| this.manual_next(cx))),
                    ),
                    cx,
                ),
            ),
        }
    }
    fn backup_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        if self.pending_backup.is_some() {
            return self.completed_operation_screen(cx);
        }
        if self.rescue_cancel.is_some()
            || self.pending_recovery.is_some()
            || self.pending_procedure.is_some()
        {
            return self.recovery_screen(cx);
        }
        let complete = self.backup_state.state() == BackupState::Complete
            && self
                .peripherals
                .as_ref()
                .is_some_and(|batch| batch.complete());
        let title = if complete {
            if self
                .navigation
                .scope()
                .is_some_and(|scope| scope.single().is_none())
            {
                "Your selected firmware copies are saved".to_owned()
            } else {
                "Your firmware copy is saved".to_owned()
            }
        } else if self.backup_state.state() == BackupState::Complete {
            "Preparing the next part".to_owned()
        } else if self.backup_state.failed() {
            "Let’s reconnect".to_owned()
        } else if self.operation.busy() {
            "Saving a copy…".to_owned()
        } else {
            self.backup_view()
                .map_or_else(|| "Preparing your backup".into(), |v| v.title)
        };
        let instruction = if complete {
            "Your firmware is saved locally.".to_owned()
        } else if self.backup_state.failed() {
            self.operation.error().cloned().unwrap_or_default()
        } else if self.operation.busy() {
            "Keep USB connected.".to_owned()
        } else if self.backup_view().is_some_and(|view| view.needs_wired_ack) {
            "Move the left switch to middle WIRED. Keep USB connected, then click Next.".to_owned()
        } else {
            self.backup_view()
                .map_or_else(|| "Keep USB connected.".into(), |v| v.instruction)
        };
        let mut screen = recovery_guide_status(
            self.backup_component(),
            title,
            instruction,
            None,
            complete || self.session.as_ref().is_some_and(Journey::can_next_return),
            cx,
        );
        let next = if complete {
            let rendered_ticket = self.peripherals.as_ref().map(|batch| batch.ticket());
            Some(
                button("backup-done", "Next").on_click(cx.listener(move |this, _, _, cx| {
                    if this.navigation.page() != Page::Backups
                        || !this.peripherals.as_ref().is_some_and(|batch| {
                            batch.complete() && Some(batch.ticket()) == rendered_ticket
                        })
                    {
                        return;
                    }
                    this.backup_state.transition(BackupEvent::Finish);
                    this.navigate(Page::Home, cx);
                })),
            )
        } else if self.backup_state.failed() {
            Some(
                button("retry-backup", "Next")
                    .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
            )
        } else {
            let ready = !self.operation.busy()
                && (self.backup_state.can_next()
                    || self.session.as_ref().is_some_and(Journey::can_next_return)
                    || self.backup_view().is_some_and(|v| v.can_save)
                    || (self
                        .session
                        .as_ref()
                        .is_some_and(|j| j.state() == crate::journey::State::Guiding)
                        && self.rescue_cancel.is_none())
                    || self.backup_state.state() == BackupState::Complete);
            Some(
                button("backup-step-next", "Next")
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.manual_next(cx))),
            )
        };
        if next.is_none() {
            screen = screen.child(waiting_indicator(
                if self.operation.busy() {
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

    fn scope_presence(&self) -> [crate::scope_presence::Presence; 3] {
        crate::scope_presence::derive(
            &self.device_key,
            self.recovery_locations,
            self.discovery_seen
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5)),
            self.usb_identification.bindings(),
        )
    }
    fn refresh_scope_presence(&mut self) {
        self.navigation.observe_available(
            self.scope_presence()
                .map(|state| state == crate::scope_presence::Presence::Connected),
        );
    }
    fn peripheral_card(
        &self,
        role: RecoveryRole,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let present = self.scope_presence()[role_index(role)];
        let connected = present.connected();
        let checked = self.navigation.checked(role);
        let caption = present.label();
        let id = match role {
            RecoveryRole::Left => "scope-left",
            RecoveryRole::Right => "scope-right",
            RecoveryRole::Receiver => "scope-dongle",
        };
        let mut card = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(16.))
            .child(img(peripheral_image(role)).w(px(140.)).h(px(110.)))
            .child(
                Checkbox::new(id)
                    .label(label)
                    .checked(checked)
                    .disabled(!connected)
                    .when(connected, |cb| cb.cursor_pointer())
                    .on_change(cx.listener(move |this, value, _, cx| {
                        this.refresh_scope_presence();
                        this.navigation.set_checked(role, *value);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(caption),
            );
        if present == crate::scope_presence::Presence::Unidentified {
            card = card.child(
                Button::new(format!("identify-{id}"))
                    .label("Identify")
                    .secondary()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.usb_identification.start(role);
                        cx.notify();
                    })),
            );
        }
        div()
            .flex_1()
            .border_1()
            .border_color(if checked {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .rounded(px(10.))
            .p(px(18.))
            .child(card)
    }
    fn peripheral_picker(&self, instruction: &'static str, cx: &mut Context<Self>) -> gpui::Div {
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
                    .child(instruction),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(self.peripheral_card(RecoveryRole::Left, "Left half", cx))
                    .child(self.peripheral_card(RecoveryRole::Right, "Right half", cx))
                    .child(self.peripheral_card(RecoveryRole::Receiver, "USB dongle", cx)),
            )
    }

    fn advance_recovery_batch(
        &mut self,
        observation: &Result<device::Snapshot, String>,
        observed_ticket: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.page() != Page::Recovery || self.navigation.setup() {
            return;
        }
        let detached = observation.as_ref().is_ok_and(|snapshot| {
            snapshot.mounts.is_empty() && !snapshot.devices.iter().any(device::Device::bootloader)
        });
        if let Some(batch) = self.peripherals.as_mut()
            && observed_ticket == Some(batch.ticket())
            && batch.waiting_detach()
        {
            batch.detached_at(batch.ticket(), detached, Instant::now());
        }
        cx.notify();
    }
    fn recovery_screen(&self, cx: &mut Context<Self>) -> JourneyScreen {
        if let Some(batch) = self.peripherals.as_ref().filter(|b| b.waiting_detach()) {
            return JourneyScreen {
                body: recovery_guide(
                    batch.role(),
                    "Unplug this part",
                    "Unplug its USB cable, then click Next.",
                    Some(
                        self.recovery_waiting("Waiting for USB to disconnect…", cx)
                            .into_any_element(),
                    ),
                    cx,
                ),
                actions: Some(
                    self.footer(
                        Some(
                            button("detach-next", "Next")
                                .disabled(!batch.can_next())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let role = this.peripherals.as_mut().and_then(|batch| {
                                        if batch.next(batch.ticket()) {
                                            batch.role()
                                        } else {
                                            None
                                        }
                                    });
                                    if let Some(role) = role {
                                        this.cancel_recovery();
                                        this.start_recovery(role, cx);
                                    }
                                })),
                        ),
                        cx,
                    ),
                ),
            };
        }
        let recovery_ready = self.recovery_proof_ready();
        let recovery_next = |cx: &mut Context<Self>| {
            button("recovery-step-next", "Next")
                .disabled(!recovery_ready)
                .on_click(cx.listener(|this, _, _, cx| this.recovery_next(cx)))
        };
        let (body, next) = match self.rescue.state() {
            RecoveryState::Choose => (
                self.peripheral_picker("Choose the part you want to put into recovery mode", cx),
                Some(recovery_next(cx)),
            ),
            RecoveryState::Identify(role) => (
                recovery_guide_status(
                    Some(role),
                    "Connect your device",
                    crate::recovery_journey::instruction(role),
                    Some(
                        self.recovery_waiting("Checking its firmware…", cx)
                            .into_any_element(),
                    ),
                    recovery_ready,
                    cx,
                ),
                Some(recovery_next(cx)),
            ),
            RecoveryState::Guiding(role, procedure) => (
                recovery_guide_status(
                    Some(role),
                    "Open the recovery drive",
                    procedure.instruction(role),
                    Some(
                        self.recovery_waiting("Waiting for the recovery drive…", cx)
                            .into_any_element(),
                    ),
                    recovery_ready,
                    cx,
                ),
                Some(recovery_next(cx)),
            ),
            RecoveryState::Ready(role) => (
                recovery_guide_status(
                    Some(role),
                    "Recovery drive is ready",
                    "The recovery drive is open. Your firmware hasn’t been changed.",
                    None,
                    true,
                    cx,
                ),
                Some({
                    let rendered_ticket = self.peripherals.as_ref().map(|batch| batch.ticket());
                    button("recovery-done", "Next").on_click(cx.listener(move |this, _, _, cx| {
                        if this.navigation.page() != Page::Recovery
                            || this.rescue.state() != RecoveryState::Ready(role)
                        {
                            return;
                        }
                        let Some(batch) = this.peripherals.as_mut() else {
                            return;
                        };
                        let Some(ticket) =
                            rendered_ticket.filter(|ticket| *ticket == batch.ticket())
                        else {
                            return;
                        };
                        if batch.request_next(ticket) {
                            cx.notify();
                            return;
                        }
                        if !batch.done(ticket) {
                            return;
                        }
                        this.cancel_recovery();
                        this.navigation.reset();
                        this.peripherals = None;
                        cx.notify();
                    }))
                }),
            ),
            RecoveryState::Failed(role, error) => (
                recovery_guide(Some(role), "Let’s try again", error.clone(), None, cx),
                Some(
                    button("retry-recovery", "Next").on_click(cx.listener(|this, _, _, cx| {
                        if this.firmware_page() {
                            this.cancel_recovery();
                            this.operation.clear_error();
                            this.manual_advance = true;
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

    fn install_pairing_active(&self) -> bool {
        self.firmware_page()
            && !self.navigation.setup()
            && self
                .install
                .as_ref()
                .is_some_and(|m| m.stage() == InstallStage::Pairing)
    }
    fn pairing_active(&self) -> bool {
        (self.navigation.page() == Page::Pairing && !self.navigation.setup())
            || self.install_pairing_active()
    }
    fn install_checks_active(&self) -> bool {
        self.firmware_page()
            && self.install.as_ref().is_some_and(|m| {
                matches!(
                    m.stage(),
                    InstallStage::Setup(_) | InstallStage::Typing(_) | InstallStage::Complete
                )
            })
    }
    fn observed_status(&self) -> crate::device_status::Status {
        crate::device_status::Observation {
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
        .derive()
    }
    fn observe_install(&mut self) {
        if !self.install_checks_active() {
            return;
        }
        let status = self.observed_status();
        let usb_fresh = self
            .discovery_seen
            .is_some_and(|t| t.elapsed() < Duration::from_secs(5));
        let direct_bt = status.connection == crate::status_strip::Connection::Bluetooth
            && !status.left.usb_connected
            && !status.right.usb_connected
            && !self.dongle_connected;
        let source_seen = if direct_bt {
            self.bluetooth_seen
        } else {
            self.telemetry_seen
        };
        let factory = self
            .install
            .as_ref()
            .is_some_and(|m| m.target() == install_journey::Target::Factory);
        let evidence = if factory {
            // Stock left and dongle share a descriptor and can reuse one port.
            // Fresh inventory proves only how many are present. The owner
            // confirms the intended setup before the typing check.
            let stock_count = self
                .device_key
                .iter()
                .filter(|(p, v, id, name)| {
                    crate::device::Device {
                        location: *p,
                        vendor: *v,
                        product: *id,
                        name: name.clone(),
                    }
                    .factory_left()
                })
                .count();
            let bt_seen = self
                .bluetooth_seen
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5));
            install_journey::Evidence {
                mode: None,
                route: if self.factory_bluetooth_connected {
                    crate::status_strip::Connection::Bluetooth
                } else {
                    crate::status_strip::Connection::Unknown
                },
                left_usb: None,
                right_usb: usb_fresh
                    .then_some(!crate::device_status::right_usb(&self.device_key).is_empty()),
                dongle_usb: None,
                factory_usb_count: usb_fresh.then_some(stock_count),
                right_link: None,
                bluetooth_connected: bt_seen.then_some(self.factory_bluetooth_connected),
                fresh: usb_fresh && bt_seen,
                observed_at: self.discovery_seen.unwrap_or_else(Instant::now),
            }
        } else {
            install_journey::Evidence {
                mode: status.left.mode,
                factory_usb_count: None,
                route: status.connection,
                left_usb: usb_fresh.then_some(status.left.usb_connected),
                right_usb: usb_fresh.then_some(status.right.usb_connected),
                dongle_usb: usb_fresh.then_some(self.dongle_connected),
                right_link: status.right.link_connected,
                bluetooth_connected: self.bluetooth_seen.map(|_| self.bluetooth_connected),
                fresh: usb_fresh
                    && source_seen.is_some_and(|t| t.elapsed() < Duration::from_secs(30)),
                observed_at: source_seen.unwrap_or_else(Instant::now),
            }
        };
        if let Some(install) = self.install.as_mut() {
            install.observe(install.ticket(), evidence);
        }
    }
    fn install_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) -> JourneyScreen {
        let stage = self
            .install
            .as_ref()
            .map(InstallMachine::stage)
            .unwrap_or(InstallStage::Installing);
        match stage {
            InstallStage::Installing => self.firmware_screen(cx),
            InstallStage::Pairing => {
                if matches!(self.pairing.state(), PairingState::Failed(_)) {
                    let generation = self.pairing_generation;
                    let mut screen = self.pairing_screen(cx);
                    screen.actions = Some(self.footer(
                        Some(
                            button("retry-install-pairing", "Next").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if this.install_pairing_active()
                                        && this.pairing_generation == generation
                                        && matches!(this.pairing.state(), PairingState::Failed(_))
                                    {
                                        this.start_pairing(cx);
                                        cx.notify();
                                    }
                                },
                            )),
                        ),
                        cx,
                    ));
                    screen
                } else {
                    self.pairing_screen(cx)
                }
            }
            InstallStage::Setup(mode) | InstallStage::Typing(mode) => {
                let typing = matches!(stage, InstallStage::Typing(_));
                let ticket = self.install.as_ref().expect("active install").ticket();
                let mut body = recovery_guide_status(
                    None,
                    mode.label(),
                    self.install
                        .as_ref()
                        .expect("active install")
                        .target()
                        .instruction(mode),
                    None,
                    typing || self.install.as_ref().is_some_and(InstallMachine::can_next),
                    cx,
                );
                if mode == crate::device_status::Mode::Bluetooth && !typing {
                    body = body.child(
                        Button::new("open-bluetooth-settings")
                            .secondary()
                            .cursor_pointer()
                            .label("Bluetooth settings")
                            .on_click(cx.listener(|_, _, _, _| {
                                let _ = std::process::Command::new("open")
                                    .arg("x-apple.systempreferences:com.apple.Bluetooth")
                                    .spawn();
                            })),
                    );
                }
                if typing {
                    if self
                        .typing_input
                        .as_ref()
                        .is_none_or(|(old, _, _)| *old != ticket)
                    {
                        let input =
                            cx.new(|cx| InputState::new(window, cx).placeholder("qwert HJKL h"));
                        let subscription =
                            cx.subscribe_in(&input, window, move |this, input, event, _, cx| {
                                if matches!(event, InputEvent::Change) {
                                    let text = input.read(cx).value().to_string();
                                    if let Some(install) = this.install.as_mut() {
                                        install.input(ticket, text);
                                    }
                                    cx.notify();
                                }
                            });
                        input.focus_handle(cx).focus(window, cx);
                        self.typing_input = Some((ticket, input, subscription));
                    }
                    let input = &self.typing_input.as_ref().expect("typing field").1;
                    let expected = self
                        .install
                        .as_ref()
                        .expect("active install")
                        .text()
                        .to_owned();
                    if input.read(cx).value().as_ref() != expected {
                        input.update(cx, |state, cx| state.set_value(expected, window, cx));
                    }
                    body = body.child(instruction_line("Type qwert on the left, a space, hold left Shift for right HJKL, release Shift, then a space and right h.", self.install.as_ref().is_some_and(InstallMachine::can_next), cx))
                        .child(Input::new(input));
                } else {
                    // A lost route invalidates the old confirmation and its widget together.
                    self.typing_input = None;
                    if self.install.as_ref().is_some_and(|m| m.can_next()) {
                        body = body.child(
                            div()
                                .text_center()
                                .text_color(cx.theme().muted_foreground)
                                .child("Confirm this setup, then click Next."),
                        );
                    } else {
                        body = body.child(waiting_indicator("Waiting for this connection…", cx));
                    }
                }
                let enabled = self.install.as_ref().is_some_and(InstallMachine::can_next);
                JourneyScreen {
                    body,
                    actions: Some(
                        self.footer(
                            Some(
                                button("next-mode-test", "Next")
                                    .disabled(!enabled)
                                    .when(!enabled, |b| b.cursor_default())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.observe_install();
                                        if let Some(install) = this.install.as_mut() {
                                            install.next(ticket);
                                        }
                                        cx.notify();
                                    })),
                            ),
                            cx,
                        ),
                    ),
                }
            }
            InstallStage::Complete => JourneyScreen {
                body: recovery_guide(
                    None,
                    "You’re ready",
                    if self
                        .navigation
                        .scope()
                        .is_some_and(|scope| scope.single().is_some())
                    {
                        "The selected part’s firmware is verified and it has returned to normal operation."
                    } else if self.navigation.scope() != Some(Scope::Whole) {
                        "The selected parts’ firmware is verified and they have returned to normal operation."
                    } else if self.navigation.page() == Page::Restore {
                        "Factory firmware is restored. Wired, Bluetooth and dongle typing checks are complete."
                    } else {
                        "RMK is installed. Pairing, wired, Bluetooth and dongle typing checks are complete."
                    },
                    None,
                    cx,
                ),
                actions: Some(self.footer(
                    Some(button("finish-install", "Next").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.navigation.reset();
                            this.typing_input = None;
                            cx.notify();
                        },
                    ))),
                    cx,
                )),
            },
            InstallStage::Failed(error) => JourneyScreen {
                body: recovery_guide(None, "Let’s try again", error, None, cx),
                actions: Some(self.footer(None, cx)),
            },
            InstallStage::Cancelled => self.setup_screen(cx),
        }
    }

    fn start_pairing(&mut self, cx: &mut Context<Self>) {
        if self.operation.busy() {
            return;
        }
        self.pairing = if self.install_pairing_active() {
            PairingJourney::whole_keyboard()
        } else {
            PairingJourney::new()
        };
        self.pairing_observation = None;
        self.pairing_generation += 1;
        let generation = self.pairing_generation;
        cx.spawn(async move |this, cx| {
            loop {
                let allowed = this.update(cx, |this, _| {
                    this.pairing_active()
                        && this.pairing_generation == generation
                        && !matches!(
                            this.pairing.state(),
                            PairingState::Cancelled | PairingState::Failed(_)
                        )
                        && !(*this.pairing.state() == PairingState::Connected
                            && (!this.install_pairing_active()
                                || this
                                    .pairing_observation
                                    .as_ref()
                                    .is_some_and(|o| o.dongle.is_some())))
                });
                if !matches!(allowed, Ok(true)) {
                    break;
                }
                let busy = this
                    .update(cx, |this, _| this.operation.busy())
                    .unwrap_or(true);
                if !busy {
                    let result = cx
                        .background_executor()
                        .spawn(async { dongle_pairing::query() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if !this.pairing_active()
                            || this.pairing_generation != generation
                            || this.operation.busy()
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
        if self.operation.busy() {
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
        let Some(ticket) = self
            .operation
            .begin(crate::operation::Kind::Pairing, None, None)
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { dongle_pairing::begin(request) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.pairing_generation != generation || !this.pairing_active() {
                    return;
                }
                if !this.operation.complete(ticket) {
                    return;
                }
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
                "Connect the left half by USB. Keep the dongle plugged in to check it too.",
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
                "Keep both USB connections in place.",
                Some("Pairing your dongle…"),
            ),
            PairingState::Connected => (
                if self
                    .pairing_observation
                    .as_ref()
                    .is_some_and(|o| o.dongle.is_some())
                {
                    "Pairing check passed for both halves and your dongle."
                } else {
                    "Pairing check passed for both keyboard halves."
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
            .child(instruction_line(
                instruction.to_owned(),
                matches!(
                    self.pairing.state(),
                    PairingState::Ready | PairingState::Connected
                ) || self.pairing.can_next(),
                cx,
            ));
        if let Some(label) = waiting {
            body = body.child(waiting_indicator(label, cx));
        }
        let generation = self.pairing_generation;
        let standalone = self.navigation.page() == Page::Pairing;
        let ready = self.pairing.can_next()
            || matches!(
                self.pairing.state(),
                PairingState::Ready | PairingState::Connected | PairingState::Failed(_)
            );
        let next = Some(
            button("pairing-step-next", "Next")
                .disabled(!ready || self.operation.busy())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.operation.busy()
                        || !this.pairing_active()
                        || this.pairing_generation != generation
                    {
                        return;
                    }
                    if this.pairing.can_next() {
                        this.pairing.next();
                    } else {
                        match this.pairing.state() {
                            PairingState::Ready => this.begin_pairing(cx),
                            PairingState::Connected => {
                                if standalone {
                                    this.pairing.cancel();
                                    this.navigation.reset();
                                } else if let Some(install) = this.install.as_mut() {
                                    install.paired(install.ticket(), Ok(()));
                                }
                            }
                            PairingState::Failed(_) => this.start_pairing(cx),
                            _ => {}
                        }
                    }
                    cx.notify();
                })),
        );
        JourneyScreen {
            body,
            actions: Some(self.footer(next, cx)),
        }
    }

    fn start_firmware(&mut self, cx: &mut Context<Self>) {
        if self.operation.busy() {
            return;
        }
        self.cancel_recovery();
        self.operation.clear_error();
        self.firmware = None;
        self.pending_firmware = None;
        let factory = self.navigation.page() == Page::Restore;
        let scope = self.navigation.scope().expect("Started firmware scope");
        self.install = Some(InstallMachine::scoped(
            if factory {
                install_journey::Target::Factory
            } else {
                install_journey::Target::Rmk
            },
            scope,
        ));
        let originals = self.factory_release.clone();
        self.typing_input = None;

        let Some(ticket) =
            self.operation
                .begin(crate::operation::Kind::PrepareFirmware, None, None)
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if factory {
                        FirmwareJourney::factory_scoped(
                            originals
                                .ok_or("Choose factory firmware for the selected parts first.")?,
                            scope,
                        )
                    } else {
                        FirmwareRelease::bundled()
                            .map(|release| FirmwareJourney::new_scoped(release, scope))
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.operation.complete(ticket) {
                    return;
                }
                match result {
                    Ok(mut journey) => {
                        journey.authorize_install();

                        this.pending_firmware = Some(journey);
                    }
                    Err(error) => this.operation.fail(error),
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
        let Some(ticket) = self.operation.begin(
            crate::operation::Kind::Firmware,
            Some(journey.view().role),
            Some(journey.view()),
        ) else {
            self.firmware = Some(journey);
            return;
        };
        self.operation.clear_error();
        cx.spawn(async move |this, cx| {
            let (journey, result) = cx
                .background_executor()
                .spawn(async move {
                    let result = work(&mut journey);
                    (journey, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.operation.complete(ticket) {
                    return;
                }

                match result {
                    Ok(()) => this.pending_firmware = Some(journey),
                    Err(error) => {
                        this.firmware = Some(journey);
                        this.operation.fail(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn advance_firmware(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.manual_advance) {
            return;
        }
        if !self.firmware_page()
            || self.navigation.setup()
            || self.operation.busy()
            || self.operation.error().is_some()
            || self.rescue_cancel.is_some()
        {
            return;
        }
        let Some(view) = self.firmware_view() else {
            return;
        };
        if !self
            .install
            .as_ref()
            .is_some_and(|m| m.stage() == InstallStage::Installing)
        {
            return;
        }
        if view.complete {
            if let Some(journey) = self.firmware.as_ref() {
                for (role, location) in journey.verified_locations() {
                    self.recovery_locations[role_index(role)] = Some(location);
                }
            }
            if let Some(install) = self.install.as_mut() {
                install.installed(install.ticket(), Ok(()));
            }
            self.cancel_recovery();
            if self.install_pairing_active() {
                self.start_pairing(cx);
            }
            return;
        }
        if view.can_transfer {
            self.firmware_job(cx, |journey| journey.transfer_if_ready().map(|_| ()));
            return;
        }
        if !view.complete && view.needs_recovery {
            let role = view.role;
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
                                if !this.firmware_page() || !this.rescue_cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancelled)) || cancelled.load(Ordering::Relaxed) { return; }
                                this.rescue_cancel = None;
                                match result {
                                    Ok(session) => {
                                        if let Ok((_, location, _)) = session.recovery_binding() { this.recovery_locations[role_index(role)] = Some(location); }
                                        this.pending_recovery = Some((attempt, role, session));
                                    }
                                    Err(error) => {
                                        this.rescue.complete(attempt, role, Err(error.clone()));
                                        this.operation.fail(error);
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
                                        if this.rescue.state() != RecoveryState::Guiding(role, procedure) { this.pending_procedure = Some((attempt, role, procedure)); }
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
        if self.pending_firmware.is_some() {
            return self.completed_operation_screen(cx);
        }
        if let Some(error) = self.operation.error() {
            return JourneyScreen {
                body: recovery_guide(
                    self.firmware_view().as_ref().map(|v| v.role),
                    "Let’s reconnect",
                    error.clone(),
                    None,
                    cx,
                ),
                actions: Some(
                    self.footer(
                        Some(
                            button("check-firmware-again", "Next")
                                .disabled(self.operation.busy())
                                .when(self.operation.busy(), |button| button.cursor_default())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.operation.clear_error();
                                    if this.firmware.is_none() {
                                        this.start_firmware(cx);
                                    } else {
                                        this.manual_advance = true;
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
        if self.rescue_cancel.is_some()
            || self.pending_recovery.is_some()
            || self.pending_procedure.is_some()
        {
            return self.recovery_screen(cx);
        }
        let Some(view) = self.firmware_view() else {
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
        let mut body = recovery_guide_status(
            Some(view.role),
            view.title.clone(),
            if self.operation.busy() {
                "Keep USB connected.".to_owned()
            } else if view.needs_wired_ack {
                "Move the left switch to middle WIRED. Keep USB connected, then click Next."
                    .to_owned()
            } else {
                view.error
                    .clone()
                    .unwrap_or_else(|| view.instruction.clone())
            },
            None,
            view.complete
                || self
                    .firmware
                    .as_ref()
                    .is_some_and(FirmwareJourney::can_next),
            cx,
        );
        body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(16.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(view.title.clone()),
            )
            .child(body);
        let ready = !self.operation.busy()
            && (view.can_transfer
                || view.needs_recovery
                || view.complete
                || self
                    .firmware
                    .as_ref()
                    .is_some_and(FirmwareJourney::can_next));
        if self.operation.busy() {
            body = body.child(waiting_indicator("Working…", cx));
        }
        let next = Some(
            button("firmware-step-next", "Next")
                .disabled(!ready)
                .on_click(cx.listener(|this, _, _, cx| this.manual_next(cx))),
        );
        JourneyScreen {
            body,
            actions: Some(self.footer(next, cx)),
        }
    }

    fn start_copies(&mut self, role: RecoveryRole, cx: &mut Context<Self>) {
        if self.backup_state.active() || self.operation.busy() {
            return;
        }
        if !self.backup_state.transition(BackupEvent::Select) {
            return;
        }
        if let Some(journey) = self.session.as_mut()
            && journey.state() == crate::journey::State::Paused
            && journey.component() == role
        {
            journey.resume();

            self.operation.clear_error();
            self.advance(cx);
            cx.notify();
            return;
        }
        let session = Journey::backup_part(role);
        self.operation.clear_error();

        self.session = Some(session);

        self.advance(cx);
        cx.notify();
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        if let Some(journey) = self.session.as_ref() {
            self.backup_state
                .transition(BackupEvent::Observed(journey.state()));
        }
        if !std::mem::take(&mut self.manual_advance) {
            return;
        }
        if self.navigation.page() == Page::Backups
            && !self.navigation.setup()
            && self.backup_state.state() == BackupState::Complete
        {
            let next = self.peripherals.as_mut().and_then(|batch| {
                if batch.done(batch.ticket()) {
                    batch.role()
                } else {
                    None
                }
            });
            if let Some(role) = next {
                self.session = None;
                self.start_copies(role, cx);
            }
            return;
        }
        if !self.backup_state.active()
            || self.operation.busy()
            || self.backup_state.failed()
            || (self.backup_state.state() == BackupState::Complete)
        {
            return;
        }
        if let Some(journey) = self.session.as_ref() {
            self.backup_state
                .transition(BackupEvent::Observed(journey.state()));
        }
        if self.backup_state.state() == BackupState::Complete {
            self.advance(cx);
            return;
        }
        if self
            .session
            .as_ref()
            .is_some_and(|j| j.state() == crate::journey::State::Guiding)
            && !self.backup_state.recovery()
        {
            self.cancel_recovery();
            let Some(role) = self.backup_component() else {
                return;
            };
            if let Some(attempt) = self.rescue.start(role) {
                self.run_recovery(role, attempt, true, cx);
            }
            return;
        }
        if self.backup_view().is_some_and(|v| v.can_save)
            && self.backup_state.state() != BackupState::Complete
        {
            self.save(cx);
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        if self.operation.busy() {
            return;
        }
        if self.backup_state.state() == BackupState::RecoveryFailed {
            if let Some((role, attempt)) = self.rescue.retry() {
                self.run_recovery(role, attempt, true, cx);
            }
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.retry();
            self.backup_state.transition(BackupEvent::Retry);

            self.operation.clear_error();
            cx.notify();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.backup_state.active()
            || !self.backup_view().is_some_and(|v| v.can_save)
            || self.operation.busy()
            || self.backup_state.failed()
        {
            return;
        }
        let Some(mut session) = self.session.take() else {
            return;
        };
        self.backup_state.transition(BackupEvent::SaveStarted);
        let Some(ticket) = self.operation.begin(
            crate::operation::Kind::Backup,
            Some(session.component()),
            None,
        ) else {
            self.session = Some(session);
            return;
        };
        self.operation.clear_error();
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
                if !this.operation.complete(ticket) {
                    return;
                }
                match result {
                    Ok(path) => {
                        this.pending_backup = Some(session);
                        this.copies_folder = path.parent().map(PathBuf::from);
                        this.operation.clear_error();
                    }
                    Err(error) => {
                        this.session = Some(session);
                        this.backup_state
                            .transition(BackupEvent::Observed(crate::journey::State::Failed));
                        this.operation
                            .fail(format!("Your firmware copy could not be saved. {error}"));
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
        if self.operation.busy() || self.navigation.page() == page {
            return;
        }
        self.pending_firmware = None;
        self.pending_backup = None;
        self.usb_identification.cancel();
        if let Some(batch) = self.peripherals.as_mut() {
            batch.cancel();
        }
        self.peripherals = None;
        let leaving_backup = self.backup_state.active() && page != Page::Backups;
        if self.backup_state.active() && page != Page::Backups {
            if let Some(journey) = self.session.as_mut() {
                journey.pause();
            }
            self.backup_state.transition(BackupEvent::Pause);
        }
        if (self.navigation.page() == Page::Recovery && page != Page::Recovery)
            || leaving_backup
            || (self.firmware_page() && page != self.navigation.page())
        {
            self.cancel_recovery();
        }
        if self.firmware_page()
            && page != self.navigation.page()
            && let Some(journey) = self.firmware.as_mut()
            && !journey.view().complete
        {
            journey.cancel();
        }
        if self.firmware_page() && page != self.navigation.page() {
            if let Some(install) = self.install.as_mut() {
                install.cancel();
            }
            self.pairing.cancel();
            self.pairing_generation += 1;
            self.typing_input = None;
        }
        if self.navigation.page() == Page::Pairing && page != Page::Pairing {
            self.pairing.cancel();
            self.pairing_generation += 1;
        }
        self.operation.clear_error();
        self.navigation.navigate(page);
        if page == Page::Firmware {
            self.version_refresh = true;
            self.versions_seen = None;
        }
        self.observe_preflight();
        if page == Page::Restore {
            self.load_factory_sources(None, cx);
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
            .when(!self.operation.busy(), |item| item.cursor_pointer())
            .mb(px(6.))
            .h(px(36.))
            .icon(icon)
            .active(
                self.navigation.page() == page
                    || (page == Page::Backups && self.navigation.page() == Page::Home),
            )
            .disable(self.operation.busy())
            .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx)))
    }
}

impl Render for Companion {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_scope_presence();
        if self.appearance_subscription.is_none() {
            Theme::sync_system_appearance(Some(window), cx);
            self.appearance_subscription =
                Some(cx.observe_window_appearance(window, |_, window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    cx.notify();
                }));
        }
        let tasks = SidebarGroup::new("Tasks")
            .child(self.nav_row("Install RMK", Page::Firmware, IconName::Download, cx))
            .child(self.nav_row("Restore factory", Page::Restore, IconName::Undo, cx));
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

        let title = match self.navigation.page() {
            Page::Backups | Page::Home => "Backup firmware",
            Page::Recovery => "Enter recovery mode",
            Page::Pairing => "Check pairing",
            Page::Firmware => "Install RMK",
            Page::Restore => "Restore factory",
        };
        let mut heading = div().flex().flex_col().gap(px(24.)).w_full().child(
            div()
                .text_size(px(23.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        );
        if let Some(batch) = self
            .peripherals
            .as_ref()
            .filter(|b| b.len() > 1 && !self.navigation.setup())
        {
            heading = heading.child(
                Stepper::new("peripheral-steps")
                    .small()
                    .disabled(true)
                    .selected_index(if batch.complete() {
                        batch.len()
                    } else {
                        batch.index()
                    })
                    .items(
                        self.navigation
                            .scope()
                            .unwrap_or(Scope::Whole)
                            .roles()
                            .into_iter()
                            .map(|role| StepperItem::new().child(Scope::Part(role).label())),
                    ),
            );
        }
        match self.navigation.page() {
            Page::Backups
                if !self.navigation.setup()
                    && self.backup_state.shown()
                    && self
                        .navigation
                        .scope()
                        .is_some_and(|scope| scope.single().is_some()) =>
            {
                let progress = if self.backup_state.state() == BackupState::Saving {
                    Some(flow_presentation::saving_backup())
                } else {
                    self.session.as_ref().map(flow_presentation::backup)
                };
                if let Some(progress) = progress {
                    heading = heading.child(flow_indicator(progress, "backup-steps", cx));
                }
            }
            Page::Firmware | Page::Restore
                if !self.navigation.setup()
                    && matches!(self.navigation.scope(), Some(Scope::Pair(_, _))) =>
            {
                let scope = self.navigation.scope().expect("Active scope");
                let order = if self.navigation.page() == Page::Restore {
                    [
                        RecoveryRole::Left,
                        RecoveryRole::Right,
                        RecoveryRole::Receiver,
                    ]
                } else {
                    [
                        RecoveryRole::Receiver,
                        RecoveryRole::Right,
                        RecoveryRole::Left,
                    ]
                };
                let labels: Vec<_> = order
                    .into_iter()
                    .filter(|role| scope.contains(*role))
                    .map(|role| Scope::Part(role).label())
                    .collect();
                let current = if self
                    .install
                    .as_ref()
                    .is_some_and(|machine| machine.stage() == InstallStage::Complete)
                {
                    labels.len()
                } else {
                    self.firmware_view().map_or(0, |view| view.step)
                };
                heading = heading.child(
                    Stepper::new("install-steps")
                        .small()
                        .disabled(true)
                        .selected_index(current)
                        .items(
                            labels
                                .into_iter()
                                .map(|label| StepperItem::new().child(label)),
                        ),
                );
            }
            Page::Firmware | Page::Restore
                if !self.navigation.setup() && self.navigation.scope() == Some(Scope::Whole) =>
            {
                let current = match self.install.as_ref().map(InstallMachine::stage) {
                    Some(InstallStage::Installing) | None => {
                        self.firmware_view().map_or(0, |v| v.step)
                    }
                    Some(InstallStage::Pairing) => 3,
                    Some(InstallStage::Setup(_)) | Some(InstallStage::Typing(_)) => {
                        if self.navigation.page() == Page::Restore {
                            3
                        } else {
                            4
                        }
                    }
                    Some(InstallStage::Complete) => {
                        if self.navigation.page() == Page::Restore {
                            4
                        } else {
                            5
                        }
                    }
                    _ => 0,
                };
                heading = heading.child(
                    Stepper::new("install-steps")
                        .small()
                        .selected_index(current)
                        .disabled(true)
                        .items(
                            (if self.navigation.page() == Page::Restore {
                                vec!["Left half", "Right half", "Dongle", "Test modes"]
                            } else {
                                vec!["Dongle", "Right half", "Left half", "Pairing", "Test modes"]
                            })
                            .into_iter()
                            .map(|label| StepperItem::new().child(label)),
                        ),
                );
            }
            _ => {}
        }
        let screen = if self.navigation.setup() {
            self.setup_screen(cx)
        } else {
            match self.navigation.page() {
                Page::Backups if self.backup_state.recovery() => self.recovery_screen(cx),
                Page::Backups if self.backup_state.shown() => self.backup_screen(cx),
                Page::Backups | Page::Home => JourneyScreen {
                    body: self.peripheral_picker("Choose the part you want to back up", cx),
                    actions: None,
                },
                Page::Recovery => self.recovery_screen(cx),
                Page::Pairing => self.pairing_screen(cx),
                Page::Firmware | Page::Restore => self.install_screen(window, cx),
            }
        };
        let canvas = div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .whitespace_normal()
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
                    .min_w_0()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .min_w_0()
                            .w_full()
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
        let observed = self.observed_status();
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
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(navigation)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .id("journey-canvas")
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
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
    title: impl Into<gpui::SharedString>,
    instruction: impl Into<gpui::SharedString>,
    controls: Option<gpui::AnyElement>,
    cx: &App,
) -> gpui::Div {
    recovery_guide_status(role, title, instruction, controls, false, cx)
}

fn instruction_line(
    instruction: impl Into<gpui::SharedString>,
    satisfied: bool,
    cx: &App,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .w_full()
        .max_w(px(680.))
        .min_w(px(0.))
        .when(satisfied, |row| {
            row.child(
                Icon::new(IconName::Check)
                    .size(px(18.))
                    .flex_shrink_0()
                    .text_color(gpui::rgb(0x22c55e)),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .whitespace_normal()
                .text_center()
                .text_size(px(15.))
                .line_height(px(23.))
                .text_color(cx.theme().muted_foreground)
                .child(instruction.into()),
        )
}

fn recovery_guide_status(
    role: Option<RecoveryRole>,
    _title: impl Into<gpui::SharedString>,
    instruction: impl Into<gpui::SharedString>,
    controls: Option<gpui::AnyElement>,
    satisfied: bool,
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
        .child(instruction_line(instruction, satisfied, cx))
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
        .min_w_0()
        .max_w_full()
        .text_size(px(13.))
        .text_color(cx.theme().muted_foreground)
        .child(Spinner::new().small())
        .child(div().min_w_0().whitespace_normal().child(label))
}

fn button(id: &'static str, label: impl Into<gpui::SharedString>) -> Button {
    Button::new(id)
        .cursor_pointer()
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

fn scope_guide(scope: Scope, instruction: &str, cx: &App) -> gpui::Div {
    let mut artwork = div().flex().items_center().justify_center().gap(px(18.));
    for role in scope.roles() {
        artwork = artwork.child(img(peripheral_image(role)).w(px(140.)).h(px(110.)));
    }
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(24.))
        .child(artwork)
        .child(div().font_weight(FontWeight::MEDIUM).child(scope.label()))
        .child(instruction_line(instruction.to_owned(), false, cx))
}
