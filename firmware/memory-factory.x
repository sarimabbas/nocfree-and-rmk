/* Compile-only layout until INFO_UF2.TXT and recovery are verified.
 * Preserve factory SoftDevice, filesystem and bootloader. */
MEMORY {
  FLASH : ORIGIN = 0x27000, LENGTH = 0x3e000
  RAM : ORIGIN = 0x20008000, LENGTH = 96K
}
