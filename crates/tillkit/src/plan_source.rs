use crate::data_paths::DataPaths;
use front2::kernel::KernelPlanFacts;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanSource {
    Pair,
    Kernel(Box<KernelPlanFacts>),
}

impl PlanSource {
    pub fn store_root(&self, paths: &DataPaths) -> PathBuf {
        match self {
            Self::Pair => paths.pairs(),
            Self::Kernel(_) => paths.kernel_sets(),
        }
    }

    pub fn kernel_facts(&self) -> Option<&KernelPlanFacts> {
        match self {
            Self::Pair => None,
            Self::Kernel(facts) => Some(facts),
        }
    }
}
