# v1 finishing work

This is the current release checklist. Earlier trial documents are historical
and must not be used as current startup instructions.

## Current checkpoints

- macOS owner typing checks passed over USB, direct Bluetooth, and the RMK dongle,
  including cross-half Shift. This is not an instrumented loss/latency test.
- Companion runtime recovery and exact application readback passed on both
  halves and the dongle. Factory bootloaders are retained.
- Backlight tap, hold, split synchronization, and saved brightness passed owner
  checks. Battery percentages are displayed from voltage-derived firmware data;
  remaining capacity and battery life are not physically calibrated.
- USB-powered halves have the reviewed awake override. Owner reports immediate
  first keys after six minutes idle with both USB cables connected.
- The 30-minute battery idle candidate is installed on LEFT with settings
  preserved; first-key sleep/wake acceptance is pending.

## Finish before v1

1. Complete battery sleep/wake checks separately for first input from right and
   first input from left; check release, reconnection, and USB-power override.
2. Verify the blue status LED candidate. Green and a right blue channel are not
   established by the published hardware mapping.
3. Finalize three Bluetooth profiles and the shortcut guide together. Current
   framework User action IDs depend on profile count; changing only the count
   would change maintenance shortcuts. Remove unused legacy maintenance bindings
   deliberately, retaining Companion recovery.
4. Verify install, update, backup, and factory restore using the shared recovery
   journey. Use user-provided factory UF2s or checked backups, never bundled
   official firmware. Keep rollback and role identity checks.
5. Finish version reporting on every role and live-check factory version querying.
6. Validate Windows/Linux and instrumented input/wake/current behavior before
   claiming those platforms, no-loss performance, or battery endurance.

No bootloader rewrite, extra shim, new dongle protocol, or new configuration
application is needed for these finishing steps. Keep Vial for remapping.
