# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

## [0.1.2] - 2023-01-12

### Added

### Changed

### Fixed

- Bumped `clokwerk` to version 0.4 to fix [Seg fault in the time package](https://github.com/sopherapps/py_scdb/security/dependabot/2)


## [0.1.1] - 2023-01-12

### Added

### Changed

### Fixed

- Fixed issue with calling `InvertedIndex.add` with same key would delete keys that were added first, but which shared
  prefixes with that key e.g. `bar` would be deleted from the index (but not from the store) if `bare` was `add`ed twice.

## [0.1.0] - 2023-01-11

### Added

- Added full-text search for keys, with pagination using `store.search(term, skip, limit)`

### Changed

- Changed the `Store::new()` signature to include `max_search_index_key_length` option.

### Fixed

## [0.0.2] - 2022-11-09

### Added

### Changed

### Fixed

- Fixed a few typos in the docs
- Fixed typo in the BufferPool.compact_file code

## [0.0.1] - 2022-10-26

### Added

- Initial release

### Changed

### Fixed
