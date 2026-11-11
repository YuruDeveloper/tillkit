use super::magisk_package::MagiskPackage;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct PatchInputs<'a> {
    pub package: &'a MagiskPackage,
    pub preinit_device: &'a str,
    pub work_dir: &'a Path,
}
