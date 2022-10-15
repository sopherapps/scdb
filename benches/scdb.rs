use std::iter::{IntoIterator, Iterator};
use std::string::ToString;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use scdb::Store;

// Setting
fn setting_benchmark(c: &mut Criterion) {
    let mut store = Store::new("testdb", None, None, None).expect("create store");
    let records = get_records();
    for (k, v) in &records {
        c.bench_function(
            &format!(
                "set {} {}",
                String::from_utf8(k.clone()).unwrap(),
                String::from_utf8(v.clone()).unwrap()
            ),
            |b| {
                b.iter(|| {
                    store.set(
                        black_box(&k.clone()),
                        black_box(&v.clone()),
                        black_box(None),
                    )
                })
            },
        );
    }
    store.close();
}

// Updating
fn updating_benchmark(c: &mut Criterion) {
    let mut store = Store::new("testdb", None, None, None).expect("create store");
    let records = get_records();
    let updates = get_updates();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }
    for (k, v) in &updates {
        c.bench_function(
            &format!(
                "update {} to {}",
                String::from_utf8(k.clone()).unwrap(),
                String::from_utf8(v.clone()).unwrap()
            ),
            |b| b.iter(|| store.set(black_box(k), black_box(v), black_box(None))),
        );
    }
    store.close();
}

// Getting
fn getting_benchmark(c: &mut Criterion) {
    let mut store = Store::new("testdb", None, None, None).expect("create store");
    let records = get_records();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }
    for (k, _) in &records {
        c.bench_function(
            &format!("get {}", String::from_utf8(k.clone()).unwrap()),
            |b| b.iter(|| store.get(black_box(k))),
        );
    }
    store.close();
}

// Deleting
fn deleting_benchmark(c: &mut Criterion) {
    let mut store = Store::new("testdb", None, None, None).expect("create store");
    let records = get_records();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records {
        c.bench_function(
            &format!("delete {}", String::from_utf8(k.clone()).unwrap()),
            |b| b.iter(|| store.delete(black_box(k))),
        );
    }
    store.close();
}

// Clearing
fn clearing_benchmark(c: &mut Criterion) {
    let mut store = Store::new("testdb", None, None, None).expect("create store");
    let records = get_records();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    c.bench_function("clear", |b| b.iter(|| store.clear()));
    store.close();
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
    setting_benchmark,
    updating_benchmark,
    getting_benchmark,
    deleting_benchmark,
    clearing_benchmark
);
criterion_main!(benches);
