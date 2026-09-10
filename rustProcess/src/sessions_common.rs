use std::borrow::Cow;

use crate::{runtime::MAX_SESSION_TITLE_LEN, session_runtime::commands::SessionInput};

pub(crate) fn normalized_session_title(requested_title: &str) -> crate::Result<String> {
    let trimmed = requested_title.trim();
    if trimmed.is_empty() {
        return Err(crate::AppError::Unsupported {
            reason: "session titles cannot be blank".to_owned(),
        });
    }
    Ok(if trimmed.len() > MAX_SESSION_TITLE_LEN {
        trimmed[..trimmed.floor_char_boundary(MAX_SESSION_TITLE_LEN)].to_owned()
    } else {
        trimmed.to_owned()
    })
}

pub(crate) fn session_input_bytes(input: &SessionInput) -> Cow<'_, [u8]> {
    Cow::Borrowed(input.as_bytes())
}

pub(crate) const fn session_input_command_name(_input: &SessionInput) -> &'static str {
    "session.inputBytes"
}

#[cfg(test)]
mod tests {
    use super::session_input_bytes;
    use crate::session_runtime::commands::SessionInput;

    #[test]
    fn terminal_input_bytes_preserve_every_raw_byte() {
        let all_bytes = (0_u8..=u8::MAX).collect::<Vec<_>>();
        assert_eq!(
            session_input_bytes(&SessionInput::new(all_bytes.clone())),
            all_bytes
        );
    }
}
