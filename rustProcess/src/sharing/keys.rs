use zeroize::{Zeroize, Zeroizing};

use crate::{AppError, Result};

pub(crate) fn generate_owner_secret() -> Result<Zeroizing<String>> {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| {
        tracing::error!(%error, "getrandom failed while generating owner secret");
        AppError::Unsupported {
            reason: "platform RNG unavailable; cannot mint owner secret".to_owned(),
        }
    })?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    Ok(Zeroizing::new(encoded))
}
