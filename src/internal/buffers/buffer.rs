use crate::internal::KeyValueEntry;
use std::cmp::min;
use std::io;

#[derive(Debug, PartialEq)]
pub(crate) struct Value {
    pub(crate) data: Vec<u8>,
    pub(crate) is_expired: bool,
}

/// This is the in-memory cache for byte arrays read from file
/// Its `left_offset` is the file_offset where the byte array `data` is read from
/// while its `right_offset` is the *exclusive* upper bound file offset of the same.
/// the `right_offset` is not an offset within this buffer but is the left_offset of the buffer
/// that would be got from the file to the immediate right of this buffer's data array
#[derive(Debug, PartialEq)]
pub(crate) struct Buffer {
    capacity: usize,
    pub(crate) data: Vec<u8>,
    pub(crate) left_offset: u64,
    pub(crate) right_offset: u64,
}

impl Buffer {
    /// Creates a new Buffer with the given left_offset
    #[inline]
    pub(crate) fn new(left_offset: u64, data: &[u8], capacity: usize) -> Self {
        let upper_bound = min(data.len(), capacity);
        let right_offset = left_offset + upper_bound as u64;
        let data = data[..upper_bound].to_vec();
        Self {
            capacity,
            data,
            left_offset,
            right_offset,
        }
    }

    /// Checks if the given address can be appended to this buffer
    /// The buffer should be contiguous thus this is true if `address` is
    /// equal to the exclusive `right_offset` and the capacity has not been reached yet.
    #[inline]
    pub(crate) fn can_append(&self, address: u64) -> bool {
        (self.right_offset - self.left_offset) < self.capacity as u64
            && address == self.right_offset
    }

    /// Checks if the given address is in this buffer
    #[inline]
    pub(crate) fn contains(&self, address: u64) -> bool {
        self.left_offset <= address && address < self.right_offset
    }

    /// Appends the data to the end of the array
    /// It returns the address (or offset) where the data was appended
    #[inline]
    pub(crate) fn append(&mut self, data: Vec<u8>) -> u64 {
        let mut data = data;
        let data_length = data.len();
        self.data.append(&mut data);
        let prev_right_offset = self.right_offset;
        self.right_offset += data_length as u64;
        return prev_right_offset;
    }

    /// Replaces the data at the given address with the new data
    #[inline]
    pub(crate) fn replace(&mut self, address: u64, data: Vec<u8>) -> io::Result<()> {
        let data_length = data.len();
        self.validate_bounds(address, address + data_length as u64)?;

        let start = (address - self.left_offset) as usize;
        let stop = start + data_length;
        self.data.splice(start..stop, data);
        Ok(())
    }

    /// Returns the Some(Value) at the given address if the key there corresponds to the given key
    /// Otherwise, it returns None
    /// This is to handle hash collisions.
    #[inline]
    pub(crate) fn get_value(&self, address: u64, key: &[u8]) -> io::Result<Option<Value>> {
        let offset = (address - self.left_offset) as usize;
        let entry = KeyValueEntry::from_data_array(&self.data, offset)?;
        let value = if entry.key == key {
            Some(Value::from(&entry))
        } else {
            None
        };

        Ok(value)
    }

    /// Reads an arbitrary array at the given address and of given size and returns it
    #[inline]
    pub(crate) fn read_at(&mut self, address: u64, size: usize) -> io::Result<Vec<u8>> {
        self.validate_bounds(address, address + size as u64)?;
        let offset = (address - self.left_offset) as usize;
        let data_array = self.data[offset..offset + size].to_vec();
        Ok(data_array)
    }

    /// Checks to see if the given address is for the given key
    #[inline]
    pub(crate) fn addr_belongs_to_key(&self, address: u64, key: &[u8]) -> io::Result<bool> {
        if address < self.left_offset || address > self.right_offset {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Address {} is out of bounds of {}-{}",
                    address, self.left_offset, self.right_offset
                ),
            ));
        }

        let offset = (address - self.left_offset) as usize;
        let entry = KeyValueEntry::from_data_array(&self.data, offset)?;
        Ok(entry.key == key)
    }

    /// Checks if the given range is within bounds for this buffer
    /// This is just a helper
    fn validate_bounds(&self, lower: u64, upper: u64) -> io::Result<()> {
        if lower < self.left_offset || upper > self.right_offset {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Span {}-{} is out of bounds for {:?}",
                    lower, upper, &self.data
                ),
            ))
        } else {
            Ok(())
        }
    }
}

