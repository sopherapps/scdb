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
scdb = "0.0.1" # or any available version you wish to use
```

Next:

```rust
# use std::io;

# fn main() -> io::Result<()> {
    // Create the store. You can configure its `max_keys`, `redundant_blocks` etc.
    // The defaults are usable though.
    // One very important config is `max_keys`.
    // With it, you can limit the store size to a number of keys.
    // By default, the limit is 1 million keys
    let mut store = scdb::Store::new("db", // `store_path`
                            Some(1000), // `max_keys`
                            Some(1), // `redundant_blocks`
                            Some(10), // `pool_capacity`
                            Some(1800), // `compaction_interval`
                            Some(3))?; // `max_index_key_len`
    let key = b"foo";
    let value = b"bar";

    // Insert key-value pair into the store with no time-to-live
    store.set(&key[..], &value[..], None)?;
    # assert_eq!(store.get(&key[..])?, Some(value.to_vec()));

    // Or insert it with an optional time-to-live (ttl)
    // It will disappear from the store after `ttl` seconds
    store.set(&key[..], &value[..], Some(1))?;
    # assert_eq!(store.get(&key[..])?, Some(value.to_vec()));

    // Getting the values by passing the key in bytes to store.get
    let value_in_store = store.get(&key[..])?;
    assert_eq!(value_in_store, Some(value.to_vec()));

    // Updating the values is just like inserting them. Any key-value already in the store will
    // be overwritten
    store.set(&key[..], &value[..], None)?;

    // Searching for all keys starting with a given substring is also possible.
    // We can paginate the results.
    // let's skip the first matched item, and return only upto to 2 items
    let results = store.search(&b"f"[..], 1, 2)?;
    # assert_eq!(results, vec![]);

    // Or let's just return all matched items
    let results = store.search(&b"f"[..], 0, 0)?;
    # assert_eq!(results, vec![(key.to_vec(), value.to_vec())]);

    // Delete the key-value pair by supplying the key as an argument to store.delete
    store.delete(&key[..])?;
    assert_eq!(store.get(&key[..])?, None);

    // Deleting all key-value pairs to start afresh, use store.clear()
    # store.set(&key[..], &value[..], None)?;
    store.clear()?;
    # assert_eq!(store.get(&key[..])?, None);

    # Ok(())
# }
```
 */

#![deny(missing_docs)]
#![warn(rust_2018_idioms)]

pub use store::Store;

mod internal;
mod store;
