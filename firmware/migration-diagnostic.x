/* Tiny stage-return probes must erase the old S140 detection word at 0x3004.
 * Put explicit erased data into the ELF rather than padding an unlinked BIN.
 * The normal USB/entry probes do not use this section. */
SECTIONS {
  .migration_diagnostic_extent
    MAX(ADDR(.text) + SIZEOF(.text), ORIGIN(FLASH) + 0x2004) : {
    LONG(0xffffffff);
  } > FLASH
} INSERT AFTER .text;
ASSERT(ORIGIN(FLASH) == 0x1000,
       "Diagnostic extent requires the explicit lower migration layout");
ASSERT(ADDR(.migration_diagnostic_extent) + SIZEOF(.migration_diagnostic_extent)
       >= 0x3008,
       "Diagnostic must cover the complete old S140 detection word");
ASSERT(__sdata <= __edata && __edata <= ORIGIN(RAM) + LENGTH(RAM)
       && __sdata >= ORIGIN(RAM),
       "Diagnostic must preserve runtime data-copy RAM bounds");
ASSERT(__sbss <= __ebss && __ebss <= ORIGIN(RAM) + LENGTH(RAM)
       && __sbss >= ORIGIN(RAM),
       "Diagnostic must preserve runtime BSS-zeroing RAM bounds");
