use crate::internal;
use crate::internal::get_current_timestamp;
use std::io;

const KEY_VALUE_MIN_SIZE_IN_BYTES: u32 = 4 + 4 + 8;

#[derive(Debug, PartialEq)]
pub(crate) struct KeyValueEntry<'a> {
    pub(crate) size: u32,
    pub(crate) key_size: u32,
    pub(crate) key: &'a [u8],
    pub(crate) expiry: u64,
    pub(crate) value: &'a [u8],
}

impl<'a> KeyValueEntry<'a> {
    /// Creates a new KeyValueEntry
    /// `key` is the byte array of the key
    /// `value` is the byte array of the value
    /// `expiry` is the timestamp (in seconds from unix epoch)
    pub(crate) fn new(key: &'a [u8], value: &'a [u8], expiry: u64) -> Self {
        let key_size = key.len() as u32;
        let size = key_size + KEY_VALUE_MIN_SIZE_IN_BYTES + value.len() as u32;

        Self {
            size,
            key_size,
            key,
            expiry,
            value,
        }
    }

    /// Extracts the key value entry from the data array
    pub(crate) fn from_data_array(data: &'a [u8], offset: usize) -> io::Result<Self> {
        let size = u32::from_be_bytes(internal::slice_to_array(&data[offset..offset + 4])?);
        let key_size = u32::from_be_bytes(internal::slice_to_array(&data[offset + 4..offset + 8])?);
        let k_size = key_size as usize;
        let key = &data[offset + 8..offset + 8 + k_size];
        let expiry = u64::from_be_bytes(internal::slice_to_array(
            &data[offset + 8 + k_size..offset + k_size + 16],
        )?);
        let value_size = (size - key_size - KEY_VALUE_MIN_SIZE_IN_BYTES) as usize;
        let value = &data[offset + k_size + 16..offset + k_size + 16 + value_size];

        let entry = Self {
            size,
            key_size,
            key,
            expiry,
            value,
        };
        Ok(entry)
    }

    /// Retrieves the byte array that represents the key value entry.
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        self.size
            .to_be_bytes()
            .iter()
            .chain(&self.key_size.to_be_bytes())
            .chain(self.key)
            .chain(&self.expiry.to_be_bytes())
            .chain(self.value)
            .map(|v| v.to_owned())
            .collect()
    }

    /// Returns true if key has lived for longer than its time-to-live
    /// It will always return false if time-to-live was never set
    pub(crate) fn is_expired(&self) -> bool {
        if self.expiry == 0 {
            false
        } else {
            self.expiry < get_current_timestamp()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KV_DATA_ARRAY: [u8; 22] = [
        /* size: 22u32*/ 0u8, 0, 0, 22, /* key size: 3u32*/ 0, 0, 0, 3,
        /* key */ 102, 111, 111, /* expiry 0u64 */ 0, 0, 0, 0, 0, 0, 0, 0,
        /* value */ 98, 97, 114,
    ];

    #[test]
    fn key_value_entry_from_data_array() {
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let got = KeyValueEntry::from_data_array(&KV_DATA_ARRAY[..], 0)
            .expect("key value from data array");
        assert_eq!(&got, &kv, "got = {:?}, expected = {:?}", &got, &kv);
    }

    #[test]
    fn key_value_entry_from_data_array_with_offset() {
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let data_array: Vec<u8> = [89u8, 78u8]
            .iter()
            .chain(&KV_DATA_ARRAY)
            .map(|v| v.to_owned())
            .collect();
        let got =
            KeyValueEntry::from_data_array(&data_array[..], 2).expect("key value from data array");
        assert_eq!(&got, &kv, "got = {:?}, expected = {:?}", &got, &kv);
    }

    #[test]
    fn key_value_as_bytes() {
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let kv_vec = KV_DATA_ARRAY.to_vec();
        let got = kv.as_bytes();
        assert_eq!(&got, &kv_vec, "got = {:?}, expected = {:?}", &got, &kv_vec);
    }

    #[test]
    fn key_value_is_expired_works() {
        let never_expires = KeyValueEntry::new(&b"never_expires"[..], &b"bar"[..], 0);
        // 1666023836u64 is some past timestamp in October 2022
        let expired = KeyValueEntry::new(&b"expires"[..], &b"bar"[..], 1666023836u64);
        let not_expired = KeyValueEntry::new(
            &b"not_expired"[..],
            &b"bar"[..],
            get_current_timestamp() * 2,
        );

        assert!(!never_expires.is_expired());
        assert!(!not_expired.is_expired());
        assert!(expired.is_expired());
    }
}
