# nRF52833 startup adapter

`nrf_startup.c` implements the portable gate's callbacks using nRF52833 TIMER1,
legacy TWI0, GPIO and POWER USB VBUS registers. It has compiled as an ARM
Cortex-M4 object with strict warnings against the pinned bootloader's nrfx MDK
and CMSIS headers. This establishes compilation, not device behavior or recovery.

Call `recovery_nrf_startup(role)` before starting DFU, radio, or SoftDevice, with
exclusive ownership of TIMER1 and TWI0/TWIM0/SPIM0. Explicitly skip the adapter
for OTA application jumps that arrive with SoftDevice already active. The pinned
bootloader uses TIMER2 CC0 for its version register and board initialization uses
RTC1/SysTick; source inspection found no bootloader TIMER1 use. Optional board
display code uses SPIM0, so a real NocFree build must exclude that configuration.
TWI's ENABLE alias check rejects an already enabled shared peripheral; there is
no hardware running-status register that proves TIMER1 ownership, so that is an
integration precondition. IRQs for these exclusively owned peripherals are
disabled, pending states cleared, and remain disabled on exit.

SDA is P0.11, SCL P1.09, open-drain with internal pull-ups, at 100 kHz. Three
PCA9555 input expanders are configured and read by the gate. The register pointer
write uses STOP before the two-byte read; PCA9555 preserves its pointer across
STOP. BB_SUSPEND pauses after the first byte, then BB_STOP ends the second byte,
matching Nordic's legacy TWI driver receive sequencing. The pointer write and
two input bytes share a 4 ms deadline, rather than accumulating separate budgets.

TIMER1 runs at 1 MHz. On a functioning timer, transfers check a shared elapsed
4 ms deadline and waits check the requested delay, up to 10 ms. Cleanup attempts
STOP for up to 1 ms, then disables TWI even if the bus remains stuck. All polling
also has a 200000-iteration termination bound if the timer stops. **That finite
instruction bound is not a proven wall-clock 4/10/1 ms bound with a failed clock.**
The portable harness's callback timing assumptions and 486 ms frozen-clock model
do not prove a worst-case device startup duration for this adapter. Actual timing
requires hardware observation. It neither spins forever on an unresponsive bus
nor fabricates a successful read; errors cause ordinary application-validity flow.

Cleanup disables the shared TWI peripheral before disconnecting both pins,
clears its interrupts/events/error status and selection registers, and stops,
clears and disables TIMER1 interrupts. No flash, markers, storage, UICR, radio,
clock-source or expander output state is written. GPIO inputs are disconnected
on exit instead of preserving an unrelated board configuration: these pins must
be owned only by this startup check at the integration seam.

Primary sources:

- [Pinned Adafruit bootloader main](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c)
- [Pinned board initialization and optional SPI display](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/boards/boards.c)
- [Pinned Nordic nrfx TWI driver](https://github.com/NordicSemiconductor/nrfx/blob/7a4c9d946cf1801771fc180acdbf7b878f270093/drivers/src/nrfx_twi.c)
- [PCA9555 datasheet](https://www.ti.com/lit/ds/symlink/pca9555.pdf)
- [NocFree community pin table](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting)

Neither role has been tested on hardware with this adapter. The dongle is excluded.
