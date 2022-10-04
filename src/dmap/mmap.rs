use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use twox_hash::XxHash64;

pub(crate) fn load_dmap_from_path(
    path: &str,
) -> HashMap<Vec<u8>, Vec<u8>, BuildHasherDefault<XxHash64>> {
    let m: HashMap<Vec<u8>, Vec<u8>, BuildHasherDefault<XxHash64>> = Default::default();
    m
}
