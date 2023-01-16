use std::iter::{IntoIterator, Iterator};
use std::string::ToString;

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

use scdb::Store;

const STORE_PATH: &str = "testdb";

// Setting
fn setting_without_search_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), false).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
    let (k, v) = (b"foo".to_vec(), b"bar".to_vec());

    c.bench_function(
        &format!("set(no ttl): '{}'", String::from_utf8(k.clone()).unwrap(),),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k), black_box(&v), black_box(None))),
    );

    c.bench_function(
        &format!("set(ttl): '{}'", String::from_utf8(k.clone()).unwrap(),),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k), black_box(&v), black_box(ttl))),
    );
}

fn setting_with_search_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), true).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
    let (k, v) = (b"foo".to_vec(), b"bar".to_vec());

    c.bench_function(
        &format!(
            "set(no ttl) with search: '{}'",
            String::from_utf8(k.clone()).unwrap(),
        ),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k), black_box(&v), black_box(None))),
    );

    c.bench_function(
        &format!(
            "set(ttl) with search: '{}'",
            String::from_utf8(k.clone()).unwrap(),
        ),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k), black_box(&v), black_box(ttl))),
    );
}

// Updating
fn updating_without_search_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), false).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
    let (k1, v1) = (b"foo".to_vec(), b"bar".to_vec());
    let (k2, v2) = (b"fenecans".to_vec(), b"barracks".to_vec());

    store.set(&k1, &v1, None).expect(&format!("set {:?}", k1));
    c.bench_function(
        &format!(
            "update(no ttl): '{}'",
            String::from_utf8(k1.clone()).unwrap(),
        ),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k2), black_box(&v2), black_box(None))),
    );

    store.set(&k1, &v1, ttl).expect(&format!("set {:?}", k1));
    c.bench_function(
        &format!("update(ttl): '{}'", String::from_utf8(k2.clone()).unwrap(),),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k2), black_box(&v2), black_box(ttl))),
    );
}

fn updating_with_search_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), true).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
    let (k1, v1) = (b"foo".to_vec(), b"bar".to_vec());
    let (k2, v2) = (b"fenecans".to_vec(), b"barracks".to_vec());

    store.set(&k1, &v1, None).expect(&format!("set {:?}", k1));
    c.bench_function(
        &format!(
            "update(no ttl) with search: '{}'",
            String::from_utf8(k1.clone()).unwrap(),
        ),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k2), black_box(&v2), black_box(None))),
    );

    store.set(&k1, &v1, ttl).expect(&format!("set {:?}", k1));
    c.bench_function(
        &format!(
            "update(ttl) with search: '{}'",
            String::from_utf8(k2.clone()).unwrap(),
        ),
        |b| b.iter_with_large_drop(|| store.set(black_box(&k2), black_box(&v2), black_box(ttl))),
    );
}

// Getting
fn getting_without_search_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), false).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
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

fn getting_with_search_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), true).expect("create store");
    store.clear().expect("clear store");
    let ttl = Some(3_600u64);
    let records = get_records();

    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }
    for (k, _) in &records {
        c.bench_function(
            &format!(
                "get(no ttl) with search: '{}'",
                String::from_utf8(k.clone()).unwrap()
            ),
            |b| b.iter_with_large_drop(|| store.get(black_box(k))),
        );
    }

    for (k, v) in &records {
        store.set(k, v, ttl).expect(&format!("set {:?}", k));
    }
    for (k, _) in &records {
        c.bench_function(
            &format!(
                "get(with ttl) without search: '{}'",
                String::from_utf8(k.clone()).unwrap()
            ),
            |b| b.iter_with_large_drop(|| store.get(black_box(k))),
        );
    }
}

// Searching
fn searching_without_pagination_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), true).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records {
        let term = &k[..1];
        c.bench_function(
            &format!(
                "search (not paged): '{}'",
                String::from_utf8(term.to_vec()).unwrap()
            ),
            |b| {
                b.iter_with_large_drop(|| store.search(black_box(term), black_box(0), black_box(0)))
            },
        );
    }
}

