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
- Optional searching of keys that begin with a given subsequence. This option is turned on when `scdb::new()` is called.

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
          Store::new("db", Some(1000), Some(1), Some(10), Some(1800), true).expect("create store");
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
set(no ttl): 'foo'      time:   [8.4622 µs 9.3052 µs 10.396 µs]
set(ttl): 'foo'         time:   [9.0695 µs 9.2830 µs 9.5413 µs]
set(no ttl) with search: 'foo'
                        time:   [40.573 µs 41.152 µs 41.825 µs]
set(ttl) with search: 'foo'
                        time:   [42.494 µs 43.880 µs 45.353 µs]
update(no ttl): 'foo'   time:   [8.0398 µs 8.1054 µs 8.1814 µs]
update(ttl): 'fenecans' time:   [8.2151 µs 8.3078 µs 8.4137 µs]
update(no ttl) with search: 'foo'
                        time:   [40.757 µs 40.854 µs 40.960 µs]
update(ttl) with search: 'fenecans'
                        time:   [40.901 µs 40.985 µs 41.076 µs]
                        time:   [7.9638 µs 8.0066 µs 8.0609 µs]
get(no ttl): 'hey'      time:   [209.98 ns 213.70 ns 218.01 ns]
get(no ttl): 'hi'       time:   [205.34 ns 207.45 ns 209.70 ns]
get(no ttl): 'salut'    time:   [203.01 ns 204.54 ns 206.45 ns]
get(no ttl): 'bonjour'  time:   [206.43 ns 208.68 ns 210.97 ns]
get(no ttl): 'hola'     time:   [268.69 ns 297.50 ns 334.32 ns]
get(no ttl): 'oi'       time:   [192.04 ns 192.62 ns 193.25 ns]
get(no ttl): 'mulimuta' time:   [202.74 ns 203.14 ns 203.56 ns]
get(with ttl): 'hey'    time:   [230.27 ns 230.65 ns 231.06 ns]
get(with ttl): 'hi'     time:   [229.39 ns 229.89 ns 230.50 ns]
get(with ttl): 'salut'  time:   [231.72 ns 232.10 ns 232.51 ns]
get(with ttl): 'bonjour'
                        time:   [232.30 ns 232.68 ns 233.10 ns]
get(with ttl): 'hola'   time:   [231.98 ns 232.56 ns 233.16 ns]
get(with ttl): 'oi'     time:   [228.74 ns 229.30 ns 229.87 ns]
get(with ttl): 'mulimuta'
                        time:   [237.61 ns 237.94 ns 238.29 ns]
get(no ttl) with search: 'hey'
                        time:   [194.52 ns 194.86 ns 195.25 ns]
get(no ttl) with search: 'hi'
                        time:   [195.36 ns 195.61 ns 195.86 ns]
get(no ttl) with search: 'salut'
                        time:   [198.78 ns 199.01 ns 199.25 ns]
get(no ttl) with search: 'bonjour'
                        time:   [199.74 ns 200.18 ns 200.79 ns]
get(no ttl) with search: 'hola'
                        time:   [199.81 ns 200.20 ns 200.60 ns]
get(no ttl) with search: 'oi'
                        time:   [191.97 ns 192.37 ns 192.80 ns]
get(no ttl) with search: 'mulimuta'
                        time:   [198.39 ns 198.80 ns 199.22 ns]
get(with ttl) without search: 'hey'
                        time:   [232.84 ns 234.11 ns 235.46 ns]
get(with ttl) without search: 'hi'
                        time:   [230.81 ns 231.25 ns 231.76 ns]
get(with ttl) without search: 'salut'
                        time:   [233.56 ns 234.07 ns 234.67 ns]
get(with ttl) without search: 'bonjour'
                        time:   [233.81 ns 234.23 ns 234.67 ns]
get(with ttl) without search: 'hola'
                        time:   [234.02 ns 234.43 ns 234.86 ns]
get(with ttl) without search: 'oi'
                        time:   [228.52 ns 228.84 ns 229.18 ns]
get(with ttl) without search: 'mulimuta'
                        time:   [233.36 ns 233.74 ns 234.15 ns]
search (not paged): 'h' time:   [18.156 µs 18.274 µs 18.429 µs]
search (not paged): 'h' #2
                        time:   [18.093 µs 18.139 µs 18.192 µs]
search (not paged): 's' time:   [8.6507 µs 8.6653 µs 8.6807 µs]
search (not paged): 'b' time:   [8.6318 µs 8.6531 µs 8.6766 µs]
search (not paged): 'h' #3
                        time:   [18.106 µs 18.147 µs 18.188 µs]
search (not paged): 'o' time:   [8.6288 µs 8.6415 µs 8.6557 µs]
search (not paged): 'm' time:   [8.6453 µs 8.6657 µs 8.6873 µs]
search (paged): 'h'     time:   [16.161 µs 16.230 µs 16.319 µs]
search (paged): 'h' #2  time:   [15.949 µs 16.016 µs 16.093 µs]
search (paged): 's'     time:   [6.0744 µs 6.1114 µs 6.1544 µs]
search (paged): 'b'     time:   [6.2516 µs 6.3119 µs 6.3827 µs]
search (paged): 'h' #3  time:   [15.990 µs 16.026 µs 16.063 µs]
search (paged): 'o'     time:   [6.1061 µs 6.1790 µs 6.2617 µs]
search (paged): 'm'     time:   [6.5727 µs 6.6862 µs 6.7921 µs]
delete(no ttl): 'foo'   time:   [51.172 µs 52.554 µs 54.057 µs]
delete(ttl): 'foo'      time:   [53.211 µs 54.964 µs 56.804 µs]
delete(no ttl) with search: 'foo'
                        time:   [70.327 µs 70.698 µs 71.226 µs]
delete(ttl) with search: 'foo'
                        time:   [70.753 µs 71.086 µs 71.520 µs]
clear(no ttl)           time:   [144.05 µs 153.14 µs 170.79 µs]
clear(ttl)              time:   [142.17 µs 142.68 µs 143.23 µs]
clear(no ttl) with search
                        time:   [221.58 µs 223.04 µs 224.52 µs]
clear(ttl) with search  time:   [218.17 µs 226.53 µs 242.62 µs]
compact                 time:   [126.76 ms 128.26 ms 129.86 ms]
compact with search     time:   [128.80 ms 131.45 ms 134.50 ms]
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

