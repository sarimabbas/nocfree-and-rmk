#include "recovery_gate.h"

/* Experiment policy: 60 ms stable chord; 200 ms nominal deadline, <=4 ms per transfer.
 * Bounded sample count also prevents a broken clock from looping forever.
 */
enum { GATE_MS = 200, STABLE_MS = 60, TRANSFER_MS = 4, SAMPLE_MS = 10, MAX_SAMPLES = 21 };
static const uint8_t addresses[] = {0x20, 0x22, 0x24};
static bool within(const struct recovery_io *io, uint32_t start) {
    return (uint32_t)(io->now_ms(io->context) - start) < GATE_MS;
}
static bool usable(const struct recovery_io *io, uint32_t start) {
    return within(io, start) && io->usb_present(io->context);
}
enum recovery_result recovery_gate(const struct recovery_io *io, enum recovery_role role) {
    if (!io || !io->now_ms || !io->usb_present || !io->write || !io->read || !io->wait_ms ||
        (role != RECOVERY_LEFT && role != RECOVERY_RIGHT)) return RECOVERY_IO_ERROR;
    const uint32_t start = io->now_ms(io->context);
    if (!io->usb_present(io->context)) return START_APPLICATION;
    const uint64_t chord = role == RECOVERY_LEFT ? (UINT64_C(1) << 40) | 1 :
                                                  (UINT64_C(1) << 42) | (UINT64_C(1) << 14);
    const uint8_t inputs[] = {6, 255, 255};
    const uint8_t polarity[] = {4, 0, 0};
    for (unsigned i = 0; i < 3; ++i) {
        if (!usable(io, start)) return START_APPLICATION;
        if (!io->write(io->context, addresses[i], inputs, sizeof inputs, TRANSFER_MS)) return RECOVERY_IO_ERROR;
        if (!usable(io, start)) return START_APPLICATION;
        if (!io->write(io->context, addresses[i], polarity, sizeof polarity, TRANSFER_MS)) return RECOVERY_IO_ERROR;
    }
    uint32_t held_since = 0;
    for (unsigned sample = 0; sample < MAX_SAMPLES; ++sample) {
        uint64_t pressed = 0;
        for (unsigned i = 0; i < 3; ++i) {
            if (!usable(io, start)) return START_APPLICATION;
            uint8_t ports[2];
            if (!io->read(io->context, addresses[i], 0, ports, sizeof ports, TRANSFER_MS)) return RECOVERY_IO_ERROR;
            pressed |= (uint64_t)(uint16_t)~((uint16_t)ports[0] | ((uint16_t)ports[1] << 8)) << (i * 16);
        }
        if (!usable(io, start)) return START_APPLICATION;
        /* Only an already-held chord counts; release or a missing first chord
         * starts the application immediately, never a later accidental press. */
        if ((pressed & chord) != chord) return START_APPLICATION;
        uint32_t now = io->now_ms(io->context);
        if (sample == 0) held_since = now;
        if ((uint32_t)(now - held_since) >= STABLE_MS) return ENTER_USB_RECOVERY;
        io->wait_ms(io->context, SAMPLE_MS);
    }
    return START_APPLICATION;
}