fn searching_with_pagination_benchmark(c: &mut Criterion) {
    let mut store = Store::new(STORE_PATH, None, None, None, Some(0), true).expect("create store");
    store.clear().expect("clear store");
    let records = get_records();
    for (k, v) in &records {
        store.set(k, v, None).expect(&format!("set {:?}", k));
    }

    for (k, _) in &records {
        let term = &k[..1];
        c.bench_function(
            &format!(
                "search (paged): '{}'",
                String::from_utf8(term.to_vec()).unwrap()
            ),
            |b| {
                b.iter_with_large_drop(|| store.search(black_box(term), black_box(1), black_box(2)))
            },
        );
    }
}

// Deleting
fn deleting_benchmark(c: &mut Criterion) {
    let ttl = Some(3_600u64);
    let (k, v) = (b"foo".to_vec(), b"bar".to_vec());

    let prep = |ttl: Option<u64>, is_with_search: bool| {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0), is_with_search)
            .expect("create store");

        store.set(&k, &v, ttl).expect(&format!("set {:?}", k));
        store
    };

    c.bench_function(
        &format!(
            "delete(no ttl): '{}'",
            String::from_utf8(k.clone()).unwrap()
        ),
        |b| {
            b.iter_batched(
                || prep(None, false),
                |mut store| store.delete(black_box(&k)),
                BatchSize::PerIteration,
            )
        },
    );

    c.bench_function(
        &format!("delete(ttl): '{}'", String::from_utf8(k.clone()).unwrap()),
        |b| {
            b.iter_batched(
                || prep(ttl, false),
                |mut store| store.delete(black_box(&k)),
                BatchSize::PerIteration,
            )
        },
    );

    c.bench_function(
        &format!(
            "delete(no ttl) with search: '{}'",
            String::from_utf8(k.clone()).unwrap()
        ),
        |b| {
            b.iter_batched(
                || prep(None, true),
                |mut store| store.delete(black_box(&k)),
                BatchSize::PerIteration,
            )
        },
    );

    c.bench_function(
        &format!(
            "delete(ttl) with search: '{}'",
            String::from_utf8(k.clone()).unwrap()
        ),
        |b| {
            b.iter_batched(
                || prep(ttl, true),
                |mut store| store.delete(black_box(&k)),
                BatchSize::PerIteration,
            )
        },
    );
}

// Clearing
fn clearing_benchmark(c: &mut Criterion) {
    let ttl = Some(3_600u64);

    let prep = |ttl: Option<u64>, is_with_search: bool| {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0), is_with_search)
            .expect("create store");
        store.clear().expect("clear store");
        let records = get_records();
        for (k, v) in &records {
            store.set(k, v, ttl).expect(&format!("set {:?}", k));
        }
        store
    };

    c.bench_function("clear(no ttl)", |b| {
        b.iter_batched(
            || prep(None, false),
            |mut store| store.clear(),
            BatchSize::PerIteration,
        )
    });

    c.bench_function("clear(ttl)", |b| {
        b.iter_batched(
            || prep(ttl, false),
            |mut store| store.clear(),
            BatchSize::PerIteration,
        )
    });

    c.bench_function("clear(no ttl) with search", |b| {
        b.iter_batched(
            || prep(None, true),
            |mut store| store.clear(),
            BatchSize::PerIteration,
        )
    });

    c.bench_function("clear(ttl) with search", |b| {
        b.iter_batched(
            || prep(ttl, true),
            |mut store| store.clear(),
            BatchSize::PerIteration,
        )
    });
}

// Compacting
fn compacting_benchmark(c: &mut Criterion) {
    let prep = |is_with_search: bool| {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0), is_with_search)
            .expect("create store");
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
        store
    };

    c.bench_function("compact", |b| {
        b.iter_batched(
            || prep(false),
            |mut store| store.compact(),
            BatchSize::PerIteration,
        )
    });

    c.bench_function("compact with search", |b| {
        b.iter_batched(
            || prep(true),
            |mut store| store.compact(),
            BatchSize::PerIteration,
        )
    });
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

criterion_group!(
    benches,
    setting_without_search_benchmark,
    setting_with_search_benchmark,
    updating_without_search_benchmark,
    updating_with_search_benchmark,
    getting_without_search_benchmark,
    getting_with_search_benchmark,
    searching_without_pagination_benchmark,
    searching_with_pagination_benchmark,
    deleting_benchmark,
    clearing_benchmark,
    compacting_benchmark,
);
criterion_main!(benches);
