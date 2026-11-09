//! Public-fixture candidate and latency probe; no installation, history or network.
use retype_types::LearningStore;
use std::{path::PathBuf, time::Instant};
struct ReadOnly;
impl LearningStore for ReadOnly {
    fn record(&self, _: retype_types::LearningEvent) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("pass a dictionary directory")?,
    );
    let path = std::fs::read_dir(&root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("english-e") && name.ends_with(".bin"))
        })
        .ok_or("English vocabulary not found")?;
    retype_english::install(&path).map_err(std::io::Error::other)?;
    let vocabulary = retype_english::current();
    let samples = [
        "ia",
        "iam",
        "iamb",
        "iamt",
        "iamtr",
        "iamtryingto",
        "howarey",
        "thanky",
        "wearet",
        "iambic",
        "international",
        "apple",
        "IAMT",
        "don't",
        "pingguo",
        "nihao",
        "qzxvfixture",
        "zzzzzzzzzz",
    ];
    for input in samples {
        let hits = vocabulary.complete_input(input, &ReadOnly, 24, true, true, true);
        println!("sample={input}");
        for (index, candidate) in hits.iter().take(5).enumerate() {
            println!(
                "{}\tdisplay={:?}\tcommit={:?}\tscore={:.4}",
                index + 1,
                candidate.display_text(),
                candidate.text,
                candidate.score
            );
        }
    }
    let mut timings = Vec::new();
    for _ in 0..100 {
        for input in samples {
            for end in 1..=input.len() {
                let start = Instant::now();
                std::hint::black_box(vocabulary.complete_input(
                    &input[..end],
                    &ReadOnly,
                    24,
                    true,
                    true,
                    true,
                ));
                timings.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    timings.sort_by(f64::total_cmp);
    println!(
        "latency_samples={} p50_ms={:.4} p95_ms={:.4} max_ms={:.4}",
        timings.len(),
        timings[timings.len() / 2],
        timings[timings.len() * 95 / 100],
        timings[timings.len() - 1]
    );
    Ok(())
}
