# Database File Format

This is a description of the file format.

## Features

- It should be able to have its sections mapped to memory. Each section should be of OS page size.
- It should have a 100 bit header
- Each key-value pair has the following parts all in binary format (hex probably)
    - `SIZE <the 2 byte int number showing number of bits for this pair>`
    - `KEY <the key>`
    - `VALUE <the value in binary>`

## Acknowledgements

- Ideas are borrowed from [rds file format](https://rdb.fnordig.de/file_format.html)
  , [sqlite file format](https://www.sqlite.org/fileformat.html)
  and [lmdb file format](https://blog.separateconcerns.com/2016-04-03-lmdb-format.html)