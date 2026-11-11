use front2::PairManifest;
use front2::kernel::KernelSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredImage<'a> {
    pub id: &'a str,
    pub image_sha256: &'a str,
    pub vbmeta_sha256: &'a str,
}

impl<'a> StoredImage<'a> {
    pub fn of_pair(pair: &'a PairManifest) -> Self {
        Self {
            id: &pair.id,
            image_sha256: &pair.init_boot.sha256,
            vbmeta_sha256: &pair.vbmeta.sha256,
        }
    }

    pub fn of_kernel_set(set: &'a KernelSet) -> Self {
        Self {
            id: &set.id,
            image_sha256: &set.boot.sha256,
            vbmeta_sha256: &set.vbmeta.sha256,
        }
    }
}
