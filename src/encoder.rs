use rolling_hash::RollingHash;
use std::io::{BufReader, Read, Seek, Write};
use std::io;
use std::cmp;
use std::mem;

/// cpu/memory efficient hashmap from hash_value to multiple window indexes
/// window hashes must be inserted backward
pub struct WindowHashMap {
    window_size: usize,
    current_window_index: usize,
    /// hashed_value => window_index
    table: Vec<usize>,
    /// for each window, the next window_index+1 that has the same table index (while > 0)
    next_window_indexes: Vec<usize>,
}

struct Matches<'a> {
    hash_map: &'a WindowHashMap,
    window_index: usize,
}

impl<'a> Iterator for Matches<'a> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        if self.window_index > 0 {
            let real_window_index = self.window_index - 1;
            self.window_index = self.hash_map.next_window_indexes[real_window_index];
            Some(real_window_index as u64 * self.hash_map.window_size as u64)
        } else {
            None
        }
    }
}

impl WindowHashMap {
    fn new(file_size: u64, window_size: usize, hash_size: usize) -> WindowHashMap {
        let mut table = Vec::with_capacity(hash_size);
        table.resize(hash_size, 0);

        let indexes_size = (file_size / (window_size as u64)) as usize;
        let mut next_window_indexes = Vec::with_capacity(indexes_size);
        next_window_indexes.resize(indexes_size, 0);

        WindowHashMap {
            window_size,
            current_window_index: indexes_size,
            table,
            next_window_indexes,
        }
    }

    fn prepend_window(&mut self, hash_value: u32) {
        assert!(self.current_window_index > 0);
        let table_index = (hash_value as usize) % self.table.len();
        let found_window_index = &mut self.table[table_index];
        if *found_window_index > 0 {
            self.next_window_indexes[self.current_window_index] = *found_window_index;
        }
        *found_window_index = self.current_window_index;
        self.current_window_index -= 1;
    }

    fn find_matches<'a>(&'a self, hash_value: u32) -> Matches<'a> {
        let table_index = (hash_value as usize) % self.table.len();
        let found_window_index = self.table[table_index];
        Matches {
            hash_map: self,
            window_index: found_window_index,
        }
    }
}

pub struct VCDiffEncoder<OLD: Read + Seek, NEW: Read + Seek> {
    diff_window_size: usize,
    old: OLD,
    old_hash_map: WindowHashMap,
    new: NEW,
    new_hash_map: WindowHashMap,
}

fn hash_map<F: Read + Seek>(
    file: &mut F,
    rolling_hash: &RollingHash,
) -> Result<WindowHashMap, io::Error> {
    let file_size = file.seek(io::SeekFrom::End(0))?;
    let diff_window_size = rolling_hash.window_size();
    let diff_window_size_u64 = diff_window_size as u64;
    let mut hash_map = WindowHashMap::new(file_size, diff_window_size, 1 << 28 /* 268 MB */);
    let mut position = file_size - (file_size % diff_window_size_u64);
    let mut buffer = [0u8; 32768];
    let buffer_len = buffer.len() - buffer.len() % diff_window_size; // force alignment
    while position > 0 {
        let read_size = cmp::min(position, buffer_len as u64);
        let mut read_size_usize = read_size as usize;
        file.seek(io::SeekFrom::Start(position - read_size))?;
        file.read(&mut buffer[0..read_size_usize])?;
        position -= read_size;
        while read_size_usize > 0 {
            read_size_usize -= diff_window_size;
            let h = rolling_hash.hash(&buffer[read_size_usize..diff_window_size]);
            hash_map.prepend_window(h);
        }
    }
    file.seek(io::SeekFrom::Start(0))?;

    Ok(hash_map)
}

impl<OLD: Read + Seek, NEW: Read + Seek> VCDiffEncoder<OLD, NEW> {
    pub fn new(
        mut old: OLD,
        mut new: NEW,
        diff_window_size: usize,
    ) -> Result<VCDiffEncoder<OLD, NEW>, io::Error> {
        assert!(diff_window_size >= 4);
        let rolling_hash = RollingHash::new(diff_window_size);
        let old_hash_map = hash_map(&mut old, &rolling_hash)?;
        let new_hash_map = hash_map(&mut new, &rolling_hash)?;
        Ok(VCDiffEncoder {
            diff_window_size,
            old,
            old_hash_map,
            new,
            new_hash_map,
        })
    }
}
