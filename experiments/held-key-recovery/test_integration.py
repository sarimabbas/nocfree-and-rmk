#!/usr/bin/env python3
"""Host-test the actual patched upstream DFU decision function; no device access."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

STUBS = r'''
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include "recovery_gate.h"
#define DFU_MAGIC_OTA_APPJUM 0xb1
#define DFU_MAGIC_OTA_RESET 0xa8
#define DFU_MAGIC_SERIAL_ONLY_RESET 0x4e
#define DFU_MAGIC_UF2_RESET 0x57
#define DFU_MAGIC_SKIP 0x6d
#define DFU_DBL_RESET_MAGIC 0x5a1ad5
#define DFU_DBL_RESET_APP 0x4ee5677e
#define DFU_DBL_RESET_DELAY 500
#define POWER_RESETREAS_RESETPIN_Msk 1
#define BUTTON_DFU 0
#define BUTTON_FRESET 1
#define STATE_BLE_DISCONNECTED 0
#define STATE_USB_UNMOUNTED 1
static struct { uint32_t GPREGRET, RESETREAS; } power;
#define NRF_POWER (&power)
static bool _sd_inited, _ota_dfu, valid_app, marker, buttons[2];
static uint32_t retained;
static uint32_t *dbl_reset_mem = &retained;
static enum recovery_result gate_result;
static unsigned gate_calls, usb_calls, ble_calls, dfu_calls, delays, mbr_calls;
static unsigned usb_teardowns, sd_disables;
static bool serial_mode, dfu_ota, dfu_restart;
static uint32_t dfu_timeout;
static enum recovery_result recovery_nrf_startup(enum recovery_role role) {
    assert(role == NOCFREE_RECOVERY_ROLE);
    ++gate_calls;
    return gate_result;
}
static bool button_pressed(unsigned button) { return buttons[button]; }
static bool bootloader_app_is_valid(void) { return valid_app; }
#define APP_ASKS_FOR_SINGLE_TAP_RESET() (marker)
static void delay_ms(unsigned milliseconds) {
    assert(milliseconds == 500);
    ++delays;
}
#define NRFX_DELAY_MS(ms) delay_ms(ms)
static void led_state(unsigned state) { assert(state <= 1); }
static void mbr_init_sd(void) { ++mbr_calls; }
static void ble_stack_init(void) { ++ble_calls; }
static void usb_init(bool serial) { ++usb_calls; serial_mode = serial; }
static void bootloader_dfu_start(bool ota, uint32_t timeout, bool restart) {
    ++dfu_calls; dfu_ota = ota; dfu_timeout = timeout; dfu_restart = restart;
}
static void disable_softdevice(void) { ++sd_disables; }
static void usb_teardown(void) { ++usb_teardowns; }
'''

TESTS = r'''
static void reset(void) {
    power.GPREGRET = power.RESETREAS = retained = 0;
    _sd_inited = _ota_dfu = marker = buttons[0] = buttons[1] = false;
    valid_app = true;
    gate_result = START_APPLICATION;
    gate_calls = usb_calls = ble_calls = dfu_calls = delays = mbr_calls = 0;
    usb_teardowns = sd_disables = 0;
    serial_mode = dfu_ota = dfu_restart = false;
    dfu_timeout = UINT32_MAX;
}
static void usb_dfu(uint32_t timeout, bool restart, bool serial) {
    assert(dfu_calls == 1 && usb_calls == 1 && ble_calls == 0);
    assert(!dfu_ota && dfu_timeout == timeout && dfu_restart == restart);
    assert(serial_mode == serial && usb_teardowns == 1 && sd_disables == 0);
}
int main(void) {
    reset(); check_dfu_mode();
    assert(gate_calls == 1 && dfu_calls == 0);

    /* Chord wins over every legacy software entry except SD-active APPJUM. */
    const uint32_t requests[] = {0, DFU_MAGIC_SKIP, DFU_MAGIC_OTA_RESET,
                                DFU_MAGIC_SERIAL_ONLY_RESET, DFU_MAGIC_UF2_RESET};
    for (unsigned i = 0; i < sizeof requests / sizeof requests[0]; ++i) {
        reset(); power.GPREGRET = requests[i]; gate_result = ENTER_USB_RECOVERY;
        marker = true; check_dfu_mode(); usb_dfu(0, false, false);
        assert(gate_calls == 1 && power.GPREGRET == 0 && mbr_calls == 0);
    }
    reset(); buttons[0] = buttons[1] = true;
    gate_result = ENTER_USB_RECOVERY; check_dfu_mode(); usb_dfu(0, false, false);

    reset(); power.GPREGRET = DFU_MAGIC_OTA_APPJUM;
    gate_result = ENTER_USB_RECOVERY; check_dfu_mode();
    assert(gate_calls == 0 && dfu_calls == 1 && ble_calls == 1 && usb_calls == 0);
    assert(dfu_ota && dfu_timeout == 0 && !dfu_restart && mbr_calls == 0);
    assert(sd_disables == 1 && usb_teardowns == 0);

    reset(); gate_result = RECOVERY_IO_ERROR; check_dfu_mode();
    assert(gate_calls == 1 && dfu_calls == 0);
    reset(); valid_app = false; gate_result = RECOVERY_IO_ERROR;
    check_dfu_mode(); usb_dfu(0, false, false);

    reset(); marker = true; check_dfu_mode(); usb_dfu(3000, true, false);
    assert(retained == DFU_DBL_RESET_APP);
    reset(); marker = true; retained = DFU_DBL_RESET_APP; check_dfu_mode();
    assert(dfu_calls == 0 && retained == DFU_DBL_RESET_APP);

    reset(); power.GPREGRET = DFU_MAGIC_SERIAL_ONLY_RESET;
    check_dfu_mode(); usb_dfu(3000, true, true);
    reset(); power.GPREGRET = DFU_MAGIC_UF2_RESET;
    check_dfu_mode(); usb_dfu(3000, true, false);

    reset(); power.GPREGRET = DFU_MAGIC_SKIP; marker = true;
    check_dfu_mode(); assert(dfu_calls == 0 && power.GPREGRET == 0);
    reset(); power.GPREGRET = DFU_MAGIC_OTA_RESET; check_dfu_mode();
    assert(gate_calls == 1 && dfu_calls == 1 && ble_calls == 1 && dfu_ota);
    assert(mbr_calls == 1 && sd_disables == 1);

    reset(); retained = DFU_DBL_RESET_MAGIC; power.RESETREAS = 1;
    check_dfu_mode(); usb_dfu(0, false, false); assert(delays == 0);
    reset(); power.RESETREAS = 1; check_dfu_mode();
    assert(dfu_calls == 0 && delays == 1 && retained == 0);
    reset(); retained = DFU_DBL_RESET_MAGIC; check_dfu_mode();
    assert(dfu_calls == 0 && delays == 0);

    reset(); buttons[0] = buttons[1] = true; check_dfu_mode();
    assert(dfu_calls == 1 && ble_calls == 1 && dfu_ota && mbr_calls == 1);
    puts("Actual patched check_dfu_mode integration cases passed");
}
'''


def extract(source):
    start = source.index("static void check_dfu_mode(void) {")
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                function = source[start:index + 1]
                if "recovery_nrf_startup(NOCFREE_RECOVERY_ROLE)" not in function:
                    raise ValueError("main.c does not contain the startup adapter integration")
                return function
    raise ValueError("unterminated check_dfu_mode function")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--main", "-m", type=Path,
                        default=ROOT / ".evidence/bootloader-work/smoke-left/src/main.c",
                        help="patched pinned upstream main.c (default: left smoke checkout)")
    parser.add_argument("--compiler", "-c", default="clang",
                        help="host C compiler executable (default: clang)")
    args = parser.parse_args()
    compiler = shutil.which(args.compiler)
    if not compiler:
        parser.error(f"compiler unavailable: {args.compiler}")
    function = extract(args.main.read_text())
    with tempfile.TemporaryDirectory(prefix="nocfree-startup-integration-") as directory:
        path = Path(directory)
        source = path / "integration.c"
        source.write_text(STUBS + function + TESTS)
        for role in ("LEFT", "RIGHT"):
            binary = path / role.lower()
            subprocess.run([compiler, "-std=c11", "-Wall", "-Wextra", "-Werror",
                            "-fsanitize=address,undefined", "-fno-omit-frame-pointer",
                            f"-DNOCFREE_RECOVERY_ROLE=RECOVERY_{role}",
                            "-I", str(HERE), str(source), "-o", str(binary)], check=True)
            subprocess.run([str(binary)], check=True)
    print("Both role integrations passed; host evidence only, no device operations.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Integration test failed: {error}") from None
