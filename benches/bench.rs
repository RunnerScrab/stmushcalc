use criterion::{black_box, criterion_group, criterion_main, Criterion};
use shipdb::{
    ships_from_bytes, simulate_damage, SimConfig, TurnRng, TurnTimingModel, DEFAULT_HORIZON_SECS,
    DEFAULT_RNG_SEED, DEFAULT_SAMPLE_DT, DEFAULT_VOLLEY_WINDOW,
};

fn test_log() -> String {
    let mut s = String::new();
    for i in 0..2000 {
        s.push_str(&format!("~~~ Ind Test Ship {i} (Class {i}) (#{i}) ~~~\r\n"));
        s.push_str("Structure: 700   Repair: 30   Mass: 12345\n");
        s.push_str("Beams: 4   DPS: 123.4\r\n");
        s.push_str("Arcs: FA   FA   PSDV FA\n");
    }
    s
}

fn bench_get_lines(c: &mut Criterion) {
    let text = test_log();
    let bytes = text.as_bytes();
    c.bench_function("get_lines_memchr", |b| {
        b.iter(|| {
            let mut total = 0_usize;
            for line in shipdb::logparse::get_lines_memchr(black_box(bytes)) {
                total += line.len();
            }
            total
        })
    });
}

fn bench_simulate(c: &mut Criterion) {
    let ships = ships_from_bytes(include_bytes!("../ships.bin")).expect("embedded ships.bin");
    let ship = ships
        .iter()
        .max_by_key(|s| s.weapons.len())
        .expect("at least one ship");
    let cfg = SimConfig::new(
        DEFAULT_HORIZON_SECS,
        DEFAULT_SAMPLE_DT,
        TurnTimingModel::Reactive,
        DEFAULT_VOLLEY_WINDOW,
    );
    c.bench_function("simulate_damage", |b| {
        b.iter(|| {
            let mut rng = TurnRng::new(DEFAULT_RNG_SEED);
            simulate_damage(black_box(ship), &mut rng, black_box(&cfg))
        })
    });
}

criterion_group!(benches, bench_get_lines, bench_simulate);
criterion_main!(benches);

