//! Install coordinates child completion and three observable typing checks.
//! Firmware and pairing retain their own machines; this parent performs no IO.
use crate::{device_status::Mode, scope::Scope, status_strip::Connection};
use statig::prelude::*;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) const TOKEN: &str = "qwert HJKL h";
const EVIDENCE_AGE: Duration = Duration::from_secs(45);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Target {
    #[default]
    Rmk,
    Factory,
}
impl Target {
    pub(crate) fn instruction(self, mode: Mode) -> &'static str {
        if self == Self::Rmk {
            return mode.install_instruction();
        }
        match mode {
            Mode::Wired => {
                "Connect left USB and move its switch to WIRED. Keep right ON with USB unplugged. Unplug the dongle and disconnect direct Bluetooth, then click Next."
            }
            Mode::Bluetooth => {
                "Unplug both halves and the dongle. Move left to Bluetooth, keep right ON and connect NocFree in Bluetooth settings, then click Next."
            }
            Mode::Dongle => {
                "Unplug both halves. Connect the dongle, move left to DONGLE and keep right ON. Disconnect direct Bluetooth, then click Next."
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ticket {
    generation: u64,
    phase: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Installing,
    Pairing,
    Setup(Mode),
    Typing(Mode),
    Complete,
    Cancelled,
    Failed(String),
}
/// Unknown cable/link observations must remain unknown, rather than becoming absence.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Evidence {
    pub mode: Option<Mode>,
    pub route: Connection,
    pub left_usb: Option<bool>,
    pub right_usb: Option<bool>,
    pub dongle_usb: Option<bool>,
    pub right_link: Option<bool>,
    pub bluetooth_connected: Option<bool>,
    pub factory_usb_count: Option<usize>,
    pub fresh: bool,
    pub observed_at: Instant,
}
impl Evidence {
    fn ready(self, mode: Mode, target: Target) -> bool {
        if target == Target::Factory {
            return self.fresh
                && self.observed_at.elapsed() <= EVIDENCE_AGE
                && self.right_usb == Some(false)
                && match mode {
                    Mode::Wired => {
                        self.factory_usb_count == Some(1)
                            && self.bluetooth_connected == Some(false)
                            && matches!(self.route, Connection::Unknown | Connection::Usb)
                    }
                    Mode::Bluetooth => {
                        self.factory_usb_count == Some(0)
                            && self.bluetooth_connected == Some(true)
                            && self.route == Connection::Bluetooth
                    }
                    Mode::Dongle => {
                        self.factory_usb_count == Some(1)
                            && self.bluetooth_connected == Some(false)
                            && matches!(self.route, Connection::Unknown | Connection::Dongle)
                    }
                };
        }
        self.fresh
            && self.observed_at.elapsed() <= EVIDENCE_AGE
            && self.right_usb == Some(false)
            && match mode {
                Mode::Wired => {
                    self.mode == Some(mode)
                        && self.right_link == Some(true)
                        && self.route == Connection::Usb
                        && self.left_usb == Some(true)
                }
                Mode::Bluetooth => {
                    // With USB absent there is no firmware status producer. The
                    // live macOS link proves this route; the owner's exact token
                    // checks the split and shared modifier without inventing telemetry.
                    self.mode.is_none_or(|selected| selected == mode)
                        && self.right_link.is_none_or(|connected| connected)
                        && self.route == Connection::Bluetooth
                        && self.left_usb == Some(false)
                        && self.dongle_usb == Some(false)
                }
                Mode::Dongle => {
                    self.mode == Some(mode)
                        && self.right_link == Some(true)
                        && self.route == Connection::Dongle
                        && self.left_usb == Some(false)
                        && self.dongle_usb == Some(true)
                }
            }
    }
    fn same_setup(self, other: Self) -> bool {
        self.mode == other.mode
            && self.route == other.route
            && self.left_usb == other.left_usb
            && self.right_usb == other.right_usb
            && self.dongle_usb == other.dongle_usb
            && self.right_link == other.right_link
            && self.fresh == other.fresh
            && self.bluetooth_connected == other.bluetooth_connected
            && self.factory_usb_count == other.factory_usb_count
    }
}
impl Mode {
    pub(crate) fn install_instruction(self) -> &'static str {
        match self {
            Self::Wired => {
                "Connect left USB and move its switch to WIRED. Keep right ON with USB unplugged."
            }
            Self::Bluetooth => {
                "Unplug both halves and the dongle. Move left to Bluetooth and keep right ON. Connect NocFree RMK in Bluetooth settings. If it is not listed, hold Fn + 1 for five seconds, then connect."
            }
            Self::Dongle => {
                "Unplug both halves. Connect the dongle, move left to DONGLE and keep right ON."
            }
        }
    }
}

mod machine {
    use super::*;
    pub struct Install {
        pub generation: u64,
        pub phase: u64,
        pub target: Target,
        pub scope: Scope,
    }
    pub enum Event {
        Installed(Ticket, Result<(), String>),
        Paired(Ticket, Result<(), String>),
        Observed(Ticket, Evidence),
        Input(Ticket, String),
        Next(Ticket),
        Cancel,
    }
    impl Install {
        fn current(&self, ticket: Ticket) -> bool {
            ticket.generation == self.generation && ticket.phase == self.phase
        }
        fn advance(&mut self) {
            self.phase = self.phase.checked_add(1).expect("Install phase exhausted");
        }
    }
    #[state_machine(initial = "State::installing()", state(derive(Debug)))]
    impl Install {
        #[state(superstate = "cancellable")]
        fn installing(&mut self, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Installed(ticket, result) if self.current(*ticket) => {
                    *context = true;
                    match result {
                        Ok(()) => {
                            self.advance();
                            if matches!(self.scope, Scope::Part(_)) {
                                return Transition(State::complete());
                            }
                            match self.target {
                                Target::Rmk => Transition(State::pairing()),
                                Target::Factory => Transition(State::checking(
                                    Mode::Wired,
                                    None,
                                    String::new(),
                                    false,
                                )),
                            }
                        }
                        Err(error) => Transition(State::failed(error.clone())),
                    }
                }
                _ => Super,
            }
        }
        #[state(superstate = "cancellable")]
        fn pairing(&mut self, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Paired(ticket, result) if self.current(*ticket) => {
                    *context = true;
                    match result {
                        Ok(()) => {
                            self.advance();
                            Transition(State::checking(Mode::Wired, None, String::new(), true))
                        }
                        Err(error) => Transition(State::failed(error.clone())),
                    }
                }
                _ => Super,
            }
        }
        #[state(superstate = "cancellable")]
        #[allow(clippy::ptr_arg)] // Statig generates owned String state storage from this argument.
        fn checking(
            &mut self,
            mode: &Mode,
            evidence: &Option<Evidence>,
            input: &String,
            armed: &bool,
            event: &Event,
            context: &mut bool,
        ) -> Outcome<State> {
            match event {
                Event::Observed(ticket, latest) if self.current(*ticket) => {
                    *context = true;
                    let changed = evidence.is_some_and(|old| {
                        !old.same_setup(*latest)
                            || old.ready(*mode, self.target) != latest.ready(*mode, self.target)
                    });
                    if changed {
                        // A recreated typing widget must not accept callbacks from
                        // the earlier route, even when that same route returns.
                        self.advance();
                    }
                    let keep = evidence.is_some_and(|old| {
                        old.ready(*mode, self.target)
                            && latest.ready(*mode, self.target)
                            && old.same_setup(*latest)
                    });
                    Transition(State::checking(
                        *mode,
                        Some(*latest),
                        if keep { input.clone() } else { String::new() },
                        *armed && !changed || self.target == Target::Rmk,
                    ))
                }
                Event::Input(ticket, text)
                    if self.current(*ticket)
                        && *armed
                        && evidence.is_some_and(|e| e.ready(*mode, self.target)) =>
                {
                    *context = true;
                    Transition(State::checking(*mode, *evidence, text.clone(), *armed))
                }
                Event::Next(ticket)
                    if self.current(*ticket)
                        && self.target == Target::Factory
                        && !*armed
                        && evidence.is_some_and(|e| e.ready(*mode, self.target)) =>
                {
                    *context = true;
                    self.advance();
                    Transition(State::checking(*mode, *evidence, String::new(), true))
                }
                Event::Next(ticket)
                    if self.current(*ticket)
                        && evidence.is_some_and(|e| e.ready(*mode, self.target))
                        && *armed
                        && input.trim() == TOKEN =>
                {
                    *context = true;
                    self.advance();
                    match mode {
                        Mode::Wired => Transition(State::checking(
                            Mode::Bluetooth,
                            None,
                            String::new(),
                            self.target == Target::Rmk,
                        )),
                        Mode::Bluetooth => Transition(State::checking(
                            Mode::Dongle,
                            None,
                            String::new(),
                            self.target == Target::Rmk,
                        )),
                        Mode::Dongle => Transition(State::complete()),
                    }
                }
                _ => Super,
            }
        }
        #[superstate]
        fn cancellable(&mut self, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Cancel => {
                    *context = true;
                    Transition(State::cancelled())
                }
                _ => Super,
            }
        }
        #[state]
        fn complete(event: &Event) -> Outcome<State> {
            let _ = event;
            Super
        }
        #[state]
        fn cancelled(event: &Event) -> Outcome<State> {
            let _ = event;
            Super
        }
        #[state(superstate = "cancellable")]
        fn failed(error: &String, event: &Event) -> Outcome<State> {
            let _ = (error, event);
            Super
        }
    }
}

