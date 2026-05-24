use super::embedded::{
    APP_JS_PARTS, EVERSCALE_LOGO_SVG, INDEX_HTML, JOKES_JSON, SMOKING_MAN_PNG, STYLES_CSS,
    TON_LOGO_SVG, TYCHO_LOGO_SVG,
};

pub(in crate::server) fn asset_version() -> String {
    let mut hash = Fnv1a64::new();
    hash.update(INDEX_HTML.as_bytes());
    hash.update(STYLES_CSS.as_bytes());
    for part in APP_JS_PARTS {
        hash.update(part.as_bytes());
    }
    hash.update(EVERSCALE_LOGO_SVG.as_bytes());
    hash.update(TYCHO_LOGO_SVG.as_bytes());
    hash.update(TON_LOGO_SVG.as_bytes());
    hash.update(SMOKING_MAN_PNG);
    hash.update(JOKES_JSON.as_bytes());

    format!("{}-{:016x}", env!("CARGO_PKG_VERSION"), hash.finish())
}

struct Fnv1a64 {
    value: u64,
}

impl Fnv1a64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    fn new() -> Self {
        Self {
            value: Self::OFFSET,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(Self::PRIME);
        }
        self.value ^= 0xff;
        self.value = self.value.wrapping_mul(Self::PRIME);
    }

    fn finish(self) -> u64 {
        self.value
    }
}
