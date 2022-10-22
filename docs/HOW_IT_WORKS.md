# How scdb Works

This is how scdb works under the hood. Keep in mind that scdb is basically a hashtable with the memory-mapped database
file acting as the underlying array.

### Operations

There are six main operations

#### 1. Initialization

- This creates the database file if it does not exist
    - adds the 100-byte header basing on user configuration and default values
    - adds the placeholders for the index blocks, each item pre-initialized with a zero.
- It then memory maps the entire database file
- And loads the derived and non-derived properties like 'max_keys', 'block_size', 'redundant_blocks',
  'number_of_index_blocks' (`(max_keys / number_of_items_per_index_block).ceil() + redundant_blocks`),
  'number_of_items_per_index_block' (`(block_size / 8).floor()`),
  'key_values_start_point' (`100 + (net_block_size * number_of_index_blocks)`),
  'net_block_size' (`number_of_items_per_index_block * 8`)

#### 2. Set

1. The key supplied is run through a hashfunction with modulo `number_of_items_per_index_block`
   and answer multiplied by 8 to get the byte offset. Let the hashed value be `hash`
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 100 + hash`.
4. The 8-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is zero, this means that no value has been set for that key yet.
    - So the key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
      , `value`, `deleted`) is appended to the end of the file at offset `last_offset`
    - the `last_offset` is then inserted at `index_address` in place of the zero
    - the `last_offset` header is then updated
      to `last_offset + get_size_of_kv(kv)` [get_size_of_kv gets the total size of the entry in bits]
6. If this `key_value_offset` is non-zero, it is possible that the value for that key has already been set.
    - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `size` before the
      key. That number gives the size of the key-value entry).
    - if this key is the same as the key passed, we have to update it appending it to the bottom of file and overwriting
      its index value:
        - The key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
          , `value`, `deleted`) is appended to the end of the file at offset `last_offset`
        - the `last_offset` is then inserted at `index_address` in place of the former offset
        - the `last_offset` header is then updated to `last_offset + get_size_of_kv(kv)`
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, raise
          the `CollisionSaturatedError` error. We have run out of blocks without getting a free slot to add the
          key-value entry.
        - else go back to step 3.

__Note: this uses a form of [separate chaining](https://www.geeksforgeeks.org/hashing-set-2-separate-chaining/) to
handle hash collisions. Having multiple index blocks is a form of separate chaining__

##### Performance

- Time complexity: This operation is O(k) where k is the `number_of_index_blocks`.
- Auxiliary Space: This operation is O(kn+m) where n = key length, m = value length and k = `number_of_index_blocks`.
  The key-value entry is copied into a contiguous byte array before insertion
  and if the hash for the given key already has keys associated with it, each will be allocated in memory thus `kn`.

##### Caveats

- There is a possibility that when one sets a given key, we may find all index blocks for the given hash already filled.
  We thus have to throw a `CollisionSaturatedError` and abort inserting the key. This means that the occurrence of such
  errors will increase in frequency as the number of keys comes closer to the `max_keys` value.
  One possible remedy to this is to add a more redundant index block(s) i.e. increase `redundant_blocks`. Keep in mind
  that this consumes extra disk and memory space.

#### 3. Delete

1. The key supplied is run through a hashfunction with modulo `number_of_items_per_index_block`
   and answer multiplied by 8 to get the byte offset. Let the hashed value be `hash`.
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 100 + hash`.
4. The 8-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is non-zero, it is possible that the value for that key exists.
    - retrieve the key at the given `key_value_offset`.
        - if this key is the same as the key passed, we delete it:
            - zero is then inserted at `index_address` in place of the former offset and exit
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, stop and return.
          The key does not exist.
        - else go back to step 3.

##### Performance

