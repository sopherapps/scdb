use crate::internal;
use crate::internal::entries::values::shared::ValueEntry;
use crate::internal::macros::safe_slice;
use crate::internal::utils::{bool_to_byte_array, byte_array_to_bool};
use std::fmt::Debug;
use std::io;

pub(crate) const INVERTED_INDEX_ENTRY_MIN_SIZE_IN_BYTES: u32 = 4 + 4 + 1 + 1 + 8 + 8 + 8 + 8;
pub(crate) const INVERTED_INDEX_ENTRY_BYTES_INDEX_KEY_OFFSET: usize = 8;

#[derive(Debug, PartialEq)]
pub(crate) struct InvertedIndexEntry<'a> {
    pub(crate) size: u32,
    pub(crate) index_key_size: u32,
    pub(crate) index_key: &'a [u8],
    pub(crate) key: &'a [u8],
    pub(crate) is_deleted: bool,
    pub(crate) is_root: bool,
    pub(crate) expiry: u64,
    pub(crate) next_offset: u64,
    pub(crate) previous_offset: u64,
    pub(crate) kv_address: u64,
}

impl<'a> InvertedIndexEntry<'a> {
    /// Creates a new InvertedIndexEntry
    pub(crate) fn new(
        index_key: &'a [u8],
        key: &'a [u8],
        expiry: u64,
        is_root: bool,
        kv_address: u64,
        next_offset: u64,
        previous_offset: u64,
    ) -> Self {
        let key_size = key.len() as u32;
        let index_key_size = index_key.len() as u32;
        let size = key_size + index_key_size + INVERTED_INDEX_ENTRY_MIN_SIZE_IN_BYTES;

        Self {
            size,
            index_key_size,
            key,
            expiry,
            is_root,
            index_key,
            kv_address,
            next_offset,
            previous_offset,
            is_deleted: false,
        }
    }
}

