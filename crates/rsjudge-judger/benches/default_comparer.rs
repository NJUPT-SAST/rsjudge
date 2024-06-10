use std::{hint::black_box, io, path::Path};

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use rsjudge_judger::{comparer::DefaultComparer, Comparer as _};
use tokio::{fs::File, runtime::Runtime};

fn bench(c: &mut Criterion) {
    let data_dir = Path::new("./benches/data");
    const KILO: u64 = 1024;
    const MEGA: u64 = 1024 * 1024;
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
    for (size_str, bytes) in [
        ("1k", KILO),
        ("16k", 16 * KILO),
        ("256k", 256 * KILO),
        ("4M", 4 * MEGA),
        ("64M", 64 * MEGA),
    ] {
        let data_file = data_dir.join(size_str);

        let data_file_trimmed = data_dir.join(format!("{}.trim", size_str));
        for (comparer, id) in COMPARERS {
            group.throughput(Throughput::Bytes(bytes));
            group.bench_with_input(
                BenchmarkId::new(*id, size_str),
                &(data_file.as_path(), data_file_trimmed.as_path()),
                |b, (data_file, data_file_trimmed)| {
                    b.to_async(Runtime::new().unwrap()).iter(|| async {
                        let result = black_box(comparer)
                            .compare(
                                File::open(black_box(data_file)).await?,
                                File::open(black_box(data_file_trimmed)).await?,
                            )
                            .await?;

                        Ok::<_, io::Error>(result)
                    });
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench);

criterion_main!(benches);
