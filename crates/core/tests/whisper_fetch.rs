//! The whole of it, against the real releases — run by hand, never in CI.
//!
//! `SPOKEN=/path/to/speech.wav cargo test -p agentland-core --test whisper_fetch
//! -- --ignored --nocapture`
//!
//! It fetches about 160 MB, which is why it is ignored, and it is the only
//! thing that proves the parts fit: the build comes down, unpacks, is made
//! runnable, and reads a sentence back. Measured with whisper.cpp's own
//! `samples/jfk.wav`, which came back word for word in 35 seconds.

use std::path::{Path, PathBuf};

#[tokio::test]
#[ignore]
async fn it_fetches_whisper_and_reads_a_sentence_back() {
    let data: PathBuf = std::env::temp_dir().join("agentland-whisper-live");
    std::fs::create_dir_all(&data).unwrap();

    let standing = agentland_core::whisper::fetch(&data, "base", |says| println!("· {says}"))
        .await
        .expect("whisper was fetched");

    assert!(standing.ready, "both halves are here");
    let tool = standing.tool.expect("the tool");
    let model = standing.model.expect("the model");

    let line = agentland_core::whisper::transcriber_line(Path::new(&tool), Path::new(&model));
    println!("· {line}");

    let spoken = std::env::var("SPOKEN").expect("SPOKEN names a wav to read");
    let said = agentland_core::voice::read_back(
        &data,
        &std::fs::read(&spoken).expect("the recording"),
        "audio/wav",
        &line,
    )
    .expect("read back");

    println!("· heard \"{said}\"");
    assert!(!said.trim().is_empty(), "whisper said nothing");
}
