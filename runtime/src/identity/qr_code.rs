use qrcodegen::{QrCode, QrCodeEcc};

pub(crate) fn rows(text: &str) -> Option<Vec<String>> {
    let code = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    Some(
        (0..code.size())
            .map(|y| {
                (0..code.size())
                    .map(|x| if code.get_module(x, y) { '1' } else { '0' })
                    .collect()
            })
            .collect(),
    )
}
