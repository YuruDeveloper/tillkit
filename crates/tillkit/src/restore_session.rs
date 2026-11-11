use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::diagnosis::Diagnosis;
use crate::plan_review::PlanReview;
use crate::plan_source::PlanSource;
use crate::stored_image::StoredImage;
use front2::current_pair::find_stored_current_pair;
use front2::kernel::{KernelPlan, KernelStore, find_stored_current_kernel_set};
use front2::{
    DeviceProfile, DeviceState, Front2Error, PairKind, PairStore, PlanRequest, Result,
    SupportOverride, SupportReport, WritePlan,
};

pub const SAME_IMAGES: &str = "the device has the images of this backup now";
pub const UNKNOWN_IMAGES: &str =
    "no stored images are the same as the images that the device has now; back up the device first";
pub const MISSING_BACKUP: &str = "the storage does not have this backup now";
pub const AMBIGUOUS_BACKUP: &str =
    "the storage has more than one backup with this time; a restore does not select one of them";
pub const OTHER_VBMETA: &str = "the compared images of this backup are the same as the images on the device, but vbmeta is different; a restore does not write vbmeta only";

pub struct RestoreSession {
    shell: AdbShell,
    state: DeviceState,
    profile: &'static DeviceProfile,
    created_unix: u64,
    support_override: SupportOverride,
}

impl RestoreSession {
    pub fn open(support_override: SupportOverride) -> Result<Self> {
        let mut shell = AdbShell::connect()?;
        let state = DeviceState::read(&mut shell)?;
        let profile = Diagnosis::profile_for(&state);
        SupportReport::evaluate(profile, &state).require_allowed_with(&support_override)?;
        Ok(Self {
            shell,
            state,
            profile,
            created_unix: now_unix(),
            support_override,
        })
    }

    pub fn plan(mut self, paths: &DataPaths, backup_unix: u64) -> Result<PlanReview> {
        let pairs = PairStore::new(paths.pairs(), self.profile);
        let mut saved_pairs = pairs.list()?;
        saved_pairs.retain(|pair| {
            pair.kind == PairKind::DeviceReadback && pair.created_unix == backup_unix
        });
        let saved_pair = only_one(saved_pairs)?;
        let current_pair = find_stored_current_pair(&mut self.shell, &self.state, &pairs).ok();
        let saved_image = saved_pair.as_ref().map(StoredImage::of_pair);
        let current_image = current_pair.as_ref().map(StoredImage::of_pair);
        if let Some((backup, current)) = differing(saved_image, current_image) {
            return self.plan_pair(paths, &pairs, backup, current);
        }
        let kernel_sets = KernelStore::new(paths.kernel_sets(), self.profile);
        let mut saved_sets = kernel_sets.list()?;
        saved_sets
            .retain(|set| set.kind == PairKind::DeviceReadback && set.created_unix == backup_unix);
        let saved_set = only_one(saved_sets)?;
        let current_set =
            find_stored_current_kernel_set(&mut self.shell, &self.state, &kernel_sets).ok();
        let saved_kernel = saved_set.as_ref().map(StoredImage::of_kernel_set);
        let current_kernel = current_set.as_ref().map(StoredImage::of_kernel_set);
        if let Some((backup, current)) = differing(saved_kernel, current_kernel) {
            return self.plan_kernel(paths, &kernel_sets, backup, current);
        }
        Err(nothing_to_restore(
            (saved_image, current_image),
            (saved_kernel, current_kernel),
        ))
    }

    fn plan_pair(
        mut self,
        paths: &DataPaths,
        store: &PairStore,
        backup: &str,
        current: &str,
    ) -> Result<PlanReview> {
        let id = format!("restore-{}", self.created_unix);
        let request = PlanRequest {
            id: &id,
            created_unix: self.created_unix,
            support_override: self.support_override,
        };
        let plan = WritePlan::prepare(
            &mut self.shell,
            &self.state,
            store,
            backup,
            current,
            request,
        )?;
        let directory = paths.plans().join(&id);
        let sha256 = plan.save(&directory)?;
        Ok(PlanReview {
            plan,
            sha256,
            directory,
            source: PlanSource::Pair,
        })
    }

    fn plan_kernel(
        mut self,
        paths: &DataPaths,
        store: &KernelStore,
        backup: &str,
        current: &str,
    ) -> Result<PlanReview> {
        let id = format!("restore-{}", self.created_unix);
        let request = PlanRequest {
            id: &id,
            created_unix: self.created_unix,
            support_override: self.support_override,
        };
        let kernel_plan = KernelPlan::prepare(
            &mut self.shell,
            &self.state,
            store,
            backup,
            current,
            request,
        )?;
        let directory = paths.plans().join(&id);
        let sha256 = kernel_plan.save(&directory)?;
        Ok(PlanReview {
            plan: kernel_plan.plan,
            sha256,
            directory,
            source: PlanSource::Kernel(Box::new(kernel_plan.facts)),
        })
    }
}

type Compared<'a> = (Option<StoredImage<'a>>, Option<StoredImage<'a>>);

pub fn only_one<Saved>(mut saved: Vec<Saved>) -> Result<Option<Saved>> {
    if saved.len() > 1 {
        return Err(Front2Error::Store(AMBIGUOUS_BACKUP.into()));
    }
    Ok(saved.pop())
}

