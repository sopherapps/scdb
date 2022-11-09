use std::iter::{IntoIterator, Iterator};
use std::string::ToString;

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

use scdb::Store;

const STORE_PATH: &str = "testdb";

// Setting
fn setting_without_ttl_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    for (k, v) in &records {
        c.bench_function(
            &format!("set(no ttl): '{}'", String::from_utf8(k.clone()).unwrap(),),
            |b| {
                b.iter_with_large_drop(|| {
                    store.set(
                        black_box(&k.clone()),
                        black_box(&v.clone()),
                        black_box(None),
                    )
                })
            },
        );
    }
}

fn setting_with_ttl_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
    let records = get_records();
    for (k, v) in &records {
        c.bench_function(
            &format!("set(ttl): '{}'", String::from_utf8(k.clone()).unwrap(),),
            |b| {
                b.iter_with_large_drop(|| {
                    store.set(black_box(&k.clone()), black_box(&v.clone()), black_box(ttl))
                })
            },
        );
    }
}

// Updating
fn updating_without_ttl_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    let updates = get_updates();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }
    for (k, v) in &updates {
        c.bench_function(
            &format!(
                "update(no ttl): '{}'",
                String::from_utf8(k.clone()).unwrap(),
            ),
            |b| b.iter_with_large_drop(|| store.set(black_box(k), black_box(v), black_box(None))),
        );
    }
}

fn updating_with_ttl_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    let updates = get_updates();
    let ttl = Some(3_600u64);
    for (k, v) in &records {
        store.set(k, v, ttl).expect(&format!("set {:?}", k));
    }
    for (k, v) in &updates {
        c.bench_function(
            &format!("update(ttl): '{}'", String::from_utf8(k.clone()).unwrap(),),
            |b| b.iter_with_large_drop(|| store.set(black_box(k), black_box(v), black_box(ttl))),
        );
    }
}

// Getting
fn getting_without_ttl_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records {
        c.bench_function(
            &format!("get(no ttl): '{}'", String::from_utf8(k.clone()).unwrap()),
            |b| b.iter_with_large_drop(|| store.get(black_box(k))),
        );
    }
}

fn getting_with_ttl_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    let ttl = Some(3_600u64);

    for (k, v) in &records {
        store.set(k, v, ttl).expect(&format!("set {:?}", k));
    }
    for (k, _) in &records {
        c.bench_function(
            &format!("get(with ttl): '{}'", String::from_utf8(k.clone()).unwrap()),
            |b| b.iter_with_large_drop(|| store.get(black_box(k))),
        );
    }
}

// Deleting
fn deleting_without_ttl_benchmark(c: &mut Criterion) {
    let records = get_records();
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records {
        c.bench_function(
            &format!(
                "delete(no ttl): '{}'",
                String::from_utf8(k.clone()).unwrap()
            ),
            |b| b.iter(|| store.delete(black_box(k))),
        );
    }
}

fn deleting_with_ttl_benchmark(c: &mut Criterion) {
    let records = get_records();
    let ttl = Some(3_600u64);
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    for (k, v) in &records {
        store.set(k, v, ttl).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records {
        c.bench_function(
            &format!("delete(ttl): '{}'", String::from_utf8(k.clone()).unwrap()),
            |b| b.iter(|| store.delete(black_box(k))),
        );
    }
}

// Clearing
fn clearing_without_ttl_benchmark(c: &mut Criterion) {
    c.bench_function("clear(no ttl)", |b| {
        b.iter_batched(
            || {
                let mut store =
                    Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
                store.clear().expect("clear store");
                let records = get_records();
                for (k, v) in &records {
                    store.set(k, v, None).expect(&format!("set {:?}", k));
                }
                store
            },
            |mut store| store.clear(),
            BatchSize::PerIteration,
        )
    });
}

fn clearing_with_ttl_benchmark(c: &mut Criterion) {
    c.bench_function("clear(ttl)", |b| {
        b.iter_batched(
            || {
                let mut store =
                    Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
                store.clear().expect("clear store");
                let records = get_records();
                let ttl = Some(3_600u64);
                for (k, v) in &records {
                    store.set(k, v, ttl).expect(&format!("set {:?}", k));
                }
                store
            },
            |mut store| store.clear(),
            BatchSize::PerIteration,
        )
    });
}

// Compacting
fn compacting_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    for (k, v) in &records[..3] {
        store.set(k, v, Some(1)).expect(&format!("set {:?}", k));
    }

    for (k, v) in &records[3..] {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records[2..3] {
        store.delete(k).expect(&format!("delete {:?}", k));
    }

    c.bench_function("compact", |b| b.iter(|| store.compact()));
}

fn get_records() -> Vec<(Vec<u8>, Vec<u8>)> {
    [
        ("hey", "English"),
        ("hi", "English"),
        ("salut", "French"),
        ("bonjour", "French"),
        ("hola", "Spanish"),
        ("oi", "Portuguese"),
        ("mulimuta", "Runyoro"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string().into_bytes(), v.to_string().into_bytes()))
    .collect()
}

fn get_updates() -> Vec<(Vec<u8>, Vec<u8>)> {
    [
        ("hey", "Jane"),
        ("hi", "John"),
        ("hola", "Santos"),
        ("oi", "Ronaldo"),
        ("mulimuta", "Aliguma"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string().into_bytes(), v.to_string().into_bytes()))
    .collect()
}

criterion_group!(
    benches,
    setting_without_ttl_benchmark,
    setting_with_ttl_benchmark,
    updating_without_ttl_benchmark,
    updating_with_ttl_benchmark,
    getting_without_ttl_benchmark,
    getting_with_ttl_benchmark,
    deleting_without_ttl_benchmark,
    deleting_with_ttl_benchmark,
    clearing_without_ttl_benchmark,
    clearing_with_ttl_benchmark,
    compacting_benchmark,
);
criterion_main!(benches);
