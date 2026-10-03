//! Actual pinned-framework characterization, not hardware acceptance.
use embassy_time::Duration;
use embassy_usb::{
    Handler as UsbHandler,
    class::dfu::{
        app_mode::{DfuState, Handler},
        consts::DfuAttributes,
    },
    control::{OutResponse, Request},
    driver::Direction,
};
use std::{cell::Cell, rc::Rc};
#[path = "../../firmware/src/usb_rescue_scope.rs"]
mod actual_scope;

struct Recorder(Rc<Cell<usize>>);
impl Handler for Recorder {
    fn enter_dfu(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn state() -> (DfuState<Recorder>, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    (
        DfuState::new(
            Recorder(calls.clone()),
            DfuAttributes::WILL_DETACH,
            Duration::from_millis(1000),
        ),
        calls,
    )
}
fn detach() -> Request {
    Request::parse(&[0x21, 0, 0, 0, 0, 0, 0, 0])
}

#[test]
fn valid_detach_calls_handler_before_returning_acceptance() {
    let (mut dfu, calls) = state();
    assert_eq!(dfu.control_out(detach(), &[]), Some(OutResponse::Accepted));
    assert_eq!(calls.get(), 1); // ControlPipe.accept follows this callback.
}
#[test]
fn unrelated_request_type_and_recipient_do_not_trigger() {
    for bytes in [
        [0x41, 0, 0, 0, 0, 0, 0, 0],
        [0x20, 0, 0, 0, 0, 0, 0, 0],
        [0x21, 9, 0, 0, 0, 0, 0, 0],
    ] {
        let (mut dfu, calls) = state();
        assert_eq!(dfu.control_out(Request::parse(&bytes), &[]), None);
        assert_eq!(calls.get(), 0);
    }
}
#[test]
fn current_framework_also_accepts_wrong_interface_and_payload() {
    let (mut dfu, calls) = state();
    let mut request = detach();
    request.index = 65535;
    request.length = 1;
    assert_eq!(
        dfu.control_out(request, &[0xff]),
        Some(OutResponse::Accepted)
    );
    assert_eq!(calls.get(), 1); // A production wrapper must reject this.
}
#[test]
fn direct_handler_does_not_validate_direction() {
    let (mut dfu, calls) = state();
    let mut request = detach();
    request.direction = Direction::In;
    assert_eq!(dfu.control_out(request, &[]), Some(OutResponse::Accepted));
    assert_eq!(calls.get(), 1); // UsbDevice normally routes by direction first.
}

#[test]
fn actual_scope_passes_valid_request_to_framework() {
    let (mut dfu, calls) = state();
    let mut filter = actual_scope::RequestScope;
    let response = filter
        .control_out(detach(), &[])
        .or_else(|| dfu.control_out(detach(), &[]));
    assert_eq!(response, Some(OutResponse::Accepted));
    assert_eq!(calls.get(), 1);
}

#[test]
fn actual_scope_blocks_each_invalid_detach_before_framework() {
    for (index, length, payload) in [
        (1, 0, &[][..]),
        (65535, 0, &[][..]),
        (0, 1, &[][..]),
        (0, 0, &[255][..]),
        (0, 1, &[255][..]),
    ] {
        let (mut dfu, calls) = state();
        let mut filter = actual_scope::RequestScope;
        let mut request = detach();
        request.index = index;
        request.length = length;
        let response = filter
            .control_out(request, payload)
            .or_else(|| dfu.control_out(request, payload));
        assert_eq!(response, Some(OutResponse::Rejected));
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn actual_scope_rejects_wrong_interface_status_and_defers_standard_requests() {
    let mut filter = actual_scope::RequestScope;
    let mut request = detach();
    request.request = 3; // GETSTATUS class request, wrong interface.
    request.index = 1;
    assert_eq!(
        filter.control_out(request, &[]),
        Some(OutResponse::Rejected)
    );
    assert_eq!(
        filter.control_out(Request::parse(&[0x00, 5, 0, 0, 0, 0, 0, 0]), &[]),
        None
    );
}
