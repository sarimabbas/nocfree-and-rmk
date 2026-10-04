# Companion HID worker lifetime

The owner's crash report places the failure in macOS `CFRunLoopAddSource`,
`IOHIDDeviceScheduleWithRunLoop`, HIDAPI enumeration and Companion battery
polling on a temporary GPUI background thread. The pinned HIDAPI backend keeps
a process-global manager scheduled on its initial caller's run loop.
[HIDAPI's threading contract](https://github.com/libusb/hidapi/wiki/Multi%E2%80%90threading-Notes)
requires that macOS initialization thread to remain alive until shutdown.

Battery reads now run entirely on one process-lifetime worker, including handle
creation and destruction. The worker retains the single-flight lease after a
caller times out, preventing overlapping native calls or queued replacements.
The existing GET reports, identity checks and observation deadlines are retained.

Focused tests cover 32 expiring callers using the same surviving worker and
abandoned observations retaining their lease. Bounded native enumeration stress
did not reproduce the original intermittent trap; the change enforces the
documented lifetime contract but does not prove universal crash-free operation.
The supplied report and native test artifacts remain private.
