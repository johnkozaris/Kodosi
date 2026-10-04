use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use super::{Result, invalid};

pub(crate) fn pack(body: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut stream = Compress::new(Compression::new(level), false);
    let mut packed = Vec::with_capacity(body.len() / 4 + 256);
    deflate(&mut stream, body, &mut packed)?;
    Ok(packed)
}

pub(crate) fn unpack(packed: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    inflate(&mut Decompress::new(false), packed, &mut output, limit)?;
    Ok(output)
}

fn consumed(before: u64, after: u64) -> Result<usize> {
    usize::try_from(after - before).map_err(|_| invalid("Terminal data is too large."))
}

fn deflate(stream: &mut Compress, mut input: &[u8], output: &mut Vec<u8>) -> Result<()> {
    loop {
        if output.len() == output.capacity() {
            output.reserve(output.len().max(256));
        }
        let before = stream.total_in();
        let status = stream
            .compress_vec(input, output, FlushCompress::Finish)
            .map_err(|_| invalid("Terminal data could not be packed."))?;
        input = &input[consumed(before, stream.total_in())?..];
        if status == Status::StreamEnd {
            return Ok(());
        }
    }
}

fn inflate(
    stream: &mut Decompress,
    mut input: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
) -> Result<()> {
    let corrupt = || invalid("Remote terminal data could not be unpacked.");
    let room = limit.saturating_add(1);
    output.reserve_exact(input.len().saturating_mul(4).saturating_add(256).min(room));
    loop {
        if output.len() == output.capacity() {
            output.reserve_exact(output.len().max(256).min(room - output.len()));
        }
        let (before, produced) = (stream.total_in(), output.len());
        let status = stream
            .decompress_vec(input, output, FlushDecompress::Sync)
            .map_err(|_| corrupt())?;
        if output.len() > limit {
            return Err(invalid("Remote terminal data exceeds its bound."));
        }
        let used = consumed(before, stream.total_in())?;
        input = &input[used..];
        if status == Status::StreamEnd {
            return if input.is_empty() {
                Ok(())
            } else {
                Err(corrupt())
            };
        }
        if input.is_empty() && output.len() < output.capacity() {
            return Err(corrupt());
        }
        if used == 0 && output.len() == produced && output.len() < output.capacity() {
            return Err(corrupt());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_frame_is_packed_alone_so_an_earlier_frame_changes_nothing() {
        let line = b"\x1b[38;2;215;119;87mwarning\x1b[0m: unused variable in terminal output\r\n";
        let first = pack(line, 3).unwrap();
        let second = pack(line, 3).unwrap();
        assert_eq!(first, second);
        assert_eq!(unpack(&second, 1024).unwrap(), line);
    }

    #[test]
    fn unpacked_data_cannot_exceed_its_bound_or_be_truncated() {
        let body = vec![b'x'; 1024 * 1024];
        let packed = pack(&body, 6).unwrap();
        assert!(packed.len() < 4096);
        assert_eq!(unpack(&packed, body.len()).unwrap(), body);
        assert!(unpack(&packed, body.len() - 1).is_err());
        assert!(unpack(&packed[..packed.len() - 1], body.len()).is_err());
    }
}