pub fn differing<'a>(
    saved: Option<StoredImage<'a>>,
    current: Option<StoredImage<'a>>,
) -> Option<(&'a str, &'a str)> {
    let (saved, current) = (saved?, current?);
    (saved.image_sha256 != current.image_sha256).then_some((saved.id, current.id))
}

pub fn is_unknown((saved, current): Compared<'_>) -> bool {
    saved.is_some() && current.is_none()
}

pub fn differs_in_vbmeta((saved, current): Compared<'_>) -> bool {
    saved
        .zip(current)
        .is_some_and(|(saved, current)| saved.vbmeta_sha256 != current.vbmeta_sha256)
}

pub fn nothing_to_restore(pair: Compared<'_>, kernel: Compared<'_>) -> Front2Error {
    if pair.0.is_none() && kernel.0.is_none() {
        return Front2Error::Store(MISSING_BACKUP.into());
    }
    if is_unknown(pair) || is_unknown(kernel) {
        return Front2Error::Store(UNKNOWN_IMAGES.into());
    }
    if differs_in_vbmeta(pair) || differs_in_vbmeta(kernel) {
        return Front2Error::Plan(OTHER_VBMETA.into());
    }
    Front2Error::Plan(SAME_IMAGES.into())
}

pub fn is_same_images(error: &str) -> bool {
    error.ends_with(SAME_IMAGES)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image<'a>(id: &'a str, image_sha256: &'a str, vbmeta_sha256: &'a str) -> StoredImage<'a> {
        StoredImage {
            id,
            image_sha256,
            vbmeta_sha256,
        }
    }

    #[test]
    fn a_restore_needs_a_saved_image_that_differs_from_the_known_device_image() {
        let saved = Some(image("readback-1", "aa", "v1"));
        assert_eq!(
            differing(saved, Some(image("candidate-2", "bb", "v2"))),
            Some(("readback-1", "candidate-2"))
        );
        assert_eq!(differing(saved, saved), None);
        assert_eq!(differing(saved, None), None);
        assert_eq!(
            differing(None, Some(image("candidate-2", "bb", "v2"))),
            None
        );
    }

    #[test]
    fn an_unknown_device_image_is_told_apart_from_the_same_image() {
        let saved = Some(image("readback-1", "aa", "v1"));
        assert!(is_unknown((saved, None)));
        assert!(!is_unknown((saved, saved)));
        assert!(
            !is_unknown((None, None)),
            "the backup does not hold this image"
        );
        let unknown = nothing_to_restore((saved, None), (saved, saved)).to_string();
        assert!(unknown.contains(UNKNOWN_IMAGES));
        assert!(!is_same_images(&unknown));
        let same = nothing_to_restore((saved, saved), (saved, saved)).to_string();
        assert!(is_same_images(&same));
        assert!(!is_same_images(""));
    }

    #[test]
    fn doc_1057_two_backups_with_the_same_time_are_not_one_backup() {
        assert_eq!(only_one(Vec::<u8>::new()).expect("none"), None);
        assert_eq!(only_one(vec![7]).expect("one"), Some(7));
        let error = only_one(vec![7, 9]).expect_err("two").to_string();
        assert!(error.contains(AMBIGUOUS_BACKUP));
        assert!(!is_same_images(&error));
    }

    #[test]
    fn doc_1057_a_different_vbmeta_is_not_the_same_images() {
        let saved = Some(image("readback-1", "aa", "v1"));
        let resigned = Some(image("candidate-2", "aa", "v2"));
        assert_eq!(differing(saved, resigned), None);
        let pair_differs = nothing_to_restore((saved, resigned), (saved, saved)).to_string();
        assert!(pair_differs.contains(OTHER_VBMETA));
        assert!(!is_same_images(&pair_differs));
        let kernel_differs = nothing_to_restore((saved, saved), (saved, resigned)).to_string();
        assert!(kernel_differs.contains(OTHER_VBMETA));
        let unknown_first = nothing_to_restore((saved, resigned), (saved, None)).to_string();
        assert!(unknown_first.contains(UNKNOWN_IMAGES));
    }

    #[test]
    fn doc_1057_a_backup_that_left_the_storage_is_not_the_same_images() {
        let current = Some(image("candidate-2", "bb", "v2"));
        let missing = nothing_to_restore((None, current), (None, current)).to_string();
        assert!(missing.contains(MISSING_BACKUP));
        assert!(!is_same_images(&missing));
        let saved = Some(image("readback-1", "bb", "v2"));
        let pair_only = nothing_to_restore((saved, current), (None, current)).to_string();
        assert!(is_same_images(&pair_only));
    }

    #[test]
    fn doc_1059_a_backup_with_one_image_does_not_name_an_image_without_a_comparison() {
        let saved = Some(image("readback-1", "aa", "v1"));
        let resigned = Some(image("candidate-2", "aa", "v2"));
        let pair_only = nothing_to_restore((saved, resigned), (None, None)).to_string();
        assert!(pair_only.contains(OTHER_VBMETA));
        let kernel_only = nothing_to_restore((None, None), (saved, resigned)).to_string();
        assert!(kernel_only.contains(OTHER_VBMETA));
        assert!(!OTHER_VBMETA.contains("init_boot"));
        assert!(!OTHER_VBMETA.contains("boot image"));
        let same = nothing_to_restore((saved, saved), (None, None)).to_string();
        assert!(!same.contains(OTHER_VBMETA));
    }
}
