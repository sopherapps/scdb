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
  'key_values_start_point' (`100 + (number_of_items_per_index_block * 4 * number_of_index_blocks)`),
  'net_block_size' (`number_of_items_per_index_block * 4`)

#### 2. Set

1. The key supplied is run through a hashfunction with modulo `number_of_items_per_index_block`
   and answer multiplied by 4 to get the byte offset. Let the hashed value be `hash`
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 100 + hash`.
4. The 4-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is zero, this means that no value has been set for that key yet.
    - So the key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
      , `value`, `deleted`) is appended to the end of the file at offset `last_offset`
    - the `last_offset` is then inserted at `index_address` in place of the zero
    - the `last_offset` header is then updated
      to `last_offset + get_size_of_kv(kv)` [get_size_of_kv gets the total size of the entry in bits]
6. If this `key_value_offset` is non-zero, it is possible that the value for that key has already been set.
    - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `key_size` before the
      key. That number gives the size of the key).
    - if this key is the same as the key passed, we have to update it by deleting then inserting it again:
        - update the `deleted` of the key-value entry to 1
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

1. The key supplied is run through a hashfunction with modulo `number_of_items_per_index_block`
   and answer multiplied by 4 to get the byte offset. Let the hashed value be `hash`.
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 100 + hash`.
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

1. The key supplied is run through a hashfunction with modulo `number_of_items_per_index_block`
   and answer multiplied by 4 to get the byte offset. Let the hashed value be `hash`.
2. Set `index_block_offset` to zero to start from the first block.
3. The `index_address` is set to `index_block_offset + 100 + hash`.
4. The 4-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `key_value_offset`.
5. If this `key_value_offset` is non-zero, it is possible that the value for that key exists.
    - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `key_size` before the
      key. That number gives the size of the key).
    - if this key is the same as the key passed:
        - if the `deleted` is 1 or `expiry` is greater than the `current_timestamp`, return `None`
        - else return `value` for this key-value entry
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, stop and
          return `None`.
          The key does not exist.
        - else go back to step 3.

##### Performance

- This operation is O(k) where k is the `number_of_index_blocks`.
- About 3 4-byte integers are allocated on the stack.

#### 5. Compact

Compaction can run automatically every few hours. During that time, the database would be locked.
No read, nor write would be allowed. It can also be requested for by the user.

1. Create new file
2. Copy header into the new file
3. Copy index into new file
4. Read index_map into a map of <entry_offset, index_offset>
5. scan key-value entries until offset is greater or equal to `file_size`
    - if key-value offset does not exist in index_map, do nothing
    - else copy that key-value entry to the new file,
        - get the index_offset of that key-value entry from index_map and update new file's index with the new offset
        - get the next offset by adding current offset to key-value entry's size
        - seek to that offset and do the necessary
6. Clear the buffers
7. Update file_size to the new file's file size
8. Point the buffer pool's file to that new file
9. Delete the old file
10. Rename the new file to the old file's name

##### Performance

- This operation is O(kN) where k is the `number_of_index_blocks` and N is the number of keys in the file before
  compaction.
- The worst case in terms of memory allocations is when only the first key-value entry was deleted or is expired,
  all other key-value entries would have to be copied and pasted.