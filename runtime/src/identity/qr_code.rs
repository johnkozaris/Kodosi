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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_square_rows_with_dark_ones_and_no_quiet_zone() {
        let longest_invite = format!("kodosi:{}:{}", "a".repeat(64), "ab".repeat(32));
        let code = rows(&longest_invite).unwrap();
        assert!(code.iter().all(|row| row.len() == code.len()
            && row.bytes().all(|module| matches!(module, b'0' | b'1'))));
        assert!(code[0].starts_with("11111110") && code[0].ends_with("01111111"));
        assert!(code[1].starts_with("10000010") && code[1].ends_with("01000001"));
    }
}
