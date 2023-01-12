# scdb

![CI](https://github.com/sopherapps/scdb/actions/workflows/CI.yml/badge.svg)

A very simple and fast key-value store but persisting data to disk, with a "localStorage-like" API.

**scdb may not be production-ready yet. It works, quite well but it requires more rigorous testing.**

## Purpose

Coming from front-end web
development, [localStorage](https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage) was always
a convenient way of quickly persisting data to be used later by a given application even after a restart.
Its API was extremely simple i.e. `localStorage.getItem()`, `localStorage.setItem()`, `localStorage.removeItem()`
, `localStorage.clear()`.

Coming to the backend (or even desktop) development, such an embedded persistent data store with a simple API
was hard to come by.

scdb is meant to be like the 'localStorage' of backend and desktop (and possibly mobile) systems.
Of course to make it a little more appealing, it has some extra features like:

- Time-to-live (TTL) where a key-value pair expires after a given time
- Non-blocking reads from separate processes, and threads.
- Fast Sequential writes to the store, queueing any writes from multiple processes and threads.

## Documentation

Find the following documentation sites, depending on the programming language.

- [rust scdb docs](https://docs.rs/scdb)
- [python scdb docs](https://github.com/sopherapps/py_scdb)

## Quick Start

- Create a new cargo project

  ```shell
  cargo new hello_scdb && cd hello_scdb
  ```

- Add scdb to your dependencies in `Cargo.toml` file

  ```toml
  [dependencies]
  scdb = { version = "0.1" }
  ```

- Update your `src/main.rs` to the following.

```rust
use scdb::Store;
use std::thread;
use std::time::Duration;

/// Converts a byte array to string
macro_rules! to_str {
    ($arr:expr) => {
        std::str::from_utf8($arr).expect("bytes to str")
    };
}

/// Prints data from store to the screen in a pretty way
macro_rules! pprint_data {
    ($title:expr, $data:expr) => {
        println!("\n");
        println!("{}", $title);
        println!("===============");

        for (k, got) in $data {
            let got_str = match got {
                None => "None",
                Some(v) => to_str!(v),
            };
            println!("For key: '{}', str: '{}', raw: '{:?}',", k, got_str, got);
        }
    };
}

fn main() {
  // Creat the store. You can configure its `max_keys`, `redundant_blocks` etc. The defaults are usable though.
  // One very important config is `max_keys`. With it, you can limit the store size to a number of keys.
  // By default, the limit is 1 million keys
  let mut store =
          Store::new("db", Some(1000), Some(1), Some(10), Some(1800), Some(3)).expect("create store");
  let records = [
    ("hey", "English"),
    ("hi", "English"),
    ("salut", "French"),
    ("bonjour", "French"),
    ("hola", "Spanish"),
    ("oi", "Portuguese"),
    ("mulimuta", "Runyoro"),
  ];
  let updates = [
    ("hey", "Jane"),
    ("hi", "John"),
    ("hola", "Santos"),
    ("oi", "Ronaldo"),
    ("mulimuta", "Aliguma"),
  ];
  let keys: Vec<&str> = records.iter().map(|(k, _)| *k).collect();

  // Setting the values
  println!("Let's insert data\n{:?}]...", &records);
  for (k, v) in &records {
    let _ = store.set(k.as_bytes(), v.as_bytes(), None);
  }

  // Getting the values (this is similar to what is in `get_all(&mut store, &keys)` function
  let data: Vec<(&str, Option<Vec<u8>>)> = keys
          .iter()
          .map(|k| (*k, store.get(k.as_bytes()).expect(&format!("get {}", k))))
          .collect();
  pprint_data!("After inserting data", &data);

  // Setting the values with time-to-live
  println!(
    "\n\nLet's insert data with 1 second time-to-live (ttl) for keys {:?}]...",
    &keys[3..]
  );
  for (k, v) in &records[3..] {
    let _ = store.set(k.as_bytes(), v.as_bytes(), Some(1));
  }

  println!("We will wait for 1 second to elapse...");
  thread::sleep(Duration::from_secs(2));

  let data = get_all(&mut store, &keys);
  pprint_data!("After inserting keys with ttl", &data);

  // Updating the values
  println!("\n\nLet's update with data {:?}]...", &updates);
  for (k, v) in &updates {
    let _ = store.set(k.as_bytes(), v.as_bytes(), None);
  }

  let data = get_all(&mut store, &keys);
  pprint_data!("After updating keys", &data);

  // Full-text search by key. It returns array of key-value tuples.
  let data = store
          .search(&b"h"[..], 0, 0)
          .expect("search for keys starting with h");
  println!("\nSearching for keys starting with 'h'");
  println!("=======================================", );
  for (k, v) in &data {
    // note that to_str! is a custom macro changing byte array to UTF-8 string
    println!("{}: {}", to_str!(k), to_str!(v))
  }

  // Search with pagination
  let data = store
          .search(&b"h"[..], 1, 1)
          .expect("search for keys starting with h");
  println!("\nPaginated search for keys starting with 'h'");
  println!("==============================================", );
  println!("Skipping 1, returning 1 record only");
  println!("---");
  for (k, v) in &data {
    // note that to_str! is a custom macro changing byte array to UTF-8 string
    println!("{}: {}", to_str!(k), to_str!(v))
  }

  // Deleting some values
  let keys_to_delete = ["oi", "hi"];
  println!("\n\nLet's delete keys{:?}]...", &keys_to_delete);
  for k in keys_to_delete {
    store
            .delete(k.as_bytes())
            .expect(&format!("delete key {}", k));
  }

  let data = get_all(&mut store, &keys);
  pprint_data!("After deleting keys", &data);

  // Deleting all values
  println!("\n\nClear all data...");
  store.clear().expect("clear store");

  let data = get_all(&mut store, &keys);
  pprint_data!("After clearing", &data);
}

/// Gets all from store for the given keys
fn get_all<'a>(store: &mut Store, keys: &Vec<&'a str>) -> Vec<(&'a str, Option<Vec<u8>>)> {
  keys.iter()
          .map(|k| (*k, store.get(k.as_bytes()).expect(&format!("get {}", k))))
          .collect()
}
```

- Run the `main.rs` file

  ```shell
  cargo run
  ```

## Contributing

Contributions are welcome. The docs have to maintained, the code has to be made cleaner, more idiomatic and faster,
and there might be need for someone else to take over this repo in case I move on to other things. It happens!

Please look at the [CONTRIBUTIONS GUIDELINES](./docs/CONTRIBUTING.md)

You can also look in the [./docs](./docs) folder to get up to speed with the internals of scdb e.g.

- [database file format](./docs/DB_FILE_FORMAT.md)
- [how it works](./docs/HOW_IT_WORKS.md)
- [inverted index file format](./docs/INVERTED_INDEX_FILE_FORMAT.md)
- [how the search works](./docs/HOW_INVERTED_INDEX_WORKS.md)

## Bindings

scdb is meant to be used in multiple languages of choice. However, the bindings for most of them are yet to be
developed.
Here are those that have been developed:

- [x] [rust](https://crates.io/crates/scdb)
- [x] [python](https://github.com/sopherapps/py_scdb)
- [x] [golang](https://github.com/sopherapps/go-scdb)

### TODO:

- [ ] compare benchmarks with those of redis, sqlite, lmdb etc.

### How to Test

- Make sure you have [rust](https://www.rust-lang.org/tools/install) installed on your computer.

- Clone the repo and enter its root folder

  ```bash
  git clone https://github.com/sopherapps/scdb.git && cd scdb
  ```

- Run the example

  ```shell
  cargo run --example hello_scdb
  ```

- Lint

  ```shell
  cargo clippy
  ```

- Run the test command

  ```shell
  cargo test
  ```

- Run the bench test command

  ```shell
  cargo bench
  ```

## Benchmarks

On an average PC (i7Core, 16GB RAM):

``` 
set(no ttl): 'hey'      time:   [38.292 µs 38.395 µs 38.514 µs]
set(no ttl): 'hi'       time:   [28.639 µs 28.694 µs 28.755 µs]
set(no ttl): 'salut'    time:   [38.959 µs 39.052 µs 39.153 µs]
set(no ttl): 'bonjour'  time:   [38.394 µs 38.485 µs 38.582 µs]
set(no ttl): 'hola'     time:   [38.514 µs 38.612 µs 38.722 µs]
set(no ttl): 'oi'       time:   [28.407 µs 28.473 µs 28.541 µs]
set(no ttl): 'mulimuta' time:   [38.179 µs 38.259 µs 38.345 µs]
set(ttl): 'hey'         time:   [38.300 µs 38.349 µs 38.400 µs]
set(ttl): 'hi'          time:   [28.439 µs 28.561 µs 28.748 µs]
set(ttl): 'salut'       time:   [38.395 µs 38.463 µs 38.535 µs]
set(ttl): 'bonjour'     time:   [38.378 µs 38.452 µs 38.547 µs]
set(ttl): 'hola'        time:   [38.211 µs 38.317 µs 38.448 µs]
set(ttl): 'oi'          time:   [28.488 µs 28.743 µs 29.127 µs]
set(ttl): 'mulimuta'    time:   [38.359 µs 38.491 µs 38.635 µs]
update(no ttl): 'hey'   time:   [38.065 µs 38.160 µs 38.258 µs]
update(no ttl): 'hi'    time:   [28.550 µs 28.677 µs 28.875 µs]
update(no ttl): 'hola'  time:   [38.634 µs 38.716 µs 38.801 µs]
update(no ttl): 'oi'    time:   [28.353 µs 28.411 µs 28.475 µs]
update(no ttl): 'mulimuta'
                        time:   [38.447 µs 38.534 µs 38.624 µs]
update(ttl): 'hey'      time:   [38.565 µs 38.637 µs 38.713 µs]
update(ttl): 'hi'       time:   [28.659 µs 28.725 µs 28.796 µs]
update(ttl): 'hola'     time:   [38.550 µs 38.620 µs 38.697 µs]
update(ttl): 'oi'       time:   [28.647 µs 28.707 µs 28.772 µs]
update(ttl): 'mulimuta' time:   [38.481 µs 38.554 µs 38.633 µs]
get(no ttl): 'hey'      time:   [197.75 ns 198.13 ns 198.54 ns]
get(no ttl): 'hi'       time:   [197.95 ns 198.32 ns 198.72 ns]
get(no ttl): 'salut'    time:   [198.65 ns 199.10 ns 199.56 ns]
get(no ttl): 'bonjour'  time:   [198.83 ns 199.23 ns 199.63 ns]
get(no ttl): 'hola'     time:   [200.66 ns 201.69 ns 202.76 ns]
get(no ttl): 'oi'       time:   [197.60 ns 198.17 ns 198.82 ns]
get(no ttl): 'mulimuta' time:   [200.80 ns 201.33 ns 201.99 ns]
get(with ttl): 'hey'    time:   [235.18 ns 236.47 ns 237.75 ns]
get(with ttl): 'hi'     time:   [232.61 ns 233.05 ns 233.55 ns]
get(with ttl): 'salut'  time:   [233.03 ns 233.45 ns 233.87 ns]
get(with ttl): 'bonjour'
                        time:   [235.68 ns 236.10 ns 236.59 ns]
get(with ttl): 'hola'   time:   [234.36 ns 234.71 ns 235.08 ns]
get(with ttl): 'oi'     time:   [240.90 ns 243.92 ns 247.12 ns]

get(with ttl): 'mulimuta'
                        time:   [239.99 ns 242.52 ns 245.61 ns]
search (not paged): 'h' time:   [17.980 µs 18.016 µs 18.055 µs]
search (not paged): 'h' #2
                        time:   [17.999 µs 18.033 µs 18.068 µs]
search (not paged): 's' time:   [8.5859 µs 8.6066 µs 8.6337 µs]
search (not paged): 'b' time:   [8.6193 µs 8.6350 µs 8.6505 µs]
search (not paged): 'h' #3
                        time:   [18.073 µs 18.160 µs 18.277 µs]
search (not paged): 'o' time:   [8.6134 µs 8.6296 µs 8.6474 µs]
search (not paged): 'm' time:   [8.5704 µs 8.5795 µs 8.5900 µs]
search (paged): 'h'     time:   [15.530 µs 15.562 µs 15.594 µs]
search (paged): 'h' #2  time:   [15.530 µs 15.556 µs 15.581 µs]
search (paged): 's'     time:   [5.9301 µs 5.9415 µs 5.9539 µs]
search (paged): 'b'     time:   [5.9116 µs 5.9221 µs 5.9326 µs]
search (paged): 'h' #3  time:   [15.522 µs 15.550 µs 15.582 µs]
search (paged): 'o'     time:   [5.9214 µs 5.9295 µs 5.9388 µs]
search (paged): 'm'     time:   [5.8985 µs 5.9100 µs 5.9220 µs]
delete(no ttl): 'hey'   time:   [23.371 µs 23.596 µs 23.802 µs]
delete(no ttl): 'hi'    time:   [20.279 µs 20.369 µs 20.497 µs]
delete(no ttl): 'salut' time:   [18.847 µs 18.921 µs 19.000 µs]
delete(no ttl): 'bonjour'
                        time:   [18.699 µs 18.753 µs 18.810 µs]
delete(no ttl): 'hola'  time:   [23.393 µs 24.115 µs 24.940 µs]
delete(no ttl): 'oi'    time:   [19.467 µs 19.723 µs 20.021 µs]
delete(no ttl): 'mulimuta'
                        time:   [21.095 µs 22.061 µs 23.245 µs]
delete(ttl): 'hey'      time:   [26.199 µs 27.429 µs 28.786 µs]
delete(ttl): 'hi'       time:   [19.873 µs 20.014 µs 20.201 µs]
delete(ttl): 'salut'    time:   [22.094 µs 23.212 µs 24.573 µs]
delete(ttl): 'bonjour'  time:   [20.922 µs 21.346 µs 21.801 µs]
delete(ttl): 'hola'     time:   [21.242 µs 21.824 µs 22.441 µs]
delete(ttl): 'oi'       time:   [20.427 µs 20.970 µs 21.692 µs]
delete(ttl): 'mulimuta' time:   [19.051 µs 19.591 µs 20.289 µs]
clear(no ttl)           time:   [227.61 µs 231.32 µs 235.62 µs]
clear(ttl)              time:   [227.36 µs 235.30 µs 249.33 µs]
compact                 time:   [107.47 ms 108.66 ms 109.94 ms]
```

## Acknowledgement

- Inspiration was got from [lmdb](https://www.symas.com/lmdb/technical), especially in regard to memory-mapped
  files. That is until I ran into issues with memory-mapped files...For more details, look
  at [this paper by Andrew Crotty, Viktor Leis and Andy Pavlo](https://db.cs.cmu.edu/mmap-cidr2022/).
- A few ideas were picked from [redis](https://redis.io/) and [sqlite](https://www.sqlite.org/index.html) especially to
  do with the database file format.

## License

Copyright (c) 2022 [Martin Ahindura](https://github.com/Tinitto) Licensed under the [MIT License](./LICENSE)

## Gratitude

> "For My Father’s will is that everyone who looks to the Son and believes in Him shall have eternal life, and I will
> raise them up at the last day."
>
> -- John 6: 40

All glory be to God.

<a href="https://www.buymeacoffee.com/martinahinJ" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>