pub(crate) struct Machine {
    machine: statig::blocking::StateMachine<machine::Install>,
}
impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}
impl Machine {
    pub fn new() -> Self {
        Self::scoped(Target::Rmk, Scope::Whole)
    }
    #[cfg(test)]
    pub fn factory() -> Self {
        Self::scoped(Target::Factory, Scope::Whole)
    }
    pub fn target(&self) -> Target {
        self.machine.inner().target
    }
    pub fn scoped(target: Target, scope: Scope) -> Self {
        Self {
            machine: machine::Install {
                generation: GENERATION
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                        value.checked_add(1)
                    })
                    .expect("Install generation exhausted"),
                phase: 0,
                target,
                scope,
            }
            .state_machine(),
        }
    }
    pub fn ticket(&self) -> Ticket {
        let inner = self.machine.inner();
        Ticket {
            generation: inner.generation,
            phase: inner.phase,
        }
    }
    pub fn stage(&self) -> Stage {
        match self.machine.state() {
            machine::State::Installing {} => Stage::Installing,
            machine::State::Pairing {} => Stage::Pairing,
            machine::State::Checking {
                mode,
                evidence,
                armed,
                ..
            } => {
                if *armed && evidence.is_some_and(|e| e.ready(*mode, self.target())) {
                    Stage::Typing(*mode)
                } else {
                    Stage::Setup(*mode)
                }
            }
            machine::State::Complete {} => Stage::Complete,
            machine::State::Cancelled {} => Stage::Cancelled,
            machine::State::Failed { error } => Stage::Failed(error.clone()),
        }
    }
    pub fn can_next(&self) -> bool {
        matches!(self.machine.state(), machine::State::Checking {mode, evidence, input, armed}
            if evidence.is_some_and(|e| e.ready(*mode, self.target()))
                && ((!*armed && self.target() == Target::Factory) || (*armed && input.trim() == TOKEN)))
    }
    pub fn text(&self) -> &str {
        match self.machine.state() {
            machine::State::Checking { input, .. } => input,
            _ => "",
        }
    }
    fn dispatch(&mut self, event: machine::Event) -> bool {
        let mut accepted = false;
        self.machine.handle_with_context(&event, &mut accepted);
        accepted
    }
    pub fn installed(&mut self, ticket: Ticket, result: Result<(), String>) -> bool {
        self.dispatch(machine::Event::Installed(ticket, result))
    }
    pub fn paired(&mut self, ticket: Ticket, result: Result<(), String>) -> bool {
        self.dispatch(machine::Event::Paired(ticket, result))
    }
    pub fn observe(&mut self, ticket: Ticket, evidence: Evidence) -> bool {
        self.dispatch(machine::Event::Observed(ticket, evidence))
    }
    pub fn input(&mut self, ticket: Ticket, text: String) -> bool {
        self.dispatch(machine::Event::Input(ticket, text))
    }
    pub fn next(&mut self, ticket: Ticket) -> bool {
        self.dispatch(machine::Event::Next(ticket))
    }
    pub fn cancel(&mut self) -> bool {
        self.dispatch(machine::Event::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evidence(mode: Mode) -> Evidence {
        Evidence {
            mode: Some(mode),
            route: match mode {
                Mode::Wired => Connection::Usb,
                Mode::Bluetooth => Connection::Bluetooth,
                Mode::Dongle => Connection::Dongle,
            },
            left_usb: Some(mode == Mode::Wired),
            right_usb: Some(false),
            dongle_usb: Some(mode == Mode::Dongle),
            right_link: Some(true),
            bluetooth_connected: Some(mode == Mode::Bluetooth),
            factory_usb_count: None,
            fresh: true,
            observed_at: Instant::now(),
        }
    }
    fn checks() -> Machine {
        let mut machine = Machine::new();
        assert!(machine.installed(machine.ticket(), Ok(())));
        assert!(machine.paired(machine.ticket(), Ok(())));
        machine
    }
    fn factory_evidence(mode: Mode) -> Evidence {
        let mut result = evidence(mode);
        result.mode = None;
        result.right_link = None;
        result.left_usb = None;
        result.dongle_usb = None;
        result.factory_usb_count = Some(usize::from(mode != Mode::Bluetooth));
        if mode != Mode::Bluetooth {
            result.route = Connection::Unknown;
        }
        result
    }
    #[test]
    fn part_install_completes_without_pairing_or_whole_keyboard_checks() {
        for target in [Target::Rmk, Target::Factory] {
            for role in Scope::Whole.roles() {
                let mut machine = Machine::scoped(target, Scope::Part(role));
                let ticket = machine.ticket();
                assert_eq!(machine.stage(), Stage::Installing);
                assert!(!machine.paired(ticket, Ok(())));
                assert!(machine.installed(ticket, Ok(())));
                assert_eq!(machine.stage(), Stage::Complete);
                assert!(!machine.paired(machine.ticket(), Ok(())));
                assert!(!machine.observe(machine.ticket(), evidence(Mode::Wired)));
                assert!(!machine.input(machine.ticket(), TOKEN.into()));
                assert!(!machine.next(machine.ticket()));
                assert!(!machine.installed(ticket, Ok(())));
                let mut failed = Machine::scoped(target, Scope::Part(role));
                assert!(failed.installed(failed.ticket(), Err("verification failed".into())));
                assert!(matches!(failed.stage(), Stage::Failed(_)));
            }
        }
    }
    #[test]
    fn factory_skips_rmk_pairing_and_requires_setup_ack_then_typing_for_every_mode() {
        let mut machine = Machine::factory();
        assert_eq!(machine.target(), Target::Factory);
        assert!(machine.installed(machine.ticket(), Ok(())));
        assert_eq!(machine.stage(), Stage::Setup(Mode::Wired));
        assert!(!machine.paired(machine.ticket(), Ok(())));
        for mode in [Mode::Wired, Mode::Bluetooth, Mode::Dongle] {
            assert!(machine.observe(machine.ticket(), factory_evidence(mode)));
            assert_eq!(machine.stage(), Stage::Setup(mode));
            assert!(machine.can_next());
            assert!(!machine.input(machine.ticket(), TOKEN.into()));
            let setup_ticket = machine.ticket();
            assert!(machine.next(setup_ticket));
            assert_eq!(machine.stage(), Stage::Typing(mode));
            assert!(!machine.next(setup_ticket));
            assert!(!machine.can_next());
            assert!(machine.input(machine.ticket(), TOKEN.into()));
            assert!(machine.next(machine.ticket()));
        }
        assert_eq!(machine.stage(), Stage::Complete);
    }
    #[test]
    fn factory_setup_requires_known_usb_and_bluetooth_facts_and_resets_after_route_loss() {
        let mut machine = Machine::factory();
        machine.installed(machine.ticket(), Ok(()));
        for change in 0..4 {
            let mut invalid = factory_evidence(Mode::Wired);
            match change {
                0 => invalid.bluetooth_connected = None,
                1 => invalid.bluetooth_connected = Some(true),
                2 => invalid.factory_usb_count = None,
                _ => invalid.factory_usb_count = Some(2),
            }
            machine.observe(machine.ticket(), invalid);
            assert!(!machine.can_next());
            assert!(!machine.next(machine.ticket()));
        }
        machine.observe(machine.ticket(), factory_evidence(Mode::Wired));
        assert!(machine.next(machine.ticket()));
        let typing_ticket = machine.ticket();
        let mut unplugged = factory_evidence(Mode::Wired);
        unplugged.factory_usb_count = Some(0);
        machine.observe(typing_ticket, unplugged);
        machine.observe(machine.ticket(), factory_evidence(Mode::Wired));
        assert_eq!(machine.stage(), Stage::Setup(Mode::Wired));
        assert!(!machine.input(typing_ticket, TOKEN.into()));
        assert!(!machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
    }
    #[test]
    fn factory_shared_identity_requires_unique_count_and_owner_ack_without_role_guessing() {
        for mode in [Mode::Wired, Mode::Bluetooth, Mode::Dongle] {
            let valid = factory_evidence(mode);
            assert!(valid.left_usb.is_none());
            assert!(valid.dongle_usb.is_none());
            assert!(valid.ready(mode, Target::Factory));
            for count in [None, Some(2), Some(usize::from(mode == Mode::Bluetooth))] {
                let mut invalid = valid;
                invalid.factory_usb_count = count;
                assert!(!invalid.ready(mode, Target::Factory));
            }
        }
        let mut machine = Machine::factory();
        machine.installed(machine.ticket(), Ok(()));
        let mut ambiguous = factory_evidence(Mode::Wired);
        ambiguous.factory_usb_count = None;
        machine.observe(machine.ticket(), ambiguous);
        assert!(!machine.next(machine.ticket()));
        machine.observe(machine.ticket(), factory_evidence(Mode::Wired));
        assert_eq!(machine.stage(), Stage::Setup(Mode::Wired));
        assert!(!machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
        let acknowledged = machine.ticket();
        machine.input(acknowledged, TOKEN.into());
        let mut two = factory_evidence(Mode::Wired);
        two.factory_usb_count = Some(2);
        machine.observe(acknowledged, two);
        assert_eq!(machine.text(), "");
        assert!(!machine.next(machine.ticket()));
        machine.observe(machine.ticket(), factory_evidence(Mode::Wired));
        assert_eq!(machine.stage(), Stage::Setup(Mode::Wired));
        assert!(!machine.input(acknowledged, TOKEN.into()));
    }
    #[test]
    fn children_and_each_transport_require_explicit_completion() {
        let mut machine = Machine::new();
        assert!(!machine.paired(machine.ticket(), Ok(())));
        assert!(!machine.next(machine.ticket()));
        let old = machine.ticket();
        assert!(machine.installed(old, Ok(())));
        assert!(!machine.installed(old, Ok(())));
        assert!(machine.paired(machine.ticket(), Ok(())));
        for mode in [Mode::Wired, Mode::Bluetooth, Mode::Dongle] {
            assert_eq!(machine.stage(), Stage::Setup(mode));
            assert!(!machine.input(machine.ticket(), TOKEN.into()));
            assert!(!machine.next(machine.ticket()));
            assert!(machine.observe(machine.ticket(), evidence(mode)));
            assert_eq!(machine.stage(), Stage::Typing(mode));
            assert!(machine.input(machine.ticket(), "qwert hjkl h".into()));
            assert!(!machine.next(machine.ticket()));
            assert!(machine.input(machine.ticket(), TOKEN.into()));
            assert!(machine.can_next());
            assert!(machine.next(machine.ticket()));
            assert!(!machine.next(machine.ticket()));
        }
        assert_eq!(machine.stage(), Stage::Complete);
        assert!(!machine.cancel());
        assert!(!machine.observe(machine.ticket(), evidence(Mode::Dongle)));
    }
    #[test]
    fn changed_route_and_unknown_or_old_evidence_clear_confirmation() {
        let mut machine = checks();
        machine.observe(machine.ticket(), evidence(Mode::Wired));
        machine.input(machine.ticket(), TOKEN.into());
        let mut wrong = evidence(Mode::Wired);
        wrong.route = Connection::Bluetooth;
        machine.observe(machine.ticket(), wrong);
        assert_eq!(machine.text(), "");
        assert!(!machine.can_next());
        machine.observe(machine.ticket(), evidence(Mode::Wired));
        assert!(!machine.can_next());
        for change in 0..5 {
            let mut invalid = evidence(Mode::Wired);
            match change {
                0 => invalid.mode = None,
                1 => invalid.left_usb = None,
                2 => invalid.right_usb = Some(true),
                3 => invalid.right_link = None,
                _ => invalid.observed_at = Instant::now() - EVIDENCE_AGE - Duration::from_secs(1),
            }
            machine.observe(machine.ticket(), invalid);
            assert!(!machine.input(machine.ticket(), TOKEN.into()));
            assert!(!machine.next(machine.ticket()));
        }
    }
    #[test]
    fn bluetooth_uses_live_os_route_and_owner_typing_without_usb_telemetry() {
        let mut machine = checks();
        let wired = machine.ticket();
        machine.observe(wired, evidence(Mode::Wired));
        machine.input(wired, TOKEN.into());
        assert!(machine.next(wired));
        let mut bluetooth = evidence(Mode::Bluetooth);
        bluetooth.mode = None;
        bluetooth.right_link = None;
        for usb in 0..3 {
            let mut plugged = bluetooth;
            match usb {
                0 => plugged.left_usb = Some(true),
                1 => plugged.right_usb = Some(true),
                _ => plugged.dongle_usb = Some(true),
            }
            machine.observe(machine.ticket(), plugged);
            assert_eq!(machine.stage(), Stage::Setup(Mode::Bluetooth));
            assert!(!machine.input(machine.ticket(), TOKEN.into()));
            assert!(!machine.next(machine.ticket()));
        }
        let mut stale = bluetooth;
        stale.fresh = false;
        machine.observe(machine.ticket(), stale);
        assert!(!machine.input(machine.ticket(), TOKEN.into()));
        machine.observe(machine.ticket(), bluetooth);
        assert_eq!(machine.stage(), Stage::Typing(Mode::Bluetooth));
        assert!(!machine.next(machine.ticket()));
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
        assert_eq!(machine.stage(), Stage::Setup(Mode::Dongle));
    }
    #[test]
    fn route_changes_invalidate_old_typing_callbacks_but_stable_polls_do_not() {
        let mut machine = checks();
        machine.observe(machine.ticket(), evidence(Mode::Wired));
        let original = machine.ticket();
        machine.observe(original, evidence(Mode::Wired));
        assert_eq!(machine.ticket(), original);
        let mut lost = evidence(Mode::Wired);
        lost.route = Connection::Disconnected;
        machine.observe(original, lost);
        assert_ne!(machine.ticket(), original);
        let lost_ticket = machine.ticket();
        machine.observe(lost_ticket, evidence(Mode::Wired));
        assert_ne!(machine.ticket(), lost_ticket);
        assert!(!machine.input(original, TOKEN.into()));
        assert!(!machine.input(lost_ticket, TOKEN.into()));
        assert!(!machine.next(original));
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
    }
    #[test]
    fn prolonged_route_fluctuations_do_not_exhaust_the_ticket_epoch() {
        let mut machine = checks();
        machine.observe(machine.ticket(), evidence(Mode::Wired));
        let original = machine.ticket();
        for _ in 0..300 {
            let mut lost = evidence(Mode::Wired);
            lost.route = Connection::Disconnected;
            assert!(machine.observe(machine.ticket(), lost));
            assert!(machine.observe(machine.ticket(), evidence(Mode::Wired)));
        }
        assert!(machine.ticket().phase > 256);
        assert!(!machine.input(original, TOKEN.into()));
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
    }
    #[test]
    fn replacing_the_parent_rejects_callbacks_from_an_earlier_install() {
        let earlier = Machine::new();
        let mut current = Machine::new();
        assert_ne!(earlier.ticket(), current.ticket());
        assert!(!current.installed(earlier.ticket(), Ok(())));
        assert_eq!(current.stage(), Stage::Installing);
    }
    #[test]
    fn cancellation_failures_and_cross_phase_callbacks_are_terminal() {
        let mut machine = checks();
        let ticket = machine.ticket();
        assert!(machine.cancel());
        assert_eq!(machine.stage(), Stage::Cancelled);
        assert!(!machine.observe(ticket, evidence(Mode::Wired)));
        assert!(!machine.input(ticket, TOKEN.into()));
        assert!(!machine.paired(ticket, Ok(())));
        let mut failed = Machine::new();
        assert!(failed.installed(failed.ticket(), Err("copy failed".into())));
        assert!(matches!(failed.stage(), Stage::Failed(_)));
        assert!(!failed.installed(failed.ticket(), Ok(())));
    }
}
