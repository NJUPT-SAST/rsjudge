use std::{hint::black_box, io, path::Path};

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use rsjudge_judger::{comparer::DefaultComparer, Comparer as _};
use tokio::{fs::File, runtime::Runtime};

fn bench(c: &mut Criterion) {
    let data_dir = Path::new("./benches/data");
    let data_file = data_dir.join("100M");
    let data_file_trimmed = data_dir.join("100M.trim");
    const MB: u64 = 1024 * 1024;
    const COMPARERS: &[(DefaultComparer, &str)] = &[
        (DefaultComparer::common(), "common"),
        (DefaultComparer::exact_match(), "exact-match"),
        (
            DefaultComparer::new(false, true, true),
            "case-insensitive-common",
        ),
    ];

    let mut group = c.benchmark_group("DefaultComparer");
    group.sampling_mode(SamplingMode::Flat);

    for (comparer, id) in COMPARERS {
        group.throughput(Throughput::Bytes(100 * MB));
        group.bench_with_input(BenchmarkId::from_parameter(id), comparer, |b, comparer| {
            b.to_async(Runtime::new().unwrap()).iter(|| async {
                let result = black_box(comparer)
                    .compare(
                        File::open(black_box(&data_file)).await?,
                        File::open(black_box(&data_file_trimmed)).await?,
                    )
                    .await?;

                Ok::<_, io::Error>(result)
            });
        });
    }
}

criterion_group!(benches, bench);

criterion_main!(benches);
