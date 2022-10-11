# How scdb Works

This is how scdb works under the hood. Keep in mind that scdb is basically a hashtable with the memory-mapped database
file
acting as the underlying array.

### Operations

There are four main operations

#### 1. Initialization

#### 2. Set

#### 3. Delete

#### 4. Get

## Ideas

- We could use special encoding like [redis' length encoding](https://rdb.fnordig.de/file_format.html#length-encoding)
  to signify that a given position
  has [separate chaining](https://www.educative.io/answers/what-is-chaining-in-hash-tables) to deal with hash
  collisions.
- The file is to be treated as the underlying array on which the hashtable is based, thus it is created
  with a given size. The permissible size of the database can be specified on creation of the store.
- To deal with varying sizes of the values, we have a fixed-size section on top which has slots for each possible key:
    - Each slot can store
        - `SIZE <the 2 byte int number showing number of bits for this pair>`
        - `KEY`
        - `OFFSET` (where the value is to be found)
            - `SIZE <the 2 byte int number showing number of bits for this pair>`
            - `KEY`
            - `OFFSET` etc
- Or we have the above index in a separate file (.idx). This would mean adding more keys does not cause the actual
  values to be shifted in position i.e. shifted down if more keys are added to the index section. Both files can be
  memory mapped.
- The above setup effectively acts as what actually happens in memory hash-tables with strings as they store pointers.
- This makes it even much easier to just append values to the values file (.scdb) and let it just keep increasing in
  size.
- This way, deleting would be a matter of removing the value the index (and leaving a gap), and also leaving a gap in
  the actual database file (maybe vacuuming can be done later)
- Or deleted keys and their sizes and offsets can be kept in a (.del) file and whenever deleting is done,
  the key section is removed from the index and put in the del file. On vacuuming, a lock is put on the whole db,
  then all deleted keys are removed from the db file, and for each key removed, keys in the index with an offset greater
  than that key's offset are decremented by the size of the key removed. This will be a slow process as there will be a
  loop
  for each key deleted. A faster way might require some kind of assured ordered key insertion in the index.
- Or avoid vacuuming altogether and instead on insertion, check the del file for the first key with a size equal or
  slightly bigger
  than the new key's size and replace that (in place) and remove it from the del file. The tradeoff is the file will be
  slightly fragmented.
- Or using the replacement method above is okay but this time when the original key is slightly bigger than the new key,
  don't delete the key from the delete file but rather reduce its size by the size of the new key. The other tradeoff is
  all insertions will be
  slowed. [The vacuuming is better as it happens once, say when a given offset is reached or when the number of deleted keys reaches a given value or when the total size of the deleted keys reaches a given value or percentage]