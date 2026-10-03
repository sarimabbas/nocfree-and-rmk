#ifndef NOCFREE_LEFT_BOOT_BOARD_H
#define NOCFREE_LEFT_BOOT_BOARD_H

/* Verified from the original LEFT bootloader, not inferred for the right. */
#define LEDS_NUMBER 2
#define LED_PRIMARY_PIN 47
#define LED_SECONDARY_PIN 42
#define LED_STATE_ON 1

#define LED_NEOPIXEL 16
#define NEOPIXELS_NUMBER 1
#define BOARD_RGB_BRIGHTNESS 0x040404

#define BUTTONS_NUMBER 2
#define BUTTON_1 34
#define BUTTON_2 10
#define BUTTON_PULL NRF_GPIO_PIN_PULLUP

#define BLEDIS_MANUFACTURER "NocFree"
#define BLEDIS_MODEL "NocFree &"
#define USB_DESC_VID 0x239A
#define USB_DESC_UF2_PID 0x0029
#define USB_DESC_CDC_ONLY_PID 0x002A
#define UF2_PRODUCT_NAME "NocFree &"
#define UF2_VOLUME_LABEL "NocFree &"
#define UF2_BOARD_ID "NocFree &"
#define UF2_INDEX_URL "https://www.nocfree.com/"

/* No REGOUT0 or DC/DC settings: original board_init writes neither. */
#endif
