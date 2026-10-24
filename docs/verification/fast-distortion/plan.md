# Fast Distortion reference plan

Pinned Lemon-Studio 0af860dfd31c7ee9cbf8af6747617e031a1a7742,
`effects/fast_dist.rs`, `os-dsp/src/level.rs` and the small-fx Fast Distortion
parameter/editor source. Five integer controls: pre 64..192 (128), threshold
1..10 (10), type A/B 0..1 (0), mix 0..128 (128), post 0..128 (128).
Level controls use the source exponential law; wet=mix*post and
 dry=(post-wet)*pre*post in prescribed f32 precision. The table has 8192 whole
16-bit levels plus a repeated last value; interpolation and final toward-zero
f32 rounding preserve signed and very loud input behavior.

Predeclare maximum absolute PCM error 2e-7, no gain/frame correction. Compare
all 20 threshold/type curves, level/mix extremes, table-index neighbors,
positive/negative amplitude sweeps and steps, impulse, multitone/stereo and
large finite values at 44.1/48/96 kHz. Hold out a mixed-setting stereo case until
development passes. Compile original reference files unchanged in isolation.

No oversampling/antialiasing or new modes: reference aliasing is preserved.
Mono stays mono; stereo processes independently; zero latency/tail and reset
is memoryless. Tables prepare outside the callback. Same-curve pre/mix/post
coefficients slew over 10 ms; threshold/type changes select prepared 20 ms
crossfades. Fixed settings remain the reference target.

Verify public offline/manual blocks 1/7/64, headless/MCP parity, seek/reset/loop,
instance isolation, nonlinear order/bypass, preview cancellation/one-step undo,
callback allocations/frees and finite transitions. Inspect actual native
INPUT/DISTORTION/CURVE/OUTPUT controls and exact transfer graph. Physical
listening/device checks remain unverified under approved automated acceptance.
