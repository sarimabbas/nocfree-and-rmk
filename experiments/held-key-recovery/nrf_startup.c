#include "nrf_startup.h"
#include "nrf.h"

#if !defined(NRF52833_XXAA)
#error "This adapter is only for the NocFree nRF52833"
#endif

enum { SDA = 11, SCL = 9, SCL_PSEL = 32 + SCL,
       POLL_LIMIT = 200000, STOP_US = 1000 };
struct startup { bool active; };

static uint32_t ticks(void) {
    NRF_TIMER1->TASKS_CAPTURE[0] = 1;
    return NRF_TIMER1->CC[0];
}
static uint32_t now_ms(void *context) {
    (void)context;
    return ticks() / 1000;
}
static bool usb_present(void *context) {
    (void)context;
    return (NRF_POWER->USBREGSTATUS & POWER_USBREGSTATUS_VBUSDETECT_Msk) != 0;
}
/* Timer deadline is primary; the iteration cap also terminates if its clock
 * fails. Every poll checks NACK/overrun. No IRQ or DMA is used. */
static bool event(volatile uint32_t *value, uint32_t start, uint32_t limit_us) {
    for (unsigned i = 0; i < POLL_LIMIT; ++i) {
        if (NRF_TWI0->EVENTS_ERROR) return false;
        if ((uint32_t)(ticks() - start) >= limit_us) return false;
        if (*value) { *value = 0; return true; }
    }
    return false;
}
static void clear_events(void) {
    NRF_TWI0->EVENTS_STOPPED = 0;
    NRF_TWI0->EVENTS_RXDREADY = 0;
    NRF_TWI0->EVENTS_TXDSENT = 0;
    NRF_TWI0->EVENTS_ERROR = 0;
    NRF_TWI0->EVENTS_BB = 0;
    NRF_TWI0->EVENTS_SUSPENDED = 0;
    NRF_TWI0->ERRORSRC = NRF_TWI0->ERRORSRC;
}
static bool stop(uint32_t start, uint32_t timeout_us) {
    NRF_TWI0->SHORTS = 0;
    NRF_TWI0->TASKS_STOP = 1;
    NRF_TWI0->TASKS_RESUME = 1;
    return event(&NRF_TWI0->EVENTS_STOPPED, start, timeout_us);
}
static bool transmit(uint8_t address, const uint8_t *data, size_t size,
                     uint32_t start, uint32_t timeout_us) {
    if (!size || !data || size > 3) return false;
    clear_events();
    NRF_TWI0->SHORTS = 0;
    NRF_TWI0->ADDRESS = address;
    NRF_TWI0->TXD = data[0];
    NRF_TWI0->TASKS_STARTTX = 1;
    for (size_t i = 0; i < size; ++i) {
        if (!event(&NRF_TWI0->EVENTS_TXDSENT, start, timeout_us)) return false;
        if (i + 1 < size) NRF_TWI0->TXD = data[i + 1];
    }
    return stop(start, timeout_us);
}
static bool write_bytes(void *context, uint8_t address, const uint8_t *data,
                        size_t size, uint32_t timeout_ms) {
    (void)context;
    if (!timeout_ms || timeout_ms > 4) return false;
    return transmit(address, data, size, ticks(), timeout_ms * 1000);
}
static bool read_bytes(void *context, uint8_t address, uint8_t reg,
                       uint8_t *data, size_t size, uint32_t timeout_ms) {
    (void)context;
    if (!data || size != 2 || !timeout_ms || timeout_ms > 4) return false;
    const uint32_t start = ticks(), timeout_us = timeout_ms * 1000;
    /* PCA9555 retains its register pointer across STOP. The pointer write and
     * both input bytes share one timeout, rather than three separate budgets. */
    if (!transmit(address, &reg, 1, start, timeout_us)) return false;
    clear_events();
    NRF_TWI0->SHORTS = TWI_SHORTS_BB_SUSPEND_Msk;
    NRF_TWI0->TASKS_STARTRX = 1;
    if (!event(&NRF_TWI0->EVENTS_RXDREADY, start, timeout_us)) return false;
    data[0] = (uint8_t)NRF_TWI0->RXD;
    if (!event(&NRF_TWI0->EVENTS_SUSPENDED, start, timeout_us)) return false;
    NRF_TWI0->SHORTS = TWI_SHORTS_BB_STOP_Msk;
    NRF_TWI0->TASKS_RESUME = 1;
    if (!event(&NRF_TWI0->EVENTS_RXDREADY, start, timeout_us)) return false;
    data[1] = (uint8_t)NRF_TWI0->RXD;
    return event(&NRF_TWI0->EVENTS_STOPPED, start, timeout_us);
}
static void wait_ms(void *context, uint32_t milliseconds) {
    (void)context;
    const uint32_t start = ticks();
    if (milliseconds > 10) milliseconds = 10;
    for (unsigned i = 0; i < POLL_LIMIT; ++i)
        if ((uint32_t)(ticks() - start) >= milliseconds * 1000) break;
}
static void cleanup(struct startup *state) {
    if (!state->active) return;
    /* Errors cannot prevent shutdown: clear ERROR before bounded STOP, then
     * disable regardless of whether a stuck bus acknowledged STOP. */
    NRF_TWI0->EVENTS_ERROR = 0;
    (void)stop(ticks(), STOP_US);
    NRF_TWI0->ENABLE = TWI_ENABLE_ENABLE_Disabled;
    NRF_TWI0->SHORTS = 0;
    NRF_TWI0->INTENCLR = UINT32_MAX;
    NVIC_ClearPendingIRQ(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0_IRQn);
    clear_events();
    NRF_TWI0->PSELSDA = UINT32_MAX;
    NRF_TWI0->PSELSCL = UINT32_MAX;
    NRF_P0->PIN_CNF[SDA] = GPIO_PIN_CNF_INPUT_Disconnect << GPIO_PIN_CNF_INPUT_Pos;
    NRF_P1->PIN_CNF[SCL] = GPIO_PIN_CNF_INPUT_Disconnect << GPIO_PIN_CNF_INPUT_Pos;
    NRF_TIMER1->TASKS_STOP = 1;
    NRF_TIMER1->TASKS_CLEAR = 1;
    NRF_TIMER1->SHORTS = 0;
    NRF_TIMER1->INTENCLR = UINT32_MAX;
    NVIC_ClearPendingIRQ(TIMER1_IRQn);
    for (unsigned i = 0; i < 4; ++i) NRF_TIMER1->EVENTS_COMPARE[i] = 0;
    state->active = false;
}
enum recovery_result recovery_nrf_startup(enum recovery_role role) {
    if (role != RECOVERY_LEFT && role != RECOVERY_RIGHT) return RECOVERY_IO_ERROR;
    if (!usb_present(0)) return START_APPLICATION;
    /* Sharing aliases must be disabled at the caller's pre-DFU seam. Never
     * silently interrupt an already-active peripheral. */
    if (NRF_TWI0->ENABLE != 0) return RECOVERY_IO_ERROR;
    struct startup state = { .active = true };
    NVIC_DisableIRQ(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0_IRQn);
    NVIC_ClearPendingIRQ(SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0_IRQn);
    NVIC_DisableIRQ(TIMER1_IRQn);
    NVIC_ClearPendingIRQ(TIMER1_IRQn);
    NRF_TIMER1->TASKS_STOP = 1;
    NRF_TIMER1->INTENCLR = UINT32_MAX;
    NRF_TIMER1->SHORTS = 0;
    NRF_TIMER1->MODE = TIMER_MODE_MODE_Timer;
    NRF_TIMER1->BITMODE = TIMER_BITMODE_BITMODE_32Bit;
    NRF_TIMER1->PRESCALER = 4; /* 16 MHz / 16 = 1 MHz. */
    NRF_TIMER1->TASKS_CLEAR = 1;
    NRF_TIMER1->TASKS_START = 1;
    const uint32_t pin_config = (GPIO_PIN_CNF_PULL_Pullup << GPIO_PIN_CNF_PULL_Pos) |
                               (GPIO_PIN_CNF_DRIVE_S0D1 << GPIO_PIN_CNF_DRIVE_Pos);
    NRF_P0->PIN_CNF[SDA] = pin_config;
    NRF_P1->PIN_CNF[SCL] = pin_config;
    NRF_TWI0->PSELSDA = SDA;
    NRF_TWI0->PSELSCL = SCL_PSEL;
    NRF_TWI0->FREQUENCY = TWI_FREQUENCY_FREQUENCY_K100;
    NRF_TWI0->INTENCLR = UINT32_MAX;
    clear_events();
    NRF_TWI0->ENABLE = TWI_ENABLE_ENABLE_Enabled;
    const struct recovery_io io = {
        .context = &state, .now_ms = now_ms, .usb_present = usb_present,
        .write = write_bytes, .read = read_bytes, .wait_ms = wait_ms
    };
    const enum recovery_result result = recovery_gate(&io, role);
    cleanup(&state);
    return result;
}