impl From<&KeyValueEntry<'_>> for Value {
    fn from(entry: &KeyValueEntry) -> Self {
        Self {
            data: entry.value.to_vec(),
            is_expired: entry.is_expired(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::get_current_timestamp;

    const KV_DATA_ARRAY: [u8; 22] = [
        /* size: 22u32*/ 0u8, 0, 0, 22, /* key size: 3u32*/ 0, 0, 0, 3,
        /* key */ 102, 111, 111, /* expiry 0u64 */ 0, 0, 0, 0, 0, 0, 0, 0,
        /* value */ 98, 97, 114,
    ];
    const CAPACITY: usize = 4098;

    #[test]
    fn value_from_key_value_entry() {
        type Record<'a> = (KeyValueEntry<'a>, Value);
        let test_table: Vec<Record> = vec![
            (
                KeyValueEntry::new(&b"never_expires"[..], &b"barer"[..], 0),
                Value {
                    data: vec![98, 97, 114, 101, 114],
                    is_expired: false,
                },
            ),
            (
                KeyValueEntry::new(&b"expires"[..], &b"Hallelujah"[..], 1666023836u64),
                Value {
                    data: vec![72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
                    is_expired: true,
                },
            ),
            (
                KeyValueEntry::new(
                    &b"not_expired"[..],
                    &b"bar"[..],
                    get_current_timestamp() * 2,
                ),
                Value {
                    data: vec![98, 97, 114],
                    is_expired: false,
                },
            ),
        ];

        for (kv, expected) in test_table {
            assert_eq!(&Value::from(&kv), &expected);
        }
    }

    #[test]
    fn buffer_contains() {
        let buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        let test_table = vec![
            (8u64, false),
            (80u64, true),
            (89u64, false),
            (876u64, false),
        ];

        for (addr, expected) in test_table {
            assert_eq!(expected, buf.contains(addr));
        }
    }

    #[test]
    fn buffer_can_append() {
        let data = &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104];
        let offset = 79u64;
        let test_table = vec![
            (CAPACITY, 8u64, false),
            (CAPACITY, 89, true),
            (10, 89, false),
            (11, 89, true),
            (CAPACITY, 90, false),
            (CAPACITY, 900, false),
            (CAPACITY, 17, false),
            (10, 83, false),
        ];

        for (cap, addr, expected) in test_table {
            let buf = Buffer::new(offset, data, cap);

            assert_eq!(expected, buf.can_append(addr));
        }
    }

    #[test]
    fn buffer_appends() {
        let mut buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        buf.append(vec![98u8, 97, 114, 101, 114]);
        assert_eq!(
            buf,
            Buffer {
                capacity: CAPACITY,
                data: vec![72u8, 97, 108, 108, 101, 108, 117, 106, 97, 104, 98, 97, 114, 101, 114],
                left_offset: 79,
                right_offset: 94,
            }
        )
    }

    #[test]
    fn buffer_replace() {
        let mut buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        buf.replace(82, vec![98u8, 97, 114, 101, 114])
            .expect("replace");
        assert_eq!(
            buf,
            Buffer {
                capacity: CAPACITY,
                data: vec![72u8, 97, 108, 98, 97, 114, 101, 114, 97, 104],
                left_offset: 79,
                right_offset: 89,
            }
        )
    }

    #[test]
    fn buffer_replace_out_of_bounds() {
        let mut buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        let test_table = vec![
            (85, vec![98u8, 97, 114, 101, 114]),
            (86, vec![98u8, 97, 114, 101]),
            (90, vec![98u8, 97, 114, 101, 114]),
            (100, vec![98u8]),
            (70, vec![98u8, 97, 114, 101, 114]),
        ];

        for (addr, data) in test_table {
            let v = buf.replace(addr, data);
            assert!(v.is_err())
        }
    }

    #[test]
    fn buffer_get_value() {
        let buf = Buffer::new(79, &KV_DATA_ARRAY[..], CAPACITY);
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);

        let test_table = vec![
            (79u64, b"foo", Some(Value::from(&kv))),
            (79u64, b"bar", None),
        ];

        for (addr, k, expected) in test_table {
            let v = buf
                .get_value(addr, &k[..])
                .expect(&format!("gets value for {:?}", &k));
            assert_eq!(v, expected);
        }
    }

    #[test]
    fn buffer_get_value_out_of_bounds() {
        let buf = Buffer::new(79, &KV_DATA_ARRAY[..], CAPACITY);

        let test_table = vec![(84u64, b"foo"), (84u64, b"bar")];

        for (addr, k) in test_table {
            let v = buf.get_value(addr, &k[..]);
            assert!(v.is_err());
        }
    }

    #[test]
    fn buffer_read_at() {
        let mut buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        let v = buf.read_at(82, 5).expect("read at 82");
        assert_eq!(v, vec![108, 101, 108, 117, 106])
    }

    #[test]
    fn buffer_read_at_out_of_bounds() {
        let mut buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        let test_table = vec![(85, 5), (86, 4), (90, 4), (100, 1), (70, 3)];

        for (addr, size) in test_table {
            let v = buf.read_at(addr, size);
            assert!(v.is_err())
        }
    }

    #[test]
    fn buffer_addr_belongs_to_key() {
        let buf = Buffer::new(79, &KV_DATA_ARRAY[..], CAPACITY);
        let test_table = vec![(79u64, b"foo", true), (79u64, b"bar", false)];

        for (addr, k, expected) in test_table {
            let v = buf
                .addr_belongs_to_key(addr, &k[..])
                .expect(&format!("gets value for {:?}", &k));
            assert_eq!(v, expected);
        }
    }

    #[test]
    fn buffer_addr_belongs_to_key_out_of_bounds() {
        let buf = Buffer::new(79, &KV_DATA_ARRAY[..], CAPACITY);
        let test_table = vec![
            (790u64, b"foo"),
            (78u64, b"foo"),
            (80u64, b"foo"),
            (78u64, b"bar"),
            (790u64, b"bar"),
        ];

        for (addr, k) in test_table {
            let v = buf.addr_belongs_to_key(addr, &k[..]);
            assert!(v.is_err());
        }
    }

    #[test]
    fn buffer_validate_bounds() {
        let buf = Buffer::new(
            79,
            &[72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
            CAPACITY,
        );
        let test_table = vec![
            (85, 90, true),
            (86, 90, true),
            (90, 94, true),
            (100, 101, true),
            (70, 73, true),
            (79, 83, false),
            (83, 88, false),
            (79, 89, false),
        ];

        for (lower, upper, expected) in test_table {
            let v = buf.validate_bounds(lower, upper);
            assert_eq!(v.is_err(), expected)
        }
    }
}
