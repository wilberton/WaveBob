# Portable WaveBob projects

`project.save` takes path (ending in `.wavebob`), overwrite and optional
expected_project_revision. It captures documents, working revisions, pinned clip
sources, instance processing, arrangement controls/navigation and project rate.
The JSON manifest has schema version 1 and a SHA-256 content checksum. Managed
audio lives beside it in `<project name>.assets/` as lossless float32 WAV files.
MP3-origin audio is never re-encoded to MP3 for project persistence.

Assets are keyed by immutable audio revision and have PCM/dimension checksums.
Repeated saves validate and reuse existing assets; new revisions add new assets.
Assets are published before the manifest is atomically replaced, so an older
manifest always keeps valid references. Cancellation before publication removes
staged temporary files. Once publication starts, cancellation reports busy.
Failed manifest publication can leave complete, unreferenced immutable assets;
they are safe to retain and are not used by the old manifest. Automatic asset
garbage collection is outside this first version.

Copy the `.wavebob` file and its managed asset directory together. Source paths
are provenance; reopening needs no original WAV/MP3 files. `project.open` takes
path and explicit discard_changes policy. It validates the full manifest, paths,
checksums, audio dimensions and arrangement before replacing the current workspace.
Missing/corrupt assets, unsupported schema versions, unsafe references or work
changed during loading fail without replacing current audio. Loading stages audio
within the existing 2 GiB retained PCM budget; manifests are bounded to 16 MiB.

`project.get` reports the current/saved project revision, path, dirty state and
saved working document revisions. Project revisions exclude navigation, transport,
clipboard, jobs and presentation preferences. Undo history and clipboard do not
survive project reopen. Current waveform working audio, pinned older source audio,
processed clip audio and all stable object IDs do survive. Layout remains separate.

Sample dirty flags refer to their original audio files. A project can safely
preserve such a working revision without replacing the source. Quitting a clean
saved project requires no source re-encoding, and a source-dirty document already
preserved in the project can close safely. Later edits mark the project dirty;
undo/redo reflects the exact saved baseline. A save completing after newer edits
records only its captured snapshot and leaves the newer workspace dirty.

Managed assets are immutable output destinations. Sample Save/export and mix
export must choose a destination outside managed asset directories. For source
paths that are relative or from another platform, choose an explicit Save As
destination when writing a standalone sample.

Both commands return jobs. Their output kinds are project_saved/project_opened,
with path, schema/revision, object/asset counts and asset reuse information.
The core persistence path is shared by GUI, headless and MCP. The File menu offers
Open project, Save project and Save project as. In multitrack mode Ctrl/Cmd+S saves
the project and Ctrl/Cmd+Shift+S chooses a new project destination. In waveform
mode those shortcuts save the standalone sample. Project close/open prompts offer
Save project, Discard and Cancel; failed loads retain the current workspace.
