use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct VbmetaTemplate<'a> {
    pub stock_slot_a: &'a Path,
    pub original_slot_b: &'a Path,
}
