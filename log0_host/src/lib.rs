#[cfg(test)]
mod tests;

pub mod leb128;
pub mod parser;

pub fn bytes_to_read(host_idx: usize, target_idx: usize, buffer_size: usize) -> usize {
    // (head_idx_ - tail_idx_ + mask_ + 1) & mask_;
    target_idx.wrapping_sub(host_idx).wrapping_add(buffer_size) % buffer_size
}
