//! Refuse Embassy initialization unless its UICR requests are already satisfied.
// No NVMC erase or write is performed by this gate.
fn compatible(reset: [u32; 2], nfc: u32, nfc_gpio: bool) -> bool {
    reset == [18, 18] && nfc & 1 == u32::from(!nfc_gpio)
}
#[cfg(target_arch = "arm")]
pub fn require_unchanged_uicr() {
    let uicr = embassy_nrf::pac::UICR;
    if !compatible(
        [uicr.pselreset(0).read().0, uicr.pselreset(1).read().0],
        uicr.nfcpins().read().0,
        cfg!(feature = "status-led"),
    ) {
        // Stock RMK writes only the boot request register and resets.
        rmk::boot::jump_to_bootloader();
        loop {
            cortex_m::asm::wfi();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::compatible;
    #[test]
    fn compatible_settings_need_no_uicr_change() {
        assert!(compatible([18, 18], 0xffff_ffff, false));
        assert!(compatible([18, 18], 0xffff_fffe, true));
    }
    #[test]
    fn either_reset_pin_mismatch_is_rejected() {
        for pins in [[18, !0], [!0, 18], [!0, !0], [19, 18]] {
            assert!(!compatible(pins, 1, false));
        }
    }
    #[test]
    fn nfc_mode_mismatch_is_rejected() {
        assert!(!compatible([18, 18], 1, true));
        assert!(!compatible([18, 18], 0, false));
    }
}
