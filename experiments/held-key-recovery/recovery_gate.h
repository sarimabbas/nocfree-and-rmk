#ifndef NOCFREE_RECOVERY_GATE_H
#define NOCFREE_RECOVERY_GATE_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

enum recovery_role { RECOVERY_LEFT, RECOVERY_RIGHT };
/* IO_ERROR is not a DFU request: caller continues existing app-validity flow.
 * Invalid apps retain upstream recovery; a bus fault must not manufacture DFU. */
enum recovery_result { START_APPLICATION, ENTER_USB_RECOVERY, RECOVERY_IO_ERROR };
/* Adapter contract: all callbacks bounded; monotonic uint32_t milliseconds;
 * write/read return false on timeout, NACK, bus fault or short transfer.
 * USB means VBUS presence, NOT USB enumeration. No writes to flash or RAM markers.
 * read receives register 0 and returns both input ports, little endian.
 * The caller must disable/restore its I2C peripheral before entering the app.
 */
struct recovery_io {
    void *context;
    uint32_t (*now_ms)(void *);
    bool (*usb_present)(void *);
    bool (*write)(void *, uint8_t address, const uint8_t *, size_t, uint32_t timeout_ms);
    bool (*read)(void *, uint8_t address, uint8_t reg, uint8_t *, size_t, uint32_t timeout_ms);
    void (*wait_ms)(void *, uint32_t);
};
enum recovery_result recovery_gate(const struct recovery_io *, enum recovery_role);
#endif
