# Bundled MP3 encoder

WaveBob links the bundled LAME 3.100 C encoder through
[`mp3lame-sys` 0.1.11](https://docs.rs/crate/mp3lame-sys/0.1.11).
The locked crate archive SHA-256 is
`54e3b1772db47828840702e5a2e05694527f731abadf9b931355d54035f019d8`.
Its archive contains `lame-3.100/`, generated bindings and the build script.
[`Cargo.lock`](../Cargo.lock) pins its compiler/build dependencies. WaveBob does
not enable the optional decoder; MP3 decoding remains Symphonia.

The build script compiles C sources with `cc`/MSVC on Windows and the supplied
configure script plus `autotools`/make on Unix, statically linking the encoder.
Unix configuration disables shared libraries, the frontend and decoder and builds
position-independent library code. Windows needs the existing MSVC/SDK tools;
Unix needs a C toolchain, make and shell (Linux build-essential, macOS Xcode tools).
No encoder executable, runtime codec download or user-installed LAME is needed.
The Windows static/headless build is verified; macOS/Linux native builds remain
under the user's remote/CI deferral.

The private Rust facade validates settings and uses owned handles and bounded
initialized buffers around the C API. It explicitly sets both input and output
rate, mono/joint-stereo mode, CBR, bitrate and quality 2. Normal final flush is
followed by replacement of LAME's reserved first MPEG frame with its final
Info/LAME tag. Inserting another frame or using streaming no-gap flush would
not implement this file-export path. Output summaries report raw encoder delay
and padding. Import metadata reports the decoder-adjusted trimming values, which
also include decoder delay. Working PCM is never replaced by lossy re-decoding.

The [recorded independent matrix](verification/ticket13/encoder-qa.json) covers
mono/stereo, all four offered bitrates and all three rates, using selected ranges.
FFmpeg 7.1 independently recovers the exact intended frame counts. The signal
comparison uses analytic tones after metadata trimming, with no fitted offset:
interior RMS error must be below 0.02 and maximum error below 0.07 full scale.
Measured maxima are RMS 0.017789 and maximum error 0.043721. FFmpeg is QA tooling
only. Ordinary tests additionally reimport all settings through Symphonia and
cover a one-frame MP3.

## Distribution inputs

The Rust bindings declare LGPL-3.0; the bundled LAME sources contain the GNU
Library GPL v2 text. Unmodified license texts are retained under
[`assets/licenses`](../assets/licenses), including the GNU GPL v3 incorporated
by LGPL v3. LAME's project is [lame.sourceforge.io](https://lame.sourceforge.io/).
The encoder source is unmodified. The bindings' optional GPL decoder is disabled.

Binary redistribution must retain the library notices/license texts and provide
corresponding library source and suitable application materials for relinking a
modified library, following the license's combined-work terms. The release work
in ticket 21 must package these materials and reproducible instructions; a bare
statically linked executable is not the complete redistribution kit. The exact
crate archive above supplies the pinned library sources/build script. Application
licensing and public distribution are separate from these local development builds.
