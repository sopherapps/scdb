# scdb

![CI](https://github.com/sopherapps/scdb/actions/workflows/CI.yml/badge.svg)

A very simple and fast key-value store but persisting data to disk, with a "localStorage-like" API.

**scdb may not be production-ready yet. It works, quite well but it requires more vigorous testing.**

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
  scdb = { version = "0.0.1" }
  ```

- Update your `src/main.rs` to the following.

  ```rust
  use scdb::Store;
  use std::thread;
  use std::time::Duration;
  
  /// Prints data from store to the screen in a pretty way
  macro_rules! pprint_data {
      ($title:expr, $data:expr) => {
          println!("\n");
          println!("{}", $title);
          println!("===============");
  
          for (k, got) in $data {
              let got_str = match got {
                  None => "None",
                  Some(v) => std::str::from_utf8(v).expect("bytes to str"),
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
          Store::new("db", Some(1000), Some(1), Some(10), Some(1800)).expect("create store");
      let records = [
          ("hey", "English"),
          ("hi", "English"),
          ("salut", "French"),
          ("bonjour", "French"),
          ("hola", "Spanish"),
          ("oi", "Portuguese"),
          ("mulimuta", "Runyoro"),
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
      let updates = [
          ("hey", "Jane"),
          ("hi", "John"),
          ("hola", "Santos"),
          ("oi", "Ronaldo"),
          ("mulimuta", "Aliguma"),
      ];
      println!("\n\nLet's update with data {:?}]...", &updates);
      for (k, v) in &updates {
          let _ = store.set(k.as_bytes(), v.as_bytes(), None);
      }
  
      let data = get_all(&mut store, &keys);
      pprint_data!("After updating keys", &data);
  
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

On an average PC.

``` 
set hey English         time:   [8.7457 µs 8.8701 µs 9.0315 µs]
set hi English          time:   [8.6830 µs 8.8013 µs 9.0218 µs]
set salut French        time:   [8.7091 µs 8.7483 µs 8.8028 µs]
set bonjour French      time:   [9.1417 µs 9.4258 µs 9.8364 µs]
set hola Spanish        time:   [8.7417 µs 8.8042 µs 8.8926 µs]
set oi Portuguese       time:   [8.7821 µs 8.8097 µs 8.8416 µs]
set mulimuta Runyoro    time:   [8.8020 µs 8.8354 µs 8.8731 µs]
update hey to Jane      time:   [8.6363 µs 8.8054 µs 8.9918 µs]
update hi to John       time:   [8.2494 µs 8.3593 µs 8.4831 µs]
update hola to Santos   time:   [10.732 µs 11.558 µs 12.560 µs]
update oi to Ronaldo    time:   [8.3271 µs 8.6427 µs 9.0522 µs]
update mulimuta to Aliguma
                        time:   [8.3180 µs 8.4352 µs 8.5829 µs]
get hey                 time:   [256.84 ns 259.07 ns 261.38 ns]
get hi                  time:   [261.81 ns 263.20 ns 264.66 ns]
get salut               time:   [275.60 ns 279.42 ns 283.53 ns]
get bonjour             time:   [263.62 ns 265.23 ns 267.19 ns]
get hola                time:   [265.41 ns 266.07 ns 266.74 ns]
get oi                  time:   [257.25 ns 258.27 ns 259.40 ns]
get mulimuta            time:   [264.03 ns 265.17 ns 266.38 ns]
delete hey              time:   [4.8796 µs 4.8911 µs 4.9027 µs]
delete hi               time:   [5.0132 µs 5.0299 µs 5.0470 µs]
delete salut            time:   [4.9815 µs 5.0167 µs 5.0676 µs]
delete bonjour          time:   [4.9342 µs 4.9455 µs 4.9575 µs]
delete hola             time:   [4.9727 µs 4.9840 µs 4.9952 µs]
delete oi               time:   [4.9626 µs 4.9735 µs 4.9848 µs]
delete mulimuta         time:   [4.9687 µs 4.9794 µs 4.9907 µs]
clear                   time:   [99.532 µs 99.964 µs 100.48 µs]
compact                 time:   [108.68 ms 109.95 ms 111.27 ms]
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