impl<'a> ValueEntry<'a> for InvertedIndexEntry<'a> {
    #[inline(always)]
    fn get_expiry(&self) -> u64 {
        self.expiry
    }

    fn from_data_array(data: &'a [u8], offset: usize) -> io::Result<Self> {
        let data_len = data.len();
        let size_slice = safe_slice!(data, offset, offset + 4, data_len)?;
        let size = u32::from_be_bytes(internal::slice_to_array(size_slice)?);

        let index_key_size_slice = safe_slice!(data, offset + 4, offset + 8, data_len)?;
        let index_key_size = u32::from_be_bytes(internal::slice_to_array(index_key_size_slice)?);

        let index_k_size = index_key_size as usize;
        let index_key = safe_slice!(data, offset + 8, offset + 8 + index_k_size, data_len)?;

        let k_size = (size - index_key_size - INVERTED_INDEX_ENTRY_MIN_SIZE_IN_BYTES) as usize;
        let key = safe_slice!(
            data,
            offset + 8 + index_k_size,
            offset + 8 + index_k_size + k_size,
            data_len
        )?;

        let is_deleted_slice = safe_slice!(
            data,
            offset + 8 + k_size + index_k_size,
            offset + k_size + index_k_size + 9,
            data_len
        )?;
        let is_deleted = byte_array_to_bool(is_deleted_slice);

        let is_root_slice = safe_slice!(
            data,
            offset + 9 + k_size + index_k_size,
            offset + k_size + index_k_size + 10,
            data_len
        )?;
        let is_root = byte_array_to_bool(is_root_slice);

        let expiry_slice = safe_slice!(
            data,
            offset + 10 + k_size + index_k_size,
            offset + k_size + index_k_size + 18,
            data_len
        )?;
        let expiry = u64::from_be_bytes(internal::slice_to_array(expiry_slice)?);

        let next_offset_slice = safe_slice!(
            data,
            offset + k_size + index_k_size + 18,
            offset + k_size + index_k_size + 26,
            data_len
        )?;
        let next_offset = u64::from_be_bytes(internal::slice_to_array(next_offset_slice)?);

        let previous_offset_slice = safe_slice!(
            data,
            offset + k_size + index_k_size + 26,
            offset + k_size + index_k_size + 34,
            data_len
        )?;
        let previous_offset = u64::from_be_bytes(internal::slice_to_array(previous_offset_slice)?);

        let kv_address_slice = safe_slice!(
            data,
            offset + k_size + index_k_size + 34,
            offset + k_size + index_k_size + 42,
            data_len
        )?;
        let kv_address = u64::from_be_bytes(internal::slice_to_array(kv_address_slice)?);

        let entry = Self {
            size,
            index_key_size,
            index_key,
            key,
            expiry,
            is_deleted,
            is_root,
            next_offset,
            previous_offset,
            kv_address,
        };

        Ok(entry)
    }

    fn as_bytes(&self) -> Vec<u8> {
        self.size
            .to_be_bytes()
            .iter()
            .chain(&self.index_key_size.to_be_bytes())
            .chain(self.index_key)
            .chain(self.key)
            .chain(bool_to_byte_array(self.is_deleted))
            .chain(bool_to_byte_array(self.is_root))
            .chain(&self.expiry.to_be_bytes())
            .chain(&self.next_offset.to_be_bytes())
            .chain(&self.previous_offset.to_be_bytes())
            .chain(&self.kv_address.to_be_bytes())
            .map(|v| v.to_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::get_current_timestamp;

    const SEARCH_ENTRY_BYTE_ARRAY: [u8; 47] = [
        /* size: 47u32*/ 0u8, 0, 0, 47, /* index key size: 2u32*/ 0, 0, 0, 2,
        /* key: fo */ 102, 111, /* key: foo */ 102, 111, 111, /* is_deleted */ 0,
        /* is_root */ 0, /* expiry 0u64 */ 0, 0, 0, 0, 0, 0, 0, 0,
        /* next offset 900u64 */ 0, 0, 0, 0, 0, 0, 3, 132, /* previous offset 90u64 */ 0,
        0, 0, 0, 0, 0, 0, 90, /* kv_address: 100u64 */ 0, 0, 0, 0, 0, 0, 0, 100,
    ];

    #[test]
    fn search_entry_from_data_array() {
        let expected = InvertedIndexEntry::new(&b"fo"[..], &b"foo"[..], 0, false, 100, 900, 90);
        let got = InvertedIndexEntry::from_data_array(&SEARCH_ENTRY_BYTE_ARRAY[..], 0)
            .expect("search entry from data array");
        assert_eq!(
            &got, &expected,
            "got = {:?}, expected = {:?}",
            &got, &expected
        );
    }

    #[test]
    fn search_entry_from_data_array_with_offset() {
        let expected = InvertedIndexEntry::new(&b"fo"[..], &b"foo"[..], 0, false, 100, 900, 90);
        let data_array: Vec<u8> = [89u8, 78u8]
            .iter()
            .chain(&SEARCH_ENTRY_BYTE_ARRAY)
            .map(|v| v.to_owned())
            .collect();
        let got = InvertedIndexEntry::from_data_array(&data_array[..], 2)
            .expect("search entry from data array");
        assert_eq!(
            &got, &expected,
            "got = {:?}, expected = {:?}",
            &got, &expected
        );
    }

    #[test]
    fn search_entry_from_data_array_with_out_of_bounds_offset() {
        let data_array: Vec<u8> = [89u8, 78u8]
            .iter()
            .chain(&SEARCH_ENTRY_BYTE_ARRAY)
            .map(|v| v.to_owned())
            .collect();
        let got = InvertedIndexEntry::from_data_array(&data_array[..], 4);
        assert!(got.is_err());
    }

    #[test]
    fn search_entry_as_bytes() {
        let entry = InvertedIndexEntry::new(&b"fo"[..], &b"foo"[..], 0, false, 100, 900, 90);
        let expected = SEARCH_ENTRY_BYTE_ARRAY.to_vec();
        let got = entry.as_bytes();
        assert_eq!(
            &got, &expected,
            "got = {:?}, expected = {:?}",
            &got, &expected
        );
    }

    #[test]
    fn entry_is_expired_works() {
        let never_expires =
            InvertedIndexEntry::new(&b"ne"[..], &b"never_expires"[..], 0, false, 100, 900, 90);
        // 1666023836u64 is some past timestamp in October 2022
        let expired = InvertedIndexEntry::new(
            &b"exp"[..],
            &b"expires"[..],
            1666023836u64,
            false,
            100,
            900,
            90,
        );
        let not_expired = InvertedIndexEntry::new(
            &b"no"[..],
            &b"not_expired"[..],
            get_current_timestamp() * 2,
            false,
            100,
            900,
            90,
        );

        assert!(!never_expires.is_expired());
        assert!(!not_expired.is_expired());
        assert!(expired.is_expired());
    }
}
