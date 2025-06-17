use std::{sync::Arc, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rattan_core::metal::timer::Timer;
use tokio::sync::Mutex;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Timer");
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create the runtime");

    for fun in [Duration::from_micros, Duration::from_millis] {
        for duration in [1, 2, 5, 10, 20, 50, 100, 200, 500].into_iter() {
            let duration = fun(duration);
            let timer = {
                let _guard = runtime.enter();
                Arc::new(Mutex::new(Timer::new().unwrap()))
            };
            group
                .warm_up_time((100 * duration).max(Duration::from_secs(5)))
                .measurement_time(
                    (1000 * duration)
                        .max(Duration::from_secs(5))
                        .min(Duration::from_secs(60)),
                )
                .sample_size(30)
                .bench_with_input(
                    BenchmarkId::new("Simple", format!("{duration:?}")),
                    &duration,
                    |b, &duration| {
                        b.to_async(&runtime).iter_custom(|batch_size| {
                            let timer = timer.clone();
                            async move {
                                let mut timer = timer.lock().await;
                                let start = std::time::Instant::now();
                                for _ in 0..batch_size {
                                    timer.sleep_with_timer(duration).await.unwrap()
                                }
                                start.elapsed()
                            }
                        })
                    },
                )
                .bench_with_input(
                    BenchmarkId::new("Optimised", format!("{duration:?}")),
                    &duration,
                    |b, &duration| {
                        b.to_async(&runtime).iter_custom(|batch_size| {
                            let timer = timer.clone();
                            async move {
                                let mut timer = timer.lock().await;
                                let start = std::time::Instant::now();
                                for _ in 0..batch_size {
                                    timer.sleep(duration).await.unwrap()
                                }
                                start.elapsed()
                            }
                        })
                    },
                );
        }
    }

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
