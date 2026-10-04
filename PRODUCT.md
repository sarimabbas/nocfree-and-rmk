# NocFree RMK Companion

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

## Users

Owners of the NocFree AND split keyboard, including newcomers who do not want to understand firmware internals.

## Product Purpose

Guide owners between factory and RMK firmware, maintain recoverable firmware copies, and show the keyboard's observed state. The current application supports read-only copies; a production firmware writer is not yet implemented.

## Stack

Native Rust application using the GPUI Kit library documented at gpui-kit.com. macOS is the current observed platform; Windows and Linux remain release work.

## Operating Context

The device has a left half, a right half, and an optional USB receiver. Firmware and startup recovery capabilities vary by installed image. Physical switches and battery-only power are not directly observable over USB.

## Capabilities and Constraints

Use one reusable state model and journey canvas. Advance from reliable device observations automatically. Ask only for physical actions that cannot be observed. Keep factory images user supplied or privately backed up; do not distribute them. USB attachment alone does not prove charging, and unreadable battery data must stay unknown. A saved readback is not automatically a restore-compatible image.

## Brand Commitments

A conventional, polished desktop sidebar application with an Apple-like sense of calm and ease. Preserve the existing independent ampersand icon and drawings of the distinctive split board.

## Product Principles

- Reduce decisions to the next useful action.
- Share journey behavior and vary only actual device capabilities and operation policy.
- Make cancellation and recovery of interrupted work clear and truthful.
- Show observed facts; keep development internals out of the normal experience.
- Verify software and hardware separately.
