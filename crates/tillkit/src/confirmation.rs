#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    ForceSupport,
    Write,
    Reboot,
    RebootTo,
    InstallUntestedBuild,
    InstallWrite,
    InstallRestart,
    FullRestoreWrite,
    FullRestoreRestart,
}
