use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::plan_review::PlanReview;
use crate::plan_source::PlanSource;
use front2::current_pair::find_stored_current_pair;
use front2::kernel::{KernelPlan, KernelStore, find_stored_current_kernel_set};
use front2::{
    DeviceState, Front2Error, PairKind, PairStore, PlanRequest, Result, SupportOverride,
    SupportReport, WritePlan,
};

pub const SAME_IMAGES: &str = "the device has the images of this backup now";
pub const UNKNOWN_IMAGES: &str =
    "no stored images are the same as the images that the device has now; back up the device first";

pub struct RestoreSession {
    shell: AdbShell,
    state: DeviceState,
    created_unix: u64,
    support_override: SupportOverride,
}

impl RestoreSession {
    pub fn open(support_override: SupportOverride) -> Result<Self> {
        let mut shell = AdbShell::connect()?;
        let state = DeviceState::read(&mut shell)?;
        SupportReport::evaluate(&state).require_allowed_with(&support_override)?;
        Ok(Self {
            shell,
            state,
            created_unix: now_unix(),
            support_override,
        })
    }

    pub fn plan(mut self, paths: &DataPaths, backup_unix: u64) -> Result<PlanReview> {
        let pairs = PairStore::new(paths.pairs());
        let saved_pair = pairs
            .list()?
            .into_iter()
            .find(|pair| pair.kind == PairKind::DeviceReadback && pair.created_unix == backup_unix);
        let current_pair = find_stored_current_pair(&mut self.shell, &self.state, &pairs).ok();
        let saved_image = saved_pair
            .as_ref()
            .map(|pair| (pair.id.as_str(), pair.init_boot.sha256.as_str()));
        let current_image = current_pair
            .as_ref()
            .map(|pair| (pair.id.as_str(), pair.init_boot.sha256.as_str()));
        if let Some((backup, current)) = differing(saved_image, current_image) {
            return self.plan_pair(paths, &pairs, backup, current);
        }
        let kernel_sets = KernelStore::new(paths.kernel_sets());
        let saved_set = kernel_sets
            .list()?
            .into_iter()
            .find(|set| set.kind == PairKind::DeviceReadback && set.created_unix == backup_unix);
        let current_set =
            find_stored_current_kernel_set(&mut self.shell, &self.state, &kernel_sets).ok();
        let saved_kernel = saved_set
            .as_ref()
            .map(|set| (set.id.as_str(), set.boot.sha256.as_str()));
        let current_kernel = current_set
            .as_ref()
            .map(|set| (set.id.as_str(), set.boot.sha256.as_str()));
        if let Some((backup, current)) = differing(saved_kernel, current_kernel) {
            return self.plan_kernel(paths, &kernel_sets, backup, current);
        }
        let unknown =
            is_unknown(saved_image, current_image) || is_unknown(saved_kernel, current_kernel);
        Err(nothing_to_restore(unknown))
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

type StoredImage<'a> = (&'a str, &'a str);

pub fn differing<'a>(
    saved: Option<StoredImage<'a>>,
    current: Option<StoredImage<'a>>,
) -> Option<(&'a str, &'a str)> {
    let ((saved_id, saved_sha256), (current_id, current_sha256)) = (saved?, current?);
    (saved_sha256 != current_sha256).then_some((saved_id, current_id))
}

pub fn is_unknown(saved: Option<StoredImage<'_>>, current: Option<StoredImage<'_>>) -> bool {
    saved.is_some() && current.is_none()
}

pub fn nothing_to_restore(unknown: bool) -> Front2Error {
    if unknown {
        Front2Error::Store(UNKNOWN_IMAGES.into())
    } else {
        Front2Error::Plan(SAME_IMAGES.into())
    }
}

pub fn is_same_images(error: &str) -> bool {
    error.ends_with(SAME_IMAGES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restore_needs_a_saved_image_that_differs_from_the_known_device_image() {
        let saved = Some(("readback-1", "aa"));
        assert_eq!(
            differing(saved, Some(("candidate-2", "bb"))),
            Some(("readback-1", "candidate-2"))
        );
        assert_eq!(differing(saved, Some(("readback-1", "aa"))), None);
        assert_eq!(differing(saved, None), None);
        assert_eq!(differing(None, Some(("candidate-2", "bb"))), None);
    }

    #[test]
    fn an_unknown_device_image_is_told_apart_from_the_same_image() {
        let saved = Some(("readback-1", "aa"));
        assert!(is_unknown(saved, None));
        assert!(!is_unknown(saved, Some(("readback-1", "aa"))));
        assert!(
            !is_unknown(None, None),
            "the backup does not hold this image"
        );
        assert!(
            nothing_to_restore(true)
                .to_string()
                .contains(UNKNOWN_IMAGES)
        );
        assert!(is_same_images(&nothing_to_restore(false).to_string()));
        assert!(!is_same_images(&nothing_to_restore(true).to_string()));
        assert!(!is_same_images(""));
    }
}
