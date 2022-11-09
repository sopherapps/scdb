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
set(no ttl): 'hey'      time:   [8.9857 µs 9.2586 µs 9.5911 µs]
set(no ttl): 'hi'       time:   [10.125 µs 11.175 µs 12.519 µs]
set(no ttl): 'salut'    time:   [8.8035 µs 9.2403 µs 10.009 µs]
set(no ttl): 'bonjour'  time:   [8.6488 µs 8.7125 µs 8.7822 µs]
set(no ttl): 'hola'     time:   [8.9169 µs 9.0807 µs 9.2678 µs]
set(no ttl): 'oi'       time:   [8.5945 µs 8.6551 µs 8.7410 µs]
set(no ttl): 'mulimuta' time:   [8.7163 µs 8.8850 µs 9.1155 µs]
set(ttl): 'hey'         time:   [8.9122 µs 9.0473 µs 9.1843 µs]
set(ttl): 'hi'          time:   [8.9988 µs 9.6177 µs 10.432 µs]
set(ttl): 'salut'       time:   [9.8412 µs 11.217 µs 12.903 µs]
set(ttl): 'bonjour'     time:   [8.6825 µs 8.7227 µs 8.7664 µs]
set(ttl): 'hola'        time:   [11.105 µs 12.667 µs 14.884 µs]
set(ttl): 'oi'          time:   [9.0882 µs 9.3976 µs 9.7844 µs]
set(ttl): 'mulimuta'    time:   [9.2010 µs 10.046 µs 11.181 µs]
update(no ttl): 'hey'   time:   [9.1358 µs 9.6845 µs 10.339 µs]
update(no ttl): 'hi'    time:   [9.4786 µs 10.937 µs 12.661 µs]
update(no ttl): 'hola'  time:   [8.1280 µs 8.1931 µs 8.2843 µs]
update(no ttl): 'oi'    time:   [9.6300 µs 9.8456 µs 10.037 µs]
update(no ttl): 'mulimuta'
                        time:   [8.0953 µs 8.1210 µs 8.1564 µs]
update(ttl): 'hey'      time:   [8.2181 µs 8.2340 µs 8.2506 µs]
update(ttl): 'hi'       time:   [8.2417 µs 8.2584 µs 8.2754 µs]
update(ttl): 'hola'     time:   [8.2893 µs 8.3120 µs 8.3371 µs]
update(ttl): 'oi'       time:   [8.2483 µs 8.3000 µs 8.3785 µs]
update(ttl): 'mulimuta' time:   [8.2621 µs 8.2829 µs 8.3044 µs]
get(no ttl): 'hey'      time:   [196.00 ns 196.30 ns 196.61 ns]
get(no ttl): 'hi'       time:   [197.20 ns 197.80 ns 198.40 ns]
get(no ttl): 'salut'    time:   [197.79 ns 198.20 ns 198.65 ns]
get(no ttl): 'bonjour'  time:   [198.76 ns 199.20 ns 199.67 ns]
get(no ttl): 'hola'     time:   [197.71 ns 198.17 ns 198.72 ns]
get(no ttl): 'oi'       time:   [197.71 ns 198.15 ns 198.66 ns]
get(no ttl): 'mulimuta' time:   [197.63 ns 198.36 ns 199.25 ns]
get(ttl): 'hey'         time:   [229.13 ns 229.39 ns 229.65 ns]
get(ttl): 'hi'          time:   [251.55 ns 308.49 ns 430.67 ns]
get(ttl): 'salut'       time:   [241.82 ns 250.34 ns 262.40 ns]
get(ttl): 'bonjour'     time:   [235.81 ns 237.32 ns 238.91 ns]
get(ttl): 'hola'        time:   [234.49 ns 235.62 ns 236.79 ns]
get(ttl): 'oi'          time:   [229.32 ns 229.96 ns 230.65 ns]
get(ttl): 'mulimuta'    time:   [233.15 ns 234.07 ns 235.08 ns]
delete(no ttl): 'hey'   time:   [4.8287 µs 4.8387 µs 4.8485 µs]
delete(no ttl): 'hi'    time:   [4.7770 µs 4.7863 µs 4.7962 µs]
delete(no ttl): 'salut' time:   [4.8409 µs 4.8589 µs 4.8751 µs]
delete(no ttl): 'bonjour'
                        time:   [4.9287 µs 4.9621 µs 5.0050 µs]
delete(no ttl): 'hola'  time:   [4.8596 µs 4.8727 µs 4.8870 µs]
delete(no ttl): 'oi'    time:   [4.8873 µs 4.9039 µs 4.9238 µs]
delete(no ttl): 'mulimuta'
                        time:   [4.8946 µs 4.9066 µs 4.9192 µs]
delete(ttl): 'hey'      time:   [4.7234 µs 4.7391 µs 4.7552 µs]
delete(ttl): 'hi'       time:   [4.8509 µs 4.8617 µs 4.8730 µs]
delete(ttl): 'salut'    time:   [4.8528 µs 4.8652 µs 4.8787 µs]
delete(ttl): 'bonjour'  time:   [4.8751 µs 4.8877 µs 4.8999 µs]
delete(ttl): 'hola'     time:   [4.7874 µs 4.8058 µs 4.8279 µs]
delete(ttl): 'oi'       time:   [4.8657 µs 4.8787 µs 4.8929 µs]
delete(ttl): 'mulimuta' time:   [4.8913 µs 4.9017 µs 4.9131 µs]
clear(no ttl)           time:   [133.85 µs 134.66 µs 135.73 µs]
clear(ttl)              time:   [133.16 µs 133.67 µs 134.25 µs]
compact                 time:   [105.99 ms 108.23 ms 112.03 ms]
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

