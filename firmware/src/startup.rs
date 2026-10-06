//! Refuse Embassy initialization if it would change UICR or leave LED pins in NFC mode.
// No NVMC erase or write is performed by this gate.
fn compatible(reset: [u32; 2], nfc: u32, nfc_gpio: bool) -> bool {
    // Embassy requests NFCPINS=1 without nfc-pins-as-gpio. If the saved bit is
    // zero, its masked writer returns Failed without enabling NVMC: flash
    // cannot change a zero bit to one. Unused NFC pins can therefore stay GPIO.
    // LEFT's LED does require GPIO; reject NFC=1 before Embassy could write it.
    reset == [18, 18] && (!nfc_gpio || nfc & 1 == 0)
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
    fn unused_nfc_pins_may_remain_gpio() {
        assert!(compatible([18, 18], 0xffff_fffe, false));
        assert!(compatible([18, 18], 0, false));
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
    }
}
