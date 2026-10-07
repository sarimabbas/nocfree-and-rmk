//! Install coordinates child completion and three observable typing checks.
//! Firmware retains its own machine; this parent performs no IO.
use crate::{device_status::Mode, scope::Scope, status_strip::Connection};
use statig::prelude::*;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) const TOKEN: &str = "qwert HJKL h";
const EVIDENCE_AGE: Duration = Duration::from_secs(5);

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
                "Connect the left USB cable and move its switch to middle WIRED. Keep the right half ON with USB unplugged. Unplug the dongle and disconnect the keyboard in Bluetooth settings."
            }
            Mode::Bluetooth => {
                "Unplug both USB cables and the dongle. Move the left switch to bottom Bluetooth and keep the right half ON. Connect NocFree in Bluetooth settings."
            }
            Mode::Dongle => {
                "Unplug both USB cables. Plug in the dongle and move the left switch to top DONGLE. Keep the right half ON. Disconnect the keyboard in Bluetooth settings."
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
    Checking(Mode),
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
    pub bluetooth_connected: Option<bool>,
    pub factory_usb_count: Option<usize>,
    pub usb_identity: u64,
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
            && self.left_usb == Some(mode == Mode::Wired)
            && self.dongle_usb == Some(mode == Mode::Dongle)
            && self.bluetooth_connected == Some(mode == Mode::Bluetooth)
    }
    fn same_setup(self, other: Self, target: Target) -> bool {
        (target == Target::Rmk
            || (self.mode == other.mode
                && self.route == other.route
                && self.factory_usb_count == other.factory_usb_count))
            && self.usb_identity == other.usb_identity
            && self.left_usb == other.left_usb
            && self.right_usb == other.right_usb
            && self.dongle_usb == other.dongle_usb
            && self.fresh == other.fresh
            && self.bluetooth_connected == other.bluetooth_connected
    }
}
impl Mode {
    pub(crate) fn install_instruction(self) -> &'static str {
        match self {
            Self::Wired => {
                "Connect LEFT by USB. Unplug RIGHT USB and the dongle. Turn off Bluetooth on this computer. Keep RIGHT ON. If keys do not type through USB, press LEFT Fn + Space once to select USB."
            }
            Self::Bluetooth => {
                "Unplug both USB cables and the dongle. Move LEFT to either top or bottom to turn it on; keep RIGHT ON. Press LEFT Fn + 1 to select Bluetooth profile 1. Turn on Bluetooth on this computer, then connect NocFree RMK in Bluetooth settings. If pairing fails, forget the old entry, hold LEFT Fn + 1 for five seconds, then pair again."
            }
            Self::Dongle => {
                "Unplug both USB cables and connect the dongle. Move LEFT to either top or bottom to turn it on; keep RIGHT ON. Turn off Bluetooth on this computer. Press LEFT Fn + Tab to select the dongle. If it does not connect, hold LEFT Fn + Tab for five seconds, then replug the dongle."
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
        CheckFactoryDongle,
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
                Event::CheckFactoryDongle if self.target == Target::Factory => {
                    *context = true;
                    self.advance();
                    Transition(State::checking(Mode::Dongle, None, String::new()))
                }
                Event::Installed(ticket, result) if self.current(*ticket) => {
                    *context = true;
                    match result {
                        Ok(()) => {
                            self.advance();
                            if self.scope != Scope::Whole {
                                return Transition(State::complete());
                            }
                            Transition(State::checking(Mode::Wired, None, String::new()))
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
            event: &Event,
            context: &mut bool,
        ) -> Outcome<State> {
            match event {
                Event::Observed(ticket, latest) if self.current(*ticket) => {
                    *context = true;
                    let changed = evidence.is_none_or(|old| {
                        !old.same_setup(*latest, self.target)
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
                            && old.same_setup(*latest, self.target)
                    });
                    Transition(State::checking(
                        *mode,
                        Some(*latest),
                        if keep { input.clone() } else { String::new() },
                    ))
                }
                Event::Input(ticket, text)
                    if self.current(*ticket)
                        && evidence.is_some_and(|e| e.ready(*mode, self.target)) =>
                {
                    *context = true;
                    Transition(State::checking(*mode, *evidence, text.clone()))
                }
                Event::Next(ticket)
                    if self.current(*ticket)
                        && evidence.is_some_and(|e| e.ready(*mode, self.target))
                        && input == TOKEN =>
                {
                    *context = true;
                    self.advance();
                    match mode {
                        Mode::Wired => {
                            Transition(State::checking(Mode::Bluetooth, None, String::new()))
                        }
                        Mode::Bluetooth => {
                            Transition(State::checking(Mode::Dongle, None, String::new()))
                        }
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
    pub fn rmk_check() -> Self {
        let mut machine = Self::new();
        machine.installed(machine.ticket(), Ok(()));
        machine
    }
    pub fn factory_dongle_check() -> Self {
        let mut machine = Self::scoped(Target::Factory, Scope::Whole);
        machine.dispatch(machine::Event::CheckFactoryDongle);
        machine
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
            machine::State::Checking { mode, .. } => Stage::Checking(*mode),
            machine::State::Complete {} => Stage::Complete,
            machine::State::Cancelled {} => Stage::Cancelled,
            machine::State::Failed { error } => Stage::Failed(error.clone()),
        }
    }
    pub fn typing_ready(&self) -> bool {
        matches!(self.machine.state(), machine::State::Checking { mode, evidence, .. }
            if evidence.is_some_and(|e| e.ready(*mode, self.target())))
    }
    pub fn can_next(&self) -> bool {
        self.typing_ready() && self.text() == TOKEN
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
            mode: None,
            route: Connection::Unknown,
            left_usb: Some(mode == Mode::Wired),
            right_usb: Some(false),
            dongle_usb: Some(mode == Mode::Dongle),
            bluetooth_connected: Some(mode == Mode::Bluetooth),
            factory_usb_count: None,
            usb_identity: 0,
            fresh: true,
            observed_at: Instant::now(),
        }
    }
    fn ready(machine: &mut Machine, mode: Mode) {
        assert!(machine.observe(machine.ticket(), evidence(mode)));
        assert_eq!(machine.stage(), Stage::Checking(mode));
        assert!(machine.typing_ready());
        assert!(!machine.can_next());
    }
    #[test]
    fn installation_and_each_check_need_explicit_completion() {
        let mut machine = Machine::new();
        assert!(!machine.next(machine.ticket()));
        assert!(machine.installed(machine.ticket(), Ok(())));
        for mode in [Mode::Wired, Mode::Bluetooth, Mode::Dongle] {
            ready(&mut machine, mode);
            for wrong in ["qwert hjkl h", "qwert HJKL h ", "qwert HJKL", ""] {
                assert!(machine.input(machine.ticket(), wrong.into()));
                assert!(!machine.can_next());
            }
            assert!(machine.input(machine.ticket(), TOKEN.into()));
            assert!(machine.can_next());
            assert_eq!(machine.stage(), Stage::Checking(mode));
            assert!(machine.next(machine.ticket()));
            assert!(!machine.next(machine.ticket()));
        }
        assert_eq!(machine.stage(), Stage::Complete);
    }
    #[test]
    fn every_route_requires_known_isolated_host_connections() {
        for mode in [Mode::Wired, Mode::Bluetooth, Mode::Dongle] {
            assert!(evidence(mode).ready(mode, Target::Rmk));
            for change in 0..10 {
                let mut invalid = evidence(mode);
                match change {
                    0 => invalid.left_usb = None,
                    1 => invalid.right_usb = None,
                    2 => invalid.dongle_usb = None,
                    3 => invalid.bluetooth_connected = None,
                    4 => invalid.left_usb = Some(mode != Mode::Wired),
                    5 => invalid.right_usb = Some(true),
                    6 => invalid.dongle_usb = Some(mode != Mode::Dongle),
                    7 => invalid.bluetooth_connected = Some(mode != Mode::Bluetooth),
                    8 => invalid.fresh = false,
                    _ => {
                        invalid.observed_at = Instant::now() - EVIDENCE_AGE - Duration::from_secs(1)
                    }
                }
                assert!(!invalid.ready(mode, Target::Rmk));
                let mut machine = Machine::rmk_check();
                machine.observe(machine.ticket(), invalid);
                assert!(!machine.can_next());
                assert!(!machine.next(machine.ticket()));
            }
        }
    }
    #[test]
    fn private_telemetry_does_not_block_or_reset_native_checks() {
        let mut machine = Machine::rmk_check();
        ready(&mut machine, Mode::Wired);
        let ticket = machine.ticket();
        machine.input(ticket, TOKEN.into());
        let mut changed = evidence(Mode::Wired);
        changed.mode = Some(Mode::Dongle);
        changed.route = Connection::Disconnected;
        changed.factory_usb_count = Some(9);
        assert!(machine.observe(ticket, changed));
        assert_eq!(machine.ticket(), ticket);
        assert_eq!(machine.text(), TOKEN);
        assert!(machine.next(ticket));
    }
    #[test]
    fn first_connection_observation_recreates_the_disabled_typing_field() {
        let mut machine = Machine::rmk_check();
        let waiting = machine.ticket();
        assert!(!machine.typing_ready());
        assert!(machine.observe(waiting, evidence(Mode::Wired)));
        assert!(machine.typing_ready());
        assert_ne!(machine.ticket(), waiting);
        assert!(!machine.input(waiting, TOKEN.into()));
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.can_next());
    }
    #[test]
    fn lost_then_restored_setup_requires_new_typing() {
        let mut machine = Machine::rmk_check();
        ready(&mut machine, Mode::Wired);
        let previous = machine.ticket();
        machine.input(previous, TOKEN.into());
        let mut lost = evidence(Mode::Wired);
        lost.left_usb = Some(false);
        machine.observe(previous, lost);
        assert_eq!(machine.text(), "");
        assert!(!machine.can_next());
        machine.observe(machine.ticket(), evidence(Mode::Wired));
        assert_eq!(machine.stage(), Stage::Checking(Mode::Wired));
        assert!(!machine.input(previous, TOKEN.into()));
        assert!(!machine.next(previous));
        assert!(!machine.next(machine.ticket()));
        assert!(!machine.can_next());
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.can_next());
    }
    #[test]
    fn usb_replacement_clears_typing_even_with_same_presence() {
        let mut machine = Machine::rmk_check();
        ready(&mut machine, Mode::Wired);
        let old = machine.ticket();
        machine.input(old, TOKEN.into());
        let mut replacement = evidence(Mode::Wired);
        replacement.usb_identity = 1;
        machine.observe(old, replacement);
        assert_eq!(machine.stage(), Stage::Checking(Mode::Wired));
        assert_eq!(machine.text(), "");
        assert!(!machine.next(old));
        assert!(!machine.input(old, TOKEN.into()));
    }

    #[test]
    fn factory_replacement_also_requires_new_typing() {
        let mut machine = Machine::factory_dongle_check();
        let mut ready = evidence(Mode::Dongle);
        ready.factory_usb_count = Some(1);
        machine.observe(machine.ticket(), ready);
        assert!(!machine.next(machine.ticket()));
        let ticket = machine.ticket();
        machine.input(ticket, TOKEN.into());
        assert!(machine.can_next());
        ready.usb_identity = 2;
        machine.observe(ticket, ready);
        assert_eq!(machine.stage(), Stage::Checking(Mode::Dongle));
        assert_eq!(machine.text(), "");
        assert!(!machine.next(ticket));
    }
    #[test]
    fn stale_observation_and_recreated_machine_reject_old_callbacks() {
        let mut machine = Machine::rmk_check();
        ready(&mut machine, Mode::Wired);
        let previous = machine.ticket();
        let mut stale = evidence(Mode::Wired);
        stale.observed_at = Instant::now() - EVIDENCE_AGE - Duration::from_secs(1);
        machine.observe(previous, stale);
        assert!(!machine.input(previous, TOKEN.into()));
        assert!(!machine.next(machine.ticket()));
        let mut replacement = Machine::rmk_check();
        assert!(!replacement.observe(previous, evidence(Mode::Wired)));
        assert!(!replacement.input(previous, TOKEN.into()));
    }
    #[test]
    fn cancellation_and_install_failure_are_terminal() {
        let mut machine = Machine::rmk_check();
        let ticket = machine.ticket();
        assert!(machine.cancel());
        assert_eq!(machine.stage(), Stage::Cancelled);
        assert!(!machine.observe(ticket, evidence(Mode::Wired)));
        assert!(!machine.next(ticket));
        let mut failed = Machine::new();
        assert!(failed.installed(failed.ticket(), Err("verification failed".into())));
        assert!(matches!(failed.stage(), Stage::Failed(_)));
        assert!(!failed.installed(failed.ticket(), Ok(())));
    }
    #[test]
    fn single_part_install_does_not_require_whole_keyboard_checks() {
        for target in [Target::Rmk, Target::Factory] {
            for role in Scope::Whole.roles() {
                let mut machine = Machine::scoped(target, Scope::Part(role));
                assert!(machine.installed(machine.ticket(), Ok(())));
                assert_eq!(machine.stage(), Stage::Complete);
            }
        }
    }
    #[test]
    fn factory_shared_identity_requires_count_before_typing() {
        let mut machine = Machine::factory();
        machine.installed(machine.ticket(), Ok(()));
        for count in [None, Some(0), Some(2)] {
            let mut invalid = evidence(Mode::Wired);
            invalid.factory_usb_count = count;
            machine.observe(machine.ticket(), invalid);
            assert!(!machine.can_next());
        }
        let mut valid = evidence(Mode::Wired);
        valid.left_usb = None;
        valid.dongle_usb = None;
        valid.factory_usb_count = Some(1);
        machine.observe(machine.ticket(), valid);
        assert!(!machine.can_next());
        assert!(machine.typing_ready());
        assert!(machine.input(machine.ticket(), TOKEN.into()));
        assert!(machine.next(machine.ticket()));
        assert_eq!(machine.stage(), Stage::Checking(Mode::Bluetooth));
        let dongle = Machine::factory_dongle_check();
        assert_eq!(dongle.stage(), Stage::Checking(Mode::Dongle));
    }
}
