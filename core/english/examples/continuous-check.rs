//! Public-fixture candidate and latency probe; no installation, history or network.
use retype_types::LearningStore;
use std::{path::PathBuf, time::Instant};
struct ReadOnly;
impl LearningStore for ReadOnly {
    fn record(&self, _: retype_types::LearningEvent) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("pass a dictionary directory")?);
    let queries: Vec<String> = args
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
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
    let defaults = [
        "ia",
        "iam",
        "iamb",
        "iamt",
        "iamtr",
        "iamtryingto",
        "iamrobot",
        "iamrobotandilikeapples",
        "jiqiren",
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
    let samples: Vec<&str> = if queries.is_empty() {
        defaults.to_vec()
    } else {
        queries.iter().map(String::as_str).collect()
    };
    for input in samples.iter().copied() {
        let hits = vocabulary.complete_input(input, &ReadOnly, 24, true, true, true);
        println!("sample={input}");
        println!("segment_exact={:?}", vocabulary.segment_exact(input));
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
        for input in samples.iter().copied() {
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
    let mut exact_timings = Vec::new();
    for _ in 0..100 {
        for input in samples.iter().copied() {
            let start = Instant::now();
            std::hint::black_box(vocabulary.segment_exact(input));
            exact_timings.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    exact_timings.sort_by(f64::total_cmp);
    println!(
        "exact_segmentation_samples={} p50_ms={:.4} p95_ms={:.4} max_ms={:.4}",
        exact_timings.len(),
        exact_timings[exact_timings.len() / 2],
        exact_timings[exact_timings.len() * 95 / 100],
        exact_timings[exact_timings.len() - 1]
    );
    Ok(())
}
