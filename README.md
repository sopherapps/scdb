# scdb

![CI](https://github.com/sopherapps/scdb/actions/workflows/CI.yml/badge.svg)

A very simple and fast key-value store but persisting data to disk, with a "localStorage-like" API.

**scdb is not yet production ready. It is not even working yet!!. It is still being heavily developed and its API (and
features) could change quite
drastically**

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

## Quick Start

- Create a new cargo project

  ```shell
  cargo new hello_scdb && cd hello_scdb
  ```

- Add scdb to your dependencies in `Cargo.toml` file

  ```toml
  [dependencies]
  scdb = { git = "https://github.com/sopherapps/scdb", branch = "master" }
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

- [ ] [rust](TODO)
- [ ] [python](TODO)
- [ ] [golang](TODO)
- [ ] [c/c++](TODO)
- [ ] [dotnet (C#, F#)](TODO)
- [ ] [java](TODO)
- [ ] [nodejs](TODO)

### TODO:

- [x] Add designs
- [x] Implement basic skeleton
- [x] Add tests for internal::utils
- [x] Add tests for internal::entries::KeyValueEntry
- [x] Add tests for internal::entries::DbFileHeader
- [x] Add tests for internal::buffers::Buffer
- [x] Add tests for internal::buffers::Value
- [x] Add tests for internal::buffers::BufferPool
- [x] Add tests for store::Store
- [x] Add examples
- [x] Add GitHub actions for CI
- [ ] Add GitHub actions for CD
- [ ] Add package documentation
- [x] Add benchmarks
- [ ] compare benchmarks with those of redis, sqlite, lmdb etc.
- [ ] Release version 0.0.1

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
set hey English         time:   [8.5479 µs 8.5804 µs 8.6147 µs]
set hi English          time:   [8.6179 µs 8.6818 µs 8.7717 µs]
set salut French        time:   [8.6286 µs 8.6593 µs 8.6923 µs]
set bonjour French      time:   [8.7125 µs 8.7572 µs 8.8033 µs]
set hola Spanish        time:   [8.5572 µs 8.5922 µs 8.6297 µs]
set oi Portuguese       time:   [8.5180 µs 8.5475 µs 8.5815 µs]
set mulimuta Runyoro    time:   [8.5312 µs 8.5632 µs 8.5987 µs]
update hey to Jane      time:   [8.2037 µs 8.2685 µs 8.3540 µs]
update hi to John       time:   [8.1770 µs 8.2065 µs 8.2387 µs]
update hola to Santos   time:   [8.1840 µs 8.2414 µs 8.3128 µs]
update oi to Ronaldo    time:   [8.1838 µs 8.2097 µs 8.2379 µs]
update mulimuta to Aliguma
                        time:   [8.2152 µs 8.2409 µs 8.2680 µs]
get hey                 time:   [286.88 ns 289.00 ns 291.10 ns]
get hi                  time:   [288.61 ns 291.28 ns 293.95 ns]
get salut               time:   [294.12 ns 296.30 ns 298.26 ns]
get bonjour             time:   [293.12 ns 295.42 ns 297.53 ns]
get hola                time:   [294.76 ns 296.78 ns 298.71 ns]
get oi                  time:   [283.30 ns 285.37 ns 287.49 ns]
get mulimuta            time:   [295.08 ns 298.12 ns 301.30 ns]
delete hey              time:   [4.1615 ms 4.1708 ms 4.1801 ms]
delete hi               time:   [4.2124 ms 4.2224 ms 4.2331 ms]
delete salut            time:   [4.1962 ms 4.2064 ms 4.2173 ms]
delete bonjour          time:   [4.1623 ms 4.1731 ms 4.1848 ms]
delete hola             time:   [4.2146 ms 4.2251 ms 4.2363 ms]
delete oi               time:   [4.2685 ms 4.2898 ms 4.3119 ms]
delete mulimuta         time:   [4.2286 ms 4.2462 ms 4.2641 ms]
clear                   time:   [7.1932 ms 7.2490 ms 7.3065 ms]
```

## Acknowledgement

- Inspiration was got from [lmdb](https://www.symas.com/lmdb/technical), especially in regard to memory-mapped
  files. That is until I ran into issues with memory-mapped files...For more details, look
  at [this paper by Andrew Crotty, Viktor Leis and Andy Pavlo](https://db.cs.cmu.edu/mmap-cidr2022/).
- A few ideas were picked from [redis](https://redis.io/) and [sqlite](https://www.sqlite.org/index.html) especially to
  do with the database file format.

## Gratitude

> "For My Father’s will is that everyone who looks to the Son and believes in Him shall have eternal life, and I will
> raise them up at the last day."
>
> -- John 6: 40

All glory be to God.

## License

Copyright (c) 2022 [Martin Ahindura](https://github.com/Tinitto) Licensed under the [MIT License](./LICENSE)

