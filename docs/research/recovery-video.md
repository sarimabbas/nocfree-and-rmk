# Manufacturer recovery video: frame review

Source: [NocFree Right Keyboard Firmware Update Tutorial | V2.0](https://www.youtube.com/watch?v=jGfepV1DYVE), published by NocFree. Duration 95 seconds. This is a right-half tutorial and an older firmware generation; neither the user's left PCB nor current firmware version can be identified from it.

Selected frames were inspected directly; no firmware was written. The video's actions are evidence, not permission to perform disassembly or flash a device.

- **00:10:** the factory updater UI lists software DFU shortcuts: left Fn+5, right Fn+0, held about five seconds. This provides a potential MSC entry distinct from the 1200-baud CDC-only path already observed on the user's left half.
- **00:25–00:38:** the demonstration removes the backplate and exposes the internal controller module.
- **00:40–00:46:** metal tweezers contact two round edge pads on the module. These are not the fine-pitch package legs of the central chip. The captions identify the fourth and last pads in the demonstrated orientation and repeated quick contacts. No electrical signal labels are visible; do not infer a left-half pin map or transpose the video to another board revision.
- **00:55:** Finder shows the mounted NocFree drive with `CURRENT.UF2`, `INFO_UF2.TXT` and `INDEX.HTM`. This establishes that the demonstrated board has an MSC recovery mode, although the user's observed 1200-baud route did not expose it.
- Later frames demonstrate factory half re-pairing after application replacement; these factory commands do not carry over automatically to RMK.

The immediate non-invasive next step is to test the factory left software shortcut while the existing application still works, read bootloader metadata, and copy readback files **off** the drive. No files should be copied **onto** it. A working software shortcut is not recovery independent of application execution.

The manufacturer [troubleshooting page](https://www.nocfree.com/pages/nocfree-and-troubleshooting) also publishes separate left and right recovery guides and cautions against opening/bridging internals without support guidance. The frame review does not substitute for identifying the owner's actual contacts and board revision. A physical recovery demonstration on another board is not proof on this one.
