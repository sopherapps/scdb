# How scdb Works

This is how scdb works under the hood. Keep in mind that scdb is basically a hashtable with the memory-mapped database
file acting as the underlying array.

### Operations

There are five main operations

#### 1. Initialization

- This creates the database file if it does not exist
    - adds the 100-byte header basing on user configuration and default values
    - adds the placeholders for the index blocks, each item pre-initialized with a zero.
- It then memory maps the entire database file
- And loads the derived and non-derived properties like 'max_keys', 'block_size', 'redundant_blocks', 'last_offset',
  'number_of_index_blocks' (`round_up(max_keys / number_of_items_per_index_block) + redundant_blocks`),
  'number_of_items_per_index_block' (`round_up(block_size / 4)`),
  'key_values_start_point' (`100 + (number_of_items_per_index_block * 4 * 8 * number_of_index_blocks) + 1`),
  'net_block_size' (`number_of_items_per_index_block * 4`)

#### 2. Set

1. The key supplied is run through a hashfunction with modulo `net_block_size`. Let the hashed value be `hash`
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 101 + (4 * hash)`.
4. The 4-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is zero, this means that no value has been set for that key yet.
    - So the key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
      , `value`, `deleted`) is appended to the end of the file at offset `last_offset + 1`
    - the `last_offset + 1` is then inserted at `index_address` in place of the zero
    - the `last_offset` header is then updated to `last_offset + size_of_kv(kv)`
6. If this `key_value_offset` is non-zero, it is possible that the value for that key has already been set.
    - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `key_size` before the
      key. That number gives the size of the key).
    - if this key is the same as the key passed, we have to update it by deleting then inserting it again:
        - update the `deleted` of the key-value entry to 1
        - The key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
          , `value`, `deleted`) is appended to the end of the file at offset `last_offset + 1`
        - the `last_offset + 1` is then inserted at `index_address` in place of the former offset
        - the `last_offset` header is then updated to `last_offset + size_of_kv(kv)`
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, raise
          the `CollisionSaturatedError` error. We have run out of blocks without getting a free slot to add the
          key-value entry.
        - else go back to step 3.

__Note: this uses a form of [separate chaining](https://www.geeksforgeeks.org/hashing-set-2-separate-chaining/) to
handle hash collisions.__

##### Performance

- This operation is O(k) where k is the `number_of_index_blocks`.
- The key-value entry is not expected to be copied as it will be consumed by this operation. About 4 4-byte integers are
  allocated on the stack.

##### Caveats

- There is a possibility that when one sets a given key, we may find all index blocks for the given hash already filled.
  We thus have to throw a `CollisionSaturatedError` and abort inserting the key. This means that the occurrence of such
  errors will increase in frequency as the number of keys comes closer to the `max_keys` value.
  One possible remedy to this is to add a more redundant index block(s) i.e. increase `redundant_blocks`. Keep in mind
  that this consumes extra disk space.

#### 3. Delete

1. The key supplied is run through a hashfunction with modulo `net_block_size`. Let the hashed value be `hash`
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 101 + (4 * hash)`.
4. The 4-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is non-zero, it is possible that the value for that key exists.
    - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `key_size` before the
      key. That number gives the size of the key).
    - if this key is the same as the key passed, we delete it:
        - update the `deleted` of the key-value entry to 1
        - zero is then inserted at `index_address` in place of the former offset
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, stop and return.
          The key does not exist.
        - else go back to step 3.

##### Performance

- This operation is O(k) where k is the `number_of_index_blocks`.
- About 4 4-byte integers are allocated on the stack.

#### 4. Get

#### 5. Compact
