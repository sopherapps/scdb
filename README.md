# scdb

A very simple and fast key-value store but persisting data to disk, with a "localStorage-like" API.

**scdb is not yet production ready. It is still being heavily developed and its API (and features) could change quite drastically**

## Purpose

Coming from front-end web development, [localStorage](https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage) was always
a convenient way of quickly persisting data to be used later by a given application even after a restart.
Its API was extremely simple i.e. `localStorage.getItem()`, `localStorage.setItem()`, `localStorage.removeItem()`, `localStorage.clear()`.

Coming to the backend (or even desktop) development, such an embedded persistent data store with a simple API
was hard to come by.

scdb is meant to be like the 'localStorage' of backend and desktop (and possibly mobile) systems.
Of course to make it a little more appealing, it has some extra features like:

- Time-to-live (TTL) where a key-value pair expires after a given time (pending implementation)
- Non-blocking reads from separate processes, and threads (pending implementation).
- Fast Sequential writes to the store, queueing any writes from multiple processes and threads (pending implementation).

## Contributing

Contributions are welcome. The docs have to maintained, the code has to be made cleaner, more idiomatic and faster,
and there might be need for someone else to take over this repo in case I move on to other things. It happens!

Please look at the [CONTRIBUTIONS GUIDELINES](./docs/CONTRIBUTING.md)

You can also look in the [./docs](./docs) folder to get up to speed with the internals of scdb e.g.

- [database file format](./docs/DB_FILE_FORMAT.md)

## Bindings

scdb is meant to be used in multiple languages of choice. However, the bindings for most of them are yet to be developed.
Here are those that have been developed:

- [ ] [rust](TODO)
- [ ] [python](TODO)
- [ ] [golang](TODO)
- [ ] [c/c++](TODO)
- [ ] [dotnet (C#, F#)](TODO)
- [ ] [java](TODO)
- [ ] [nodejs](TODO)


### TODO:

- [ ] Add designs
- [ ] Add package documentation
- [ ] Make the key hashing point to offsets on the memory-mapped database file
- [ ] Add benchmarks
- [ ] compare benchmarks with those of redis, sqlite, lmdb etc.

### How to Test

- Make sure you have [rust](https://www.rust-lang.org/tools/install) installed on your computer.

- Clone the repo and enter its root folder

  ```bash
  git clone https://github.com/sopherapps/scdb.git && cd scdb
  ```

- Run the test command

```shell
cargo test
```

- Run the bench test command

```shell
cargo bench
```

## Acknowledgement

- Inspiration was got from [lmdb](https://www.symas.com/lmdb/technical), especially with regards to memory-mapped files.
- A few ideas were picked from [redis](https://redis.io/) and [sqlite](https://www.sqlite.org/index.html) especially to do with the database file format.

## Gratitude

To God.

## License

Copyright (c) 2022 [Martin Ahindura](https://github.com/Tinitto) Licensed under the [MIT License](./LICENSE)

