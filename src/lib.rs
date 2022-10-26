/*!
A very simple and fast key-value store but persisting data to disk, with a "localStorage-like" API.

In front-end web development, [localStorage](https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage)
provides a convenient way to quickly persist data to be used later by a given application even after a restart.
Its API is extremely simple i.e.

- `localStorage.getItem()`
- `localStorage.setItem()`
- `localStorage.removeItem()`
- `localStorage.clear()`

Such an embedded persistent data store with a simple API is hard to come by in backend (or even desktop) development, until now.

scdb is meant to be like the 'localStorage' of backend and desktop (and possibly mobile) systems.
Of course to make it a little more appealing, it has some extra features like:

- Time-to-live (TTL) where a key-value pair expires after a given time.
  Useful when used as a cache.
- Non-blocking reads from separate processes, and threads.
  Useful in multithreaded applications
- Fast Sequential writes to the store, queueing any writes from multiple processes and threads.
  Useful in multithreaded applications

# Usage

First add `scdb` to your dependencies in your project's `Cargo.toml`.

```toml
[dependencies]
regex = "0.0.1" # or any available version you wish to use
```

Next:

```rust
# use std::io;

# fn main() -> io::Result<()> {
    // Creat the store. You can configure its `max_keys`, `redundant_blocks` etc. The defaults are usable though.
    // One very important config is `max_keys`. With it, you can limit the store size to a number of keys.
    // By default, the limit is 1 million keys
    let mut store =
        scdb::Store::new("db", Some(1000), Some(1), Some(10), Some(1800)).expect("create store");
    let key = b"foo";
    let value = b"bar";

    // Insert key-value pair into the store with no time-to-live
    store.set(&key, &value, None)?;

    // Or insert it with an optional time-to-live (ttl)
    // It will disappear from the store after `ttl` seconds
    store.set(&key, &value, Some(1))?;

    // Getting the values by passing the key in bytes to store.get
    let value_in_store: Option<Vec<u8>> = store.get(&key)?;

    // Updating the values is just like inserting them. Any key-value already in the store will
    // be overwritten
    store.set(&key, &value, None)?;

    // Delete the key-value pair by supplying the key as an argument to store.delete
    store.delete(&key)?;

    // Deleting all key-value pairs to start afresh, use store.clear()
    store.clear()?;

    # Ok(())
# }
```

 */

#![deny(missing_docs)]
#![warn(rust_2018_idioms)]

pub use store::Store;

mod internal;
mod store;
