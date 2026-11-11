#![allow(dead_code, unused_imports)]

mod install_fixture;
mod mock_call;
mod mock_storage;

pub use install_fixture::{InstallFixture, pattern};
pub use mock_call::MockCall;
pub use mock_storage::MockStorage;
