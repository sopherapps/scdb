# How the Inverted Index Works

This is how the search index for scdb works under the hood. Keep in mind that scdb is basically an inverted index
implemented as a hashmap, backed by a list of doubly-linked cyclic lists of offsets.

## Inverted Index

In the most basic of terms, look at the inverted index as a mapping where the keys are the character sets, and the
values are the main db offsets of the keys that have those character sets.

Note: In actual sense, the values themselves have some important meta information like `expiry`, `is_deleted` etc.

For example:

Consider the following keys: "foo", "fore", "bar", "band", "pig".

Assume that the offsets of these keys in the scdb file are:

"foo": 1, "fore": 2, "bar": 3, "band": 4, "pig": 5

The inverted index would be:

```go
package example
  
_ = map[string][]int{
	"f": [1, 2],
	"b": [3, 4],
	"p": [5],
	"fo": [1, 2],
	"ba": [3, 4],
	"pi": [5],
	"foo": [1],
	"for": [2],
	"bar": [3],
	"ban": [4],
	"pig": [5],
}
```

## Operations

There are five main operations

### 1. Initialization

- This creates the search index file if it does not exist
    - adds the 100-byte header basing on user configuration and default values
    - adds the placeholders for the index blocks, each item pre-initialized with a zero.
- And loads the derived and non-derived properties like 'max_keys', 'max_index_key_len', 'block_size', '
  redundant_blocks',
  'number_of_index_blocks' (`(max_keys / number_of_items_per_index_block).ceil() + redundant_blocks`),
  'number_of_items_per_index_block' (`(block_size / 8).floor()`),
  'values_start_point' (`100 + (net_block_size * number_of_index_blocks)`),
  'net_block_size' (`number_of_items_per_index_block * 8`)

### 2. Add

1. Get the prefix of the key passed, upto `n` characters. At the start `n` = 1.
2. The prefix supplied is run through a hash function with modulo `number_of_items_per_index_block`
   and answer multiplied by 8 to get the byte offset. Let the hashed value be `hash`
3. Set `index_block_offset` to zero to start from the first block.
4. The `index_address` is set to `index_block_offset + 100 + hash`.
5. The 8-byte offset at the `index_address` offset is read. This is the first possible pointer to the value entry. Let's
   call it `root_value_offset`.
6. If this `root_value_offset` is zero, this means that no value has been set for that inverted index key i.e. prefix
   yet.
    - set the value's `is_root` to true
    - set the value's `next_offset` to `last_offset`
    - set the value's `previous_offset` to `last_offset` (i.e. previous offset = next offset = offset of current value)
    - so the value entry (with all its data including `index_key_size`, `expiry` (got from ttl from user), `kv_address`
      , `deleted`) is appended to the end of the file at offset `last_offset`
    - the `last_offset` is then inserted at `index_address` in place of the zero
    - the `last_offset` header is then updated
      to `last_offset + get_size_of_v_entry(v)` [get_size_of_v gets the total size of the entry in bytes]
7. Else if this `root_value_offset` is non-zero, the value for that prefix has already been set.
    - set the `value_offset` of the current value to `root_value_offset`
    - read the value at the `value_offset`. This is the current value.
    - retrieve the `index_key` of this current value.
    - if the `index_key` is the same as the prefix passed:
      - i. If the `db_key` of the current value entry is equal to the key that is to be added:
          - set the new data from the db into the current value entry i.e. new `expiry` and the new `db_offset`.
          - increment `n` by 1
              - if `n` is greater than `max_index_key_len`, stop the iteration and exit.
              - else go back to step 1 ii. Else if the `db_key` is not equal to the key being added
          - if the `next_offset` is equal to the `root_value_offset`, append the new value to the end of this list i.e.
              - Append the value entry (with all its data including `index_key_size`, `expiry` (got from ttl from user)
                `deleted`, `kv_address`) is to the end of the file at offset `last_offset`
              - Set the `next_offset` of the current value entry (not the newly appended one) to `last_offset`
              - Set the `previous_offset` of the newly appended entry to the `value_offset`
              - Set the `next_offset` of the newly appended entry to the `root_value_offset`.
            - the `last_offset` header is then updated to `last_offset + get_size_of_v(v)`
        - else
            - set `value_offset` to `next_offset`.
            - read the value at `value_offset` and this becomes the current value.
            - Go back to step (i) - i.e. check `db_key` against the key passed.
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `values_start_point`, raise
          the `CollisionSaturatedError` error. We have run out of blocks without getting a free slot to add the value
          entry.
        - else go back to step 4.

