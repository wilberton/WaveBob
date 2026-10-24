# 23: Load audio from external file drops

**What to build:** Drop one or several native WAV/MP3 files onto the editor to
open documents using the existing shared import commands. Show a clear drop
affordance and handle batches, unsupported paths and import failures.

**Blocked by:** 06 — WAV and MP3 import with scalable waveform jobs.

**Status:** done

- [x] Native file drops load WAV/MP3 in either mode through documents.open, with
  background progress and actionable errors. Batch drops respect worker limits.
- [x] Hover feedback uses the existing visual language and persistent status help.
  Imports do not accidentally select audio or create arrangement clips.
- [x] Input tests cover multi-file/mixed-format batches, unsupported/missing files,
  one-shot event consumption, modal/capture deferral and cancellation/close safety.
- [x] Verify the native input path and inspect actual MCP screenshots.

## Verification

Implementation `783f6cc`. Five focused file-event tests pass, including real IPC
capture deferral. Native renderer/MCP batch tests pass at 100% and 150% scale.
See the [captures and test scope](../../../docs/verification/tickets23-24/README.md).

## Specification

[Editor spec](../spec.md). Directly requested and approved on 24 October 2026.
