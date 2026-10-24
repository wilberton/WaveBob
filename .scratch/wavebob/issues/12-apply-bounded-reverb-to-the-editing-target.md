# 12: Apply bounded reverb to the editing target

**What to build:** Set room/decay/wet-dry controls, apply reverb to the current
target, and inspect or undo the resulting audio.

**Blocked by:** 10 — Undoable gain, fades and reverse on the editing target.

**Status:** done

- [x] GUI and commands share bounded parameter validation and deterministic DSP.
- [x] Selection processing starts with reset state, truncates the tail at target end
  as visibly described, and leaves outside frames and document length unchanged.
- [x] Apply is a cancellable job with atomic revision/history publication and
  correct stale-result handling.
- [x] Impulse, silence and wet/dry fixtures verify the result and undo/redo through
  the command interface rather than reproducing DSP implementation details.

## Specification

[WaveBob editor specification](../spec.md).

## Verification

[Signal, presentation and inspected native MCP checks](../../../docs/verification/ticket12/README.md).
79 ordinary GUI-enabled tests and 54 GUI-free tests pass. Both Clippy variants,
formatting, release build and the native reverb/parameter/capture workflow pass.