__Note: this uses a form of [separate chaining](https://www.geeksforgeeks.org/hashing-set-2-separate-chaining/) to
handle hash collisions. Having multiple index blocks is a form of separate chaining__

#### Performance

- Time complexity: This operation is O(km+n) where:
    - k = `number_of_index_blocks`
    - m = `max_index_key_len`
    - n = length of the longest linked list of values accessed.
- Auxiliary Space: This operation is O(km+n) where:
    - n = length of the longest `db_key` in the linked list of values accessed.
    - m = `max_index_key_len`.
    - k = `number_of_index_blocks`. If the hash for the given prefix already has offsets associated with it, each will
      be allocated in memory thus `km`.

#### Caveats

- There is a possibility that when one sets a given `index_key` or prefix, we may find all index blocks for the given
  hash already filled. We thus have to throw a `CollisionSaturatedError` and abort inserting the key. This means that
  the occurrence of such errors will increase in frequency as the number of keys comes closer to the `max_keys` value.
  One possible remedy to this is to add a more redundant index block(s) i.e. increase `redundant_blocks`. Keep in mind
  that this consumes extra disk and memory space.

### 3. Remove

1. Get the prefix of the key passed, upto `n` characters. At the start `n` = 1.
2. The prefix is run through a hash function with modulo `number_of_items_per_index_block`
   and answer multiplied by 8 to get the byte offset. Let the hashed value be `hash`.
3. Set `index_block_offset` to zero to start from the first block.
4. The `index_address` is set to `index_block_offset + 100 + hash`.
5. The 8-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `root_value_offset`.
6. Set the `value_offset` of the current value to `root_value_offset`
7. If this `root_value_offset` is non-zero, it is possible that the value for that key exists.
    - read the value at the `value_offset`. This is the `current_value`.
    - retrieve the `index_key` of this `current_value`.
    - if the `index_key` is the same as the prefix:

        - i. retrieve the `db_key` of this `current_value`.
        - ii. if the `db_key` is the same as the key passed:
            - update the `is_deleted` of the `current_value` to true
            - if `previous_offset` equals `value_offset`:
                - set the `previous_value` to `current_value`
            - else:
                - read the value at `previous_offset` of the `current_value`. Set that `previous_value` to that value.
            - if `next_offset` equals `value_offset`:
                - set `next_value` to `current_value`
            - else if `next_offset` equals `previous_offset`
                - set `next_value` to `previous_value`
            - else:
            - read the value at `next_offset` of the `current_value`. Set that `next_value` to that value
        - update the `next_offset` of the `previous_value` to `next_offset` of the `current_value`.
            - update the `previous_offset` of the `next_value` to `previous_offset` of the `current_value`.
            - if `is_root` is true for `current_value`:
                - set the `is_root` of the `next_value` to true.
                - set the value at `index_address` to `next_offset`
            - if `value_offset` equals `root_value_offset`
                - set the value at `index_address` to 0 i.e. reset it
            - increment `n` by 1
            - if `n` is greater than `max_index_key_len`, exit
            - else go back to step 1

      -iii. else if `db_key` is not equal to the key passed:
        - if `next_offset` equals `root_value_offset`:
        - increment `n` by 1:
        - if `n` is greater than `max_index_key_len`, exit - else go back to step 1 - else:
        - set the `value_offset` to `next_offset`
        - read the value at the `value_offset`. This is the `current_value`. - go back to step (i)
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `values_start_point`, exit.
        - else go back to step 4.

#### Performance

- Time complexity: This operation is O(km+n) where:
    - k = `number_of_index_blocks`
    - m = `max_index_key_len`
    - n = length of the longest linked list of values accessed.
- Auxiliary space: This operation is O(km+n) where:
    - k = `number_of_index_blocks`
    - m = `max_index_key_len`
    - n = length of the longest `db_key` in the linked list of values accessed.

### 4. Search

1. Get the lower of the two values: length of the `search_term` and the `max_index_key_len`. Let it be `n`.
2. Get the prefix i.e. the first `n` characters/runes of the `search_term`
3. The prefix is run through a hash function with modulo `number_of_items_per_index_block`
   and answer multiplied by 8 to get the byte offset. Let the hashed value be `hash`.
4. Set `index_block_offset` to zero to start from the first block.
5. The `index_address` is set to `index_block_offset + 100 + hash`.
6. The 8-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
   Let's call it `root_value_offset`.
7. If this `root_value_offset` is non-zero, it is possible that the value for that prefix exists.
    - retrieve the value at the `root_value_offset`. Let it be `current_value`.
    - retrieve the `index_key` of the `current_value`
    - if `index_key` equals prefix:
        - i. let `value_offset` equal to `root_value_offset`
        - ii. retrieve the `db_key` of the `current_value`
        - iii. if `db_key` contains the `search_term`:
            - add its `kv_address` to the list of `matched_addresses`
        - iv. set the `value_offset` to `next_offset` of the `current_value`
        - v. if the `current_offset` equals `root_value_offset`
          - read the key, values of the `matched_addresses` from the main database file and return them - exit
        - vi. else go back to step (ii).
    - else increment the `index_block_offset` by `net_block_size`
        - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, stop and return an
          empty list. No matches found.
        - else go back to step 5.
8. Else return an empty list. No matches found.

#### Performance

- Time complexity: This operation is O(kn) where:
    - k = `number_of_index_blocks`.
    - n = length of the longest linked list of values accessed.
- Auxiliary space: This operation is O(kn) where:
    - k = `number_of_index_blocks`
    - n = length of the longest `db_key` in the linked list of values accessed.

### 5. Clear

Clear the entire database.

1. Initialize a new header basing on the old settings
2. Write the new header into the file
3. Shrink the file to the size of the header
4. Expand the file to the expected size of file if it had headers and index blocks. This fills any gaps with 0s.

#### Performance

- Time complexity: This operation is O(1)
- Auxiliary space: This operation is O(1).

## Optimizations

### Pagination

- `skip` and `limit` parameters are provided where:
    - `skip` is the number of matching records to skip before starting to return. Default is zero.
    - `limit` is the maximum number of records to return at any one time. Default is zero. When limit is zero, all
      matched records are returned since it would make no sense for someone to search for zero items. 

