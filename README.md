# scdb

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

- Time-to-live (TTL) where a key-value pair expires after a given time (pending implementation)
- Non-blocking reads from separate processes, and threads (pending implementation).
- Fast Sequential writes to the store, queueing any writes from multiple processes and threads (pending implementation).

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
- [ ] Add tests for internal::buffers::Buffer
- [x] Add tests for internal::buffers::Value
- [ ] Add tests for internal::buffers::BufferPool
- [ ] Add tests for store::Store
- [ ] Add GitHub actions for CI/CD
- [ ] Add package documentation
- [ ] Add benchmarks
- [ ] compare benchmarks with those of redis, sqlite, lmdb etc.
- [ ] Release version 0.0.1

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

