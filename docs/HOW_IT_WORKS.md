# How scdb Works

This is how scdb works under the hood. Keep in mind that scdb is basically a hashtable with the memory-mapped database
file acting as the underlying array.

### Operations

There are four main operations

#### 1. Initialization

- This creates the database file if it does not exist
  - adds the 100-byte header basing on user configuration and default values
  - adds the placeholders for the index blocks, each item pre-initialized with a zero.
- It then memory maps the entire database file
- And loads the derived and non-derived properties like 'max_keys', 'block_size', 'redundant_blocks',
  'number_of_index_blocks' (`round_up(max_keys / number_of_items_per_index_block) + redundant_blocks`),
  'number_of_items_per_index_block' (`round_up(block_size / 4)`),
  'key_values_start_point' (`100 + (number_of_items_per_index_block * 4 * 8 * number_of_index_blocks) + 1`),
  'net_block_size' (`number_of_items_per_index_block * 4`)

#### 2. Set

- The key supplied is run through a hashfunction with modulo `net_block_size`. Let the hashed value be `hash`
- The 4-byte offset at offset `101 + (4 * hash)` is read. This is the first possible pointer to the key-value entry.
  Let's call it `key_value_offset`.
- If this `key_value_offset` is zero, this means that no value has been set for that key yet.
  - the length of the current file is got. After adding 1 to it, we get the `expected_offset` of the new key-value
    entry
  - So the key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
    , `value`, `deleted`) are appended to the end of the file
  - the `expected_offset` is then inserted at `101 + (4 * hash)` in place of the zero
- If this `key_value_offset` is non-zero, it is possible that the value for that key has already been set.
  - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `key_size` before the key.
    That number gives the size of the key).
  - if this key is the same as the key passed:
    - update this key-value entry (i.e. the `expiry`, `value_size`, `value`, `deleted`)
    - [TODO] - need to deal with possibility of overwriting the entries after it if the value is bigger than the
      previous.
    - ...continue
  - else ...

##### Caveats

- There is a possibility that when one sets a given key, we may find all index blocks for the given hash already filled.
  We thus have to throw a `CollisionSaturatedError` and abort inserting the key. This means that the occurrence of such
  errors will increase in frequency as the number of keys comes closer to the `max_keys` value.
  One possible remedy to this is to add a more redundant index block(s) i.e. increase `redundant_blocks`. Keep in mind
  that this consumes extra space.
-

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