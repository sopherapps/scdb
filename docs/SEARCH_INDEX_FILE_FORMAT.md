# Search Index File Format

This is a description of the file format for the search index

## Features

The search index is an inverted search index, where the keys are the character sets, and the values are the main db file
offsets of the keys that have those character sets.

This index is basically a hashtable with its 'underlying array' as the search index file. It should thus have the
following features:

- All data is saved in bytes
- It should have the following major sections:
    - a 100-byte header to hold metadata for the database
    - a series of consecutive index blocks. They are `round_up(max_keys / (block_size / 8))` where `(block_size / 8)` is
      items in each index block since each item is an 8-byte offset (offset is described below).
    - a series of values where the values for a given key are linked together in a doubly linked cyclic list. The
      doubly-linked cyclic list uses up minimal space;
        - its cyclic nature makes it faster to append new items to the values of a given character set, than if it were
          not cyclic.
        - its linked nature makes it use less space, and avoid having to compact frequently as compared to if it were a
          plain old list.
        - its doubly linked nature may use up more space but makes it faster to delete any value, than if it were just a
          plain old linked list.

- The 100-byte header, similar to [sqlite](https://www.sqlite.org/fileformat.html#the_database_header) contains:

| Offset | Size |                                                                                  Description                                                                                   |
|:------:|:----:|:------------------------------------------------------------------------------------------------------------------------------------------------------------------------------:|
|   0    |  16  |                                                                     The header string: "ScdbIndex v0.001"                                                                      |
|   16   |  4   | `block_size` - the database page size in bytes. Must be a power of two, as got in a similar way to how [page_size crate](https://docs.rs/page_size/latest/page_size/) does it. |
|   20   |  8   |                                        `max_keys` - maximum number of keys (saved as a 4 byte number). Defaults to 1000,000 (1 million)                                        |
|   28   |  2   |                    `redundant_blocks` - number of redundant index blocks to cater for where all index blocks are filled up for a given hash. Defaults to 1.                    |
|   30   |  8   | `max_key_chars` - maximum number of characters (or runes) that each key of the inverted index should have. Defaults to 3                                                       |
|   38   |  62  |                                                                     Reserved for expansion. Must be zero.                                                                      |

- The index blocks each contain offsets where an offset is how far in bits from the start of the file that you will find
  the corresponding pseudo-root node of the doubly linked cyclic list of offsets that correspond to a key of the
  inverted index.
- Each entry in doubly linked list has the following parts all in binary format
    - `SIZE <the 4 byte unsigned integer showing number of bits for this whole entry>`
    - `INDEX KEY SIZE <the 4 byte unsigned integer showing number of bits for the index key>`
    - `INDEX KEY <the inverted index key>`
    - `KEY SIZE <the 4 byte unsigned integer showing number of bits for this key>`
    - `KEY <the database key>`
    - `IS DELETED <the 1-byte unsigned integer showing 1 for deleted, 0 for not>`
    - `IS ROOT <the 1-byte unsigned integer showing 1 for pseudo-root entry, 0 for others>`
    - `EXPIRY <the timestamp>`
    - `NEXT OFFSET <the 8 byte unsigned integer showing the offset in this file of the next entry of this list>`
    - `PREVIOUS OFFSET <the 8 byte unsigned integer showing the offset in this file of the previous entry of this list>`
    - `DB OFFSET <the 8 byte unsigned integer showing the offset of this key within the db file>`

## Acknowledgements

- Ideas are borrowed from [rds file format](https://rdb.fnordig.de/file_format.html)
  , [sqlite file format](https://www.sqlite.org/fileformat.html)
  and [lmdb file format](https://blog.separateconcerns.com/2016-04-03-lmdb-format.html)