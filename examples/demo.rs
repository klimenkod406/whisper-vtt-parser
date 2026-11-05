use whisper_vtt_parser::{detect_language, parse_vtt};

fn main() {
    // The stdout progress format printed by `whisper-cli`.
    let stdout = "\
[00:00:00.000 --> 00:00:02.400]   And so the first thing we tried
[00:00:02.400 --> 00:00:05.100]   was rewriting the ingestion layer.
[00:00:07.000 --> 00:00:09.200]   That turned out to matter a lot.
";

    // The .vtt file written by `-ovtt`.
    let vtt = "WEBVTT

00:00:00.000 --> 00:00:02.400
And so the first thing we tried
was rewriting the ingestion layer.

00:00:07.000 --> 00:00:09.200
That turned out to matter a lot.
";

    for (label, input) in [("stdout progress format", stdout), ("vtt file", vtt)] {
        let segments = parse_vtt(input).expect("parse failed");
        println!("--- {label}: {} segment(s) ---", segments.len());
        for s in &segments {
            println!("  [{:>6.2}s -> {:>6.2}s] {}", s.start, s.end, s.text);
        }
        println!();
    }

    let all = parse_vtt(stdout).expect("parse failed");
    let text = all
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    println!("detected language: {}", detect_language(&text));
}
