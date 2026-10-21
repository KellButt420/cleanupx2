use cleanup_operations::{Event, summaries};
use std::{hint::black_box, time::Instant};

fn main() {
    for count in [1_000, 10_000] {
        let events: Vec<Event> = (0..count)
            .map(|n| Event::UninstallRecorded {
                batch_id: format!("fixture-{n}"),
                at_unix_ms: n as u64,
                app_name: "Disposable benchmark app".into(),
                publisher: None,
                status: "removed".into(),
                message: "Synthetic record; no installer was launched.".into(),
            })
            .collect();
        let started = Instant::now();
        for _ in 0..5 {
            let result = summaries(black_box(&events));
            assert_eq!(result.len(), count);
            black_box(result);
        }
        println!(
            "{count} batches: {:.3} ms/summary (five runs, synthetic memory-only journal)",
            started.elapsed().as_secs_f64() * 200.0
        );
    }
}
