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
- [ ] [golang](TODO)
- [ ] [c/c++](TODO)
- [ ] [dotnet (C#, F#)](TODO)
- [ ] [java](TODO)
- [ ] [nodejs](TODO)

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
set hey English         time:   [8.6107 µs 8.6794 µs 8.7686 µs]
set hi English          time:   [8.5486 µs 8.5809 µs 8.6178 µs]
set salut French        time:   [8.5052 µs 8.5372 µs 8.5775 µs]
set bonjour French      time:   [8.4878 µs 8.5861 µs 8.7528 µs]
set hola Spanish        time:   [8.4698 µs 8.5153 µs 8.5816 µs]
set oi Portuguese       time:   [8.4213 µs 8.5188 µs 8.6698 µs]
set mulimuta Runyoro    time:   [9.8037 µs 10.531 µs 11.391 µs]
update hey to Jane      time:   [8.2164 µs 8.3077 µs 8.4745 µs]
update hi to John       time:   [8.1780 µs 8.1968 µs 8.2151 µs]
update hola to Santos   time:   [8.1904 µs 8.2284 µs 8.2838 µs]
update oi to Ronaldo    time:   [8.2113 µs 8.2628 µs 8.3304 µs]
update mulimuta to Aliguma
                        time:   [8.3493 µs 8.9661 µs 9.7859 µs]
get hey                 time:   [283.86 ns 290.11 ns 297.48 ns]
get hi                  time:   [284.43 ns 290.13 ns 299.30 ns]
get salut               time:   [261.23 ns 262.21 ns 263.24 ns]
get bonjour             time:   [259.94 ns 261.17 ns 262.40 ns]
get hola                time:   [285.77 ns 296.09 ns 307.81 ns]
get oi                  time:   [276.00 ns 288.82 ns 304.71 ns]
get mulimuta            time:   [264.65 ns 267.76 ns 270.81 ns]
delete hey              time:   [5.2272 µs 5.3069 µs 5.4095 µs]
delete hi               time:   [5.5279 µs 5.6024 µs 5.7390 µs]
delete salut            time:   [5.4667 µs 5.4839 µs 5.5024 µs]
delete bonjour          time:   [5.7407 µs 6.0535 µs 6.4707 µs]
delete hola             time:   [5.8874 µs 6.1807 µs 6.5191 µs]
delete oi               time:   [6.3242 µs 6.6555 µs 7.0835 µs]
delete mulimuta         time:   [5.7596 µs 5.9081 µs 6.1716 µs]
clear                   time:   [109.92 µs 111.21 µs 112.53 µs]
compact                 time:   [28.984 ms 29.675 ms 30.401 ms]
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

