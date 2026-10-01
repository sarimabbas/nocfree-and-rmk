/* Optional existing-Adafruit-bootloader recovery convention.
 * Keep this exact word at app base + 0x200; reserve it before executable text.
 * This changes boot UX and does not replace or modify the bootloader. */
_stext = ORIGIN(FLASH) + 0x204;
SECTIONS {
  .bootloader_recovery (ORIGIN(FLASH) + 0x200) : {
    LONG(0x87eeb07c);
  } > FLASH
} INSERT AFTER .vector_table;
ASSERT(SIZEOF(.vector_table) <= 0x200,
       "Recovery marker overlaps interrupt vectors");
ASSERT(SIZEOF(.bootloader_recovery) == 4,
       "Recovery marker must contain exactly one word");
