/* Explicit S140-to-SDC migration layout; NOT approved for flashing.
 * Replaces resident SoftDevice at 0x1000; preserves the MBR, factory
 * filesystem, bootloader, metadata and low 32 KiB RAM boot marker.
 * RMK storage remains 0x65000..0x6d000. */
MEMORY {
  FLASH : ORIGIN = 0x1000, LENGTH = 0x64000
  RAM : ORIGIN = 0x20008000, LENGTH = 96K
}
