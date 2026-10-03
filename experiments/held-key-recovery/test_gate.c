#include "recovery_gate.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>
struct fake {
    uint32_t now, transfer_cost;
    uint64_t pressed;
    unsigned writes, reads, fail_at, calls;
    uint32_t release_at, disconnect_at, press_at;
    bool frozen_clock;
};
static uint32_t now(void *v) { return ((struct fake *)v)->now; }
static bool usb(void *v) {
    struct fake *f = v; return f->now < f->disconnect_at;
}
static bool transfer(struct fake *f, uint32_t timeout) {
    assert(timeout == 4);
    f->calls++;
    if (!f->frozen_clock) f->now += f->transfer_cost > timeout ? timeout : f->transfer_cost;
    return f->calls != f->fail_at && f->transfer_cost <= timeout;
}
static bool write_bus(void *v, uint8_t a, const uint8_t *b, size_t n, uint32_t timeout) {
    struct fake *f = v; assert(n == 3); assert(a == 0x20 + (f->writes / 2) * 2);
    if (f->writes % 2 == 0) { assert(b[0] == 6 && b[1] == 255 && b[2] == 255); }
    else { assert(b[0] == 4 && b[1] == 0 && b[2] == 0); }
    f->writes++; return transfer(f, timeout);
}
static bool read_bus(void *v, uint8_t a, uint8_t reg, uint8_t *b, size_t n, uint32_t timeout) {
    struct fake *f = v; assert(reg == 0 && n == 2);
    unsigned index = f->reads++ % 3; assert(a == 0x20 + index * 2);
    if (!transfer(f, timeout)) return false;
    uint64_t pressed = f->now >= f->release_at || f->now < f->press_at ? 0 : f->pressed;
    uint16_t ports = (uint16_t)~(pressed >> (index * 16));
    b[0] = (uint8_t)ports; b[1] = (uint8_t)(ports >> 8); return true;
}
static void wait_ms(void *v, uint32_t ms) {
    struct fake *f = v; assert(ms == 10); if (!f->frozen_clock) f->now += ms;
}
static struct fake fixture(uint64_t chord) {
    return (struct fake){.pressed = chord, .release_at = UINT32_MAX, .disconnect_at = UINT32_MAX};
}
static enum recovery_result run(struct fake *f, enum recovery_role r) {
    struct recovery_io io = {f, now, usb, write_bus, read_bus, wait_ms};
    return recovery_gate(&io, r);
}
int main(void) {
    const uint64_t left = (UINT64_C(1) << 40) | 1;
    const uint64_t right = (UINT64_C(1) << 42) | (UINT64_C(1) << 14);
    struct fake f = fixture(left);
    assert(run(&f, RECOVERY_LEFT) == ENTER_USB_RECOVERY && f.now == 60 && f.writes == 6);
    f = fixture(right); assert(run(&f, RECOVERY_RIGHT) == ENTER_USB_RECOVERY);
    f = fixture(0); assert(run(&f, RECOVERY_LEFT) == START_APPLICATION && f.reads == 3 && f.now == 0);
    f = fixture(left); assert(run(&f, RECOVERY_RIGHT) == START_APPLICATION);
    f = fixture(right); f.press_at = 20;
    assert(run(&f, RECOVERY_RIGHT) == START_APPLICATION && f.now == 0);
    f = fixture(UINT64_C(1) << 42); assert(run(&f, RECOVERY_RIGHT) == START_APPLICATION);
    f = fixture(UINT64_C(1) << 14); assert(run(&f, RECOVERY_RIGHT) == START_APPLICATION);
    f = fixture(left); f.transfer_cost = 4; f.disconnect_at = 5;
    assert(run(&f, RECOVERY_LEFT) == START_APPLICATION && f.writes == 2);
    f = fixture(left); f.now = UINT32_MAX - 25;
    assert(run(&f, RECOVERY_LEFT) == ENTER_USB_RECOVERY && f.now == 34);
    f = fixture(UINT64_C(1) << 40); assert(run(&f, RECOVERY_LEFT) == START_APPLICATION);
    f = fixture(left); f.disconnect_at = 0;
    assert(run(&f, RECOVERY_LEFT) == START_APPLICATION && f.calls == 0);
    f = fixture(left); f.release_at = 30; assert(run(&f, RECOVERY_LEFT) == START_APPLICATION && f.now == 30);
    f = fixture(left); f.disconnect_at = 30; assert(run(&f, RECOVERY_LEFT) == START_APPLICATION && f.now == 30);
    /* Every configuration or snapshot transfer can fail, with no partial recovery. */
    for (unsigned fault = 1; fault <= 27; ++fault) {
        f = fixture(left); f.fail_at = fault;
        assert(run(&f, RECOVERY_LEFT) == RECOVERY_IO_ERROR);
    }
    f = fixture(left); f.transfer_cost = 5;
    assert(run(&f, RECOVERY_LEFT) == RECOVERY_IO_ERROR && f.now == 4);
    f = fixture(left); f.transfer_cost = 4;
    assert(run(&f, RECOVERY_LEFT) == ENTER_USB_RECOVERY && f.now < 200);
    f = fixture(left); f.frozen_clock = true;
    assert(run(&f, RECOVERY_LEFT) == START_APPLICATION && f.reads == 63);
    f = fixture(UINT64_MAX); assert(run(&f, RECOVERY_LEFT) == ENTER_USB_RECOVERY);
    assert(recovery_gate(NULL, RECOVERY_LEFT) == RECOVERY_IO_ERROR);
    f = fixture(left); assert(run(&f, (enum recovery_role)99) == RECOVERY_IO_ERROR && f.calls == 0);
    puts("recovery gate: held chords, ordinary startup, release, USB, 27 I2C faults, timeout, frozen clock passed");
}
