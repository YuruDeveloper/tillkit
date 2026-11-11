use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedBackup {
    pub created_unix: u64,
    pub has_root_image: bool,
    pub has_kernel_image: bool,
}

impl SavedBackup {
    pub fn newest_first(
        root_image_times: impl Iterator<Item = u64>,
        kernel_image_times: impl Iterator<Item = u64>,
    ) -> Vec<Self> {
        let mut found: BTreeMap<u64, (bool, bool)> = BTreeMap::new();
        for created_unix in root_image_times {
            found.entry(created_unix).or_default().0 = true;
        }
        for created_unix in kernel_image_times {
            found.entry(created_unix).or_default().1 = true;
        }
        found
            .into_iter()
            .rev()
            .map(|(created_unix, (has_root_image, has_kernel_image))| Self {
                created_unix,
                has_root_image,
                has_kernel_image,
            })
            .collect()
    }

    pub fn contents(&self) -> &'static str {
        match (self.has_root_image, self.has_kernel_image) {
            (true, true) => "init_boot · boot · vbmeta",
            (true, false) => "init_boot · vbmeta",
            (false, _) => "boot · vbmeta",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_of_one_time_are_one_backup_and_the_newest_is_first() {
        let backups = SavedBackup::newest_first([100, 300].into_iter(), [300, 200].into_iter());
        let times: Vec<u64> = backups.iter().map(|backup| backup.created_unix).collect();
        assert_eq!(times, [300, 200, 100]);
        assert_eq!(backups[0].contents(), "init_boot · boot · vbmeta");
        assert_eq!(backups[1].contents(), "boot · vbmeta");
        assert_eq!(backups[2].contents(), "init_boot · vbmeta");
    }
}
