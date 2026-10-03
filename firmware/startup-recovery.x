/* Reserve the old USB-first marker location, without requesting recovery.
 * Executable entry remains beyond the reserved word. */
_stext = ORIGIN(FLASH) + 0x204;
SECTIONS {
  .startup_recovery_reserve (ORIGIN(FLASH) + 0x200) : {
    LONG(0xffffffff);
  } > FLASH
} INSERT AFTER .vector_table;
ASSERT(SIZEOF(.vector_table) <= 0x200,
       "Startup recovery reserve overlaps interrupt vectors");
