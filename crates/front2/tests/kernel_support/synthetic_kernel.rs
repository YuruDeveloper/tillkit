pub const CURRENT_RELEASE: &str = "5.15.41-android13-8-g0c4c3f721017-ab475";
pub const NEW_RELEASE: &str = "5.15.167-android13-8-gabcdef012345-ab999";
pub const KMI: &str = "5.15-android13-8";
pub const FORMAT_STRING: &str = "Linux version %s (%s)";
pub const OS_VERSION: u32 = 436_207_986;

const ARM64_MAGIC_OFFSET: usize = 0x38;
const FIRST_TEXT_OFFSET: usize = 8192;
const TEXT_SPACING: usize = 1024;

pub struct SyntheticKernel {
    pub bytes: usize,
    pub fill: u8,
    pub arm64_magic: bool,
    pub format_string: bool,
    pub releases: Vec<String>,
}

impl SyntheticKernel {
    pub fn with_release(release: &str, bytes: usize, fill: u8) -> Self {
        Self {
            bytes,
            fill,
            arm64_magic: true,
            format_string: true,
            releases: vec![release.to_string()],
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut data = vec![self.fill; self.bytes];
        data[..64].fill(0);
        if self.arm64_magic {
            data[ARM64_MAGIC_OFFSET..ARM64_MAGIC_OFFSET + 4].copy_from_slice(b"ARMd");
        }
        let mut texts: Vec<String> = Vec::new();
        texts.extend(self.format_string.then(|| FORMAT_STRING.to_string()));
        texts.extend(self.releases.iter().map(|release| banner(release)));
        for (index, text) in texts.iter().enumerate() {
            place(&mut data, FIRST_TEXT_OFFSET + index * TEXT_SPACING, text);
        }
        data
    }

    pub fn boot_image(&self) -> Vec<u8> {
        boot_image(&self.build())
    }
}

pub fn banner(release: &str) -> String {
    format!(
        "Linux version {release} (build-user@build-host) (Android clang version 14.0.7, LLD 14.0.7) #1 SMP PREEMPT Thu Dec 4 02:47:30 UTC 2025\n"
    )
}

pub fn boot_image(kernel: &[u8]) -> Vec<u8> {
    let mut data = vec![0u8; 4096 + kernel.len().div_ceil(4096) * 4096];
    data[..8].copy_from_slice(b"ANDROID!");
    data[8..12].copy_from_slice(&(kernel.len() as u32).to_le_bytes());
    data[16..20].copy_from_slice(&OS_VERSION.to_le_bytes());
    data[20..24].copy_from_slice(&1584u32.to_le_bytes());
    data[40..44].copy_from_slice(&4u32.to_le_bytes());
    data[4096..4096 + kernel.len()].copy_from_slice(kernel);
    data
}

fn place(data: &mut [u8], offset: usize, text: &str) {
    data[offset - 1] = 0;
    data[offset..offset + text.len()].copy_from_slice(text.as_bytes());
    data[offset + text.len()] = 0;
}
