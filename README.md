# whisper-vtt-parser

[![CI](https://github.com/klimenkod406/whisper-vtt-parser/actions/workflows/ci.yml/badge.svg)](https://github.com/klimenkod406/whisper-vtt-parser/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

Parse the output of **`whisper.cpp` / `whisper-cli`** into timed transcription
segments — handling **both** output formats the tool produces.

Pure Rust. One dependency: `serde`.

```text
whisper.cpp
   │
   ├─► .vtt file  (written by -ovtt)      ──┐
   │     00:00:00.000 --> 00:00:02.100      │
   │     the cue text lives here             ├──► parse_vtt() ──► [ Segment ]
   │                                         │
   └─► stdout progress                       │
         [00:00:00.000 --> 00:00:02.100]  text
                                                │
                                                └─ identical segments either way
```

Open-source reference implementation extracted from my own open-source project
Sofia — a local-first desktop note-taking app with on-device AI (Tauri 2, Rust).

## The problem this solves

`whisper.cpp` emits cues in two shapes, and a parser that only understands one
of them produces **zero segments** — the classic *"it ran, but nothing
happened"* bug, because every cue is silently dropped rather than erroring.

| Format | Where it comes from | Shape |
|---|---|---|
| **VTT file** | `-ovtt` writes `out.vtt` | timestamp alone on its line, text on the next lines |
| **stdout progress** | `whisper-cli` prints to stdout | `[t1 --> t2]` and the text **inline**, after the bracket |

This crate parses both and yields **identical segments** from either, so the
caller does not have to know which one it captured.

## Install

```toml
[dependencies]
whisper-vtt-parser = "0.1"
```

## Usage

```rust
use whisper_vtt_parser::parse_vtt;

// Either format works.
let stdout = "[00:00:00.000 --> 00:00:02.400]   And so the first thing we tried\n";
let vtt    = "WEBVTT\n\n00:00:00.000 --> 00:00:02.400\nAnd so the first thing we tried\n";

assert_eq!(parse_vtt(stdout).unwrap(), parse_vtt(vtt).unwrap());

for segment in parse_vtt(stdout).unwrap() {
    println!("[{:.2}s -> {:.2}s] {}", segment.start, segment.end, segment.text);
}
```

Run the bundled demo to see both formats side by side:

```bash
cargo run --example demo
```

## What the parser handles

- **Both output formats**, producing identical results.
- **VTT headers** — `WEBVTT`, `NOTE`, `Kind:`, `Language:` — skipped.
- **Multi-line cue text** — joined with a single space.
- **Cues with no text** — dropped, since an empty segment is never useful.
- **Malformed timestamp lines** — skipped instead of aborting the parse. A
  partial transcript is more useful than none.
- **`HH:MM:SS.mmm` and `MM:SS.mmm`** timestamps, plus hours.

## Language heuristic

`detect_language` is deliberately minimal — it exists to pick a model, not to
be a language detector:

- more than **30%** CJK ideographs among alphabetic characters → `"zh"`
- anything else → `"auto"` (let the backend decide)

```rust
use whisper_vtt_parser::detect_language;

assert_eq!(detect_language("你好世界"), "zh");
assert_eq!(detect_language("hello world"), "auto");
```

## Design notes

- **Pure.** No I/O, no clock, no subprocess. Feed it a string, get segments
  back.
- **Never fails.** The current implementation cannot return `Err`; malformed
  cues are skipped so a partial transcript stays usable. `ParseError` exists
  so that adding a strict mode later does not break the API.
- **Deterministic.** Same input, same output — which makes it safe to run
  concurrently and trivial to test.
- **No `unwrap` on input.** The parser tolerates anything that comes out of
  a subprocess.

## Testing

```bash
cargo test
```

13 unit tests cover: timestamp parsing (1-, 2- and 3-component, plus
malformed input), both output formats, equivalence of the two formats,
header and note skipping, multi-line cue joining, empty-cue dropping,
malformed-line tolerance, and the language heuristic including its threshold
boundary.

## License

MIT