- Time complexity: This operation is O(k) where k is the `number_of_index_blocks`.
- Auxiliary space: This operation is O(kn) where n = buffer size (default is virtual memory page size), and k
  = `number_of_index_blocks`. The hash for the key is checked for each index block, until a block is found that contains
  the key.
  If the hash for the given key is not already buffered, we read a new block from file into memory.
  The worst case is when the key is non-existent and there were no buffers already in memory.

#### 4. Get

1. The key supplied is run through a hashfunction with modulo `number_of_items_per_index_block`
   and answer multiplied by 8 to get the byte offset. Let the hashed value be `hash`.
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 100 + hash`.
4. The 8-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is non-zero, it is possible that the value for that key exists.
    - retrieve the key at the given `key_value_offset`.
        - if this key is the same as the key passed:
            - if the `expiry` is greater than the `current_timestamp`, return `None`
            - else return `value` for this value
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, stop and
          return `None`.
          The key does not exist.
        - else go back to step 3.

##### Performance

- Time complexity: This operation is O(k) where k is the `number_of_index_blocks`.
- Auxiliary space: This operation is O(kn) where n = buffer size (default is virtual memory page size), and k
  = `number_of_index_blocks`. The hash for the key is checked for each index block, until a block is found that contains
  the key.
  If the hash for the given key is not already buffered, we read a new block from file into memory.
  The worst case is when the key is non-existent and there were no buffers already in memory.

#### 5. Compact

Compaction can run automatically every few hours. During that time, the database would be locked.
No read, nor write would be allowed. It can also be requested for by the user.

1. Create new file
2. Copy header into the new file
3. Copy index into new file. This done index block by block.
4. In each index block, find any non-zero index entries. For each of these:
    - if the entry has not yet expired
        - append that key-value entry to the new file,
        - update the index for that entry in the new file.
          The index being the offset where it was appended (i.e. bottom of file)
    - else
        - update the index for that entry in the new file to be 0 (zero)
5. Clear the buffers
6. Update file_size to the new file's file size
7. Point the buffer pool's file to that new file
8. Delete the old file
9. Rename the new file to the old file's name

##### Performance

- Time complexity: This operation is O(N) where N is the number of keys in the
  file before
  compaction.
- Auxiliary space: This operation is O(2m) where m is the `file_size` of the original file because we copy one file to
  another.
  The worst case is when there is no key that has been deleted or expired.

#### 6. Clear

Clear the entire database.

1. Create new file
2. Copy header into the new file and reset file_size
3. Add an empty index to the file
4. Clear the buffers
5. Delete the old file
6. Rename the new file to the old file's name

##### Performance

- Time complexity: This operation is O(1) where k is the `number_of_index_blocks` and N is the number of keys in the
  file before
  compaction.
- Auxiliary space: This operation is O(km) where k is the `number_of_index_blocks` and m is the `block_size`.

### Optimizations

- #### Index Cache Misses
  The BufferPool's buffers can be split up into two kinds; index buffers and key-value buffers.
  This is to deal with the multiple cache misses we keep having (very evident in the delete-benchmarks).
  We need to have at least the index first block always cached. This has the following consequences.
    - The index buffers are to be stored in a btree map so as to be able to expel any buffers that have a higher
      left-offset first
    - The total capacity of the buffer pool will be split up at a 2:3 ratio for index:key-value buffers.
    - The index buffer capacity will be capped to the number of index blocks such that
      if the 2:3 ratio produces an index buffer capacity higher than the total number of index blocks, we increase the
      key-value buffer capacity by the difference between `total number of index blocks` and the
      computed `index buffer capacity`. This avoids waste.
    - methods like `read_at` which could use either the index or the key-value buffers will specify what buffer type
      they are interested in i.e. index or key-value.

- #### Index Memory Hoarding
  The index can take up a lot of space in memory - so much so that updating it has to be done incrementally directly
  on disk.
  We need to use different lengths of byte arrays depending on the entry offset they hold. This is hard considering
  the fact that hashing requires that the number of elements per block be the same always.
    - To be investigated further
