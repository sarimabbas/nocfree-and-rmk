#ifndef NOCFREE_NRF_STARTUP_H
#define NOCFREE_NRF_STARTUP_H
#include "recovery_gate.h"
/* Call before DFU/SoftDevice initialization with exclusive ownership of
 * TIMER1 and TWI0/TWIM0/SPIM0. Restores pins to disconnected inputs. */
enum recovery_result recovery_nrf_startup(enum recovery_role role);
#endif
