# Tap and hold for keyboard brightness

The installed 400 Hz left image passed the owner's three-step visual dimming and USB typing check. This follow-up changes only brightness-key behavior in RMK; PWM frequency, polarity, brightness scale, storage format and split messages stay the same. A new RMK revision still changes its storage schema hash and can reset settings/bonds.

## Interaction

There are 16 levels: off (0) through full configured PWM duty (15). The lowest nonzero level uses about 6.7% PWM duty; this is not a measurement of perceived light output. Up/down saturate at the endpoints.

A physical brightness-key press changes one level immediately. After 350 ms, holding changes one additional level every 80 ms. From off to level 15, the nominal uninterrupted progression takes 1.39 seconds. These are chosen interaction timings, not measured Magic Keyboard timings or hardware latency bounds.

Release stops the matching physical key, including after a layer or mapping change. A newer brightness direction takes ownership; releasing the older key does not stop it. Releasing the newer key stops repetition without resuming the older held direction. On/off/toggle/step remain single actions and cancel an older brightness hold. Synthetic actions do not start repetition. Sleep clears the hold; wake does not resume it.

At either endpoint the repeat deadline disappears. A late scheduler visit applies at most one step and schedules from the current time, preventing catch-up bursts.

## Architecture and acceptance

RMK's existing keyboard deadline scheduler drives the repeat. The lighting output still receives coalesced absolute state and performs cooperative DMA updates. There is no extra task, subscriber, transport or host-repeat dependency, and no board-local key interpreter.

The keyboard cancels a physical hold at the start of every delivered release event, before action resolution. Host tests for a delivered release do not establish release delivery during real split-link loss. A missing release can continue brightness changes to the bounded endpoint; it is not proof of disconnect recovery. Default Mac F5/F6 live on the left. Both-half lighting passed the owner checks below. Saved brightness, lighting after split reconnect, loss/wake latency and cross-platform acceptance remain separate hardware checks.

The owner-approved left trial installed this image and verified its exact bytes, padding and untouched application gap. USB-first recovery returned successfully. After normal startup, the owner confirmed the tap, held up/down ramp, midpoint release, off endpoint and typing during a hold checks. Both working half images are now retained. The subsequent right-only trial passed exact application/padding/gap readback and independent USB-first recovery. The owner confirmed both halves brighten and dim together, held ramping stops on release, typing stays responsive during a hold, and cross-half Shift works. A slight right-side lighting delay was reported; it has not been measured.

## Software verification

An independent pass added ten deterministic tests, including actual `Keyboard::process_inner` and `Keyboard::run` scheduler integration. The same integration test compiled on prior revision `e4d359c0` and failed at runtime: after 350 ms its brightness was still level 1 rather than level 2. It passes with the new repeat implementation. All 287 framework tests passed, covering delayed repeat, cadence, no catch-up burst, matching and unrelated releases, opposing presses, cached-layer release, delivered split release, sleep, synthetic/other actions and endpoint deadline removal. Delivered-release tests are not proof of radio disconnect recovery.

The root safety/scanner harness (47 Python and seven Rust tests) and high-polarity all-role/keymap/probe cross-builds passed. Low-polarity halves with both keymaps, feature-off halves with both keymaps, and the actual protected receiver configuration also cross-built. Independent [exact-image review](backlight-hold-validation.md) passed both active-high packages and the known pairing-stack review budget. None of these software checks validates hold responsiveness on hardware.
