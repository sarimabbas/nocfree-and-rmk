//! Restrict the existing DFU protocol to its sole rescue interface and no payload.
use embassy_usb::control::{OutResponse, Recipient, Request, RequestType};

pub struct RequestScope;
impl embassy_usb::Handler for RequestScope {
    fn control_out(&mut self, request: Request, data: &[u8]) -> Option<OutResponse> {
        if request.request_type == RequestType::Class
            && request.recipient == Recipient::Interface
            && (request.index != 0
                || (request.request == 0 && (request.length != 0 || !data.is_empty())))
        {
            return Some(OutResponse::Rejected);
        }
        None
    }
}
