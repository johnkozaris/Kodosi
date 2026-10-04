use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use super::{Result, invalid};

const STREAM_LEVEL: u32 = 3;
const SNAPSHOT_LEVEL: u32 = 6;

pub(crate) struct OutputPacker {
    stream: Compress,
    restart: bool,
}

impl OutputPacker {
    pub(crate) fn new() -> Self {
        Self {
            stream: Compress::new(Compression::new(STREAM_LEVEL), false),
            restart: true,
        }
    }

    pub(crate) const fn restart(&mut self) {
        self.restart = true;
    }

    pub(crate) fn pack(&mut self, body: &[u8]) -> Result<Vec<u8>> {
        let mut packed = Vec::with_capacity(body.len() / 2 + 64);
        packed.push(u8::from(self.restart));
        if std::mem::take(&mut self.restart) {
            self.stream.reset();
        }
        deflate(&mut self.stream, body, &mut packed, FlushCompress::Sync)?;
        Ok(packed)
    }
}

pub(crate) struct OutputUnpacker {
    stream: Decompress,
    started: bool,
}

impl Default for OutputUnpacker {
    fn default() -> Self {
        Self {
            stream: Decompress::new(false),
            started: false,
        }
    }
}

impl OutputUnpacker {
    pub(crate) const fn stop(&mut self) {
        self.started = false;
    }

    pub(crate) fn unpack(&mut self, packed: &[u8], limit: usize) -> Result<Vec<u8>> {
        let (restart, packed) = match packed.split_first() {
            Some((0, packed)) => (false, packed),
            Some((1, packed)) => (true, packed),
            _ => return Err(invalid("Remote output has an invalid stream marker.")),
        };
        if restart {
            self.stream.reset(false);
            self.started = true;
        } else if !self.started {
            return Err(invalid(
                "Remote output did not restart its stream at the snapshot.",
            ));
        }
        let mut output = Vec::new();
        if let Err(error) = inflate(&mut self.stream, packed, &mut output, limit, false) {
            self.started = false;
            return Err(error);
        }
        Ok(output)
    }
}

pub(crate) fn pack_snapshot(body: &[u8]) -> Result<Vec<u8>> {
    let mut stream = Compress::new(Compression::new(SNAPSHOT_LEVEL), false);
    let mut packed = Vec::with_capacity(body.len() / 16 + 256);
    deflate(&mut stream, body, &mut packed, FlushCompress::Finish)?;
    Ok(packed)
}

pub(crate) fn unpack_snapshot(packed: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    inflate(
        &mut Decompress::new(false),
        packed,
        &mut output,
        limit,
        true,
    )?;
    Ok(output)
}

fn consumed(before: u64, after: u64) -> Result<usize> {
    usize::try_from(after - before).map_err(|_| invalid("Terminal data is too large."))
}

fn deflate(
    stream: &mut Compress,
    mut input: &[u8],
    output: &mut Vec<u8>,
    flush: FlushCompress,
) -> Result<()> {
    let finish = matches!(flush, FlushCompress::Finish);
    loop {
        if output.len() == output.capacity() {
            output.reserve(output.len().max(256));
        }
        let before = stream.total_in();
        let status = stream
            .compress_vec(input, output, flush)
            .map_err(|_| invalid("Terminal data could not be packed."))?;
        input = &input[consumed(before, stream.total_in())?..];
        let complete = if finish {
            status == Status::StreamEnd
        } else {
            input.is_empty() && output.len() < output.capacity()
        };
        if complete {
            return Ok(());
        }
    }
}

fn inflate(
    stream: &mut Decompress,
    mut input: &[u8],
    output: &mut Vec<u8>,
    limit: usize,
    finish: bool,
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
            return if finish && input.is_empty() {
                Ok(())
            } else {
                Err(corrupt())
            };
        }
        if input.is_empty() && output.len() < output.capacity() {
            return if finish { Err(corrupt()) } else { Ok(()) };
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
    fn output_stream_reuses_earlier_frames_and_restarts_on_request() {
        let mut packer = OutputPacker::new();
        let mut unpacker = OutputUnpacker::default();
        let line = b"\x1b[38;2;215;119;87mwarning\x1b[0m: unused variable in terminal output\r\n";
        let first = packer.pack(line).unwrap();
        let second = packer.pack(line).unwrap();
        assert_eq!((first[0], second[0]), (1, 0));
        assert!(second.len() < first.len() / 2);
        assert_eq!(unpacker.unpack(&first, 1024).unwrap(), line);
        assert_eq!(unpacker.unpack(&second, 1024).unwrap(), line);

        let mut late = OutputUnpacker::default();
        assert!(late.unpack(&second, 1024).is_err());
        packer.restart();
        let restarted = packer.pack(line).unwrap();
        assert_eq!(restarted[0], 1);
        assert_eq!(late.unpack(&restarted, 1024).unwrap(), line);
        assert_eq!(unpacker.unpack(&restarted, 1024).unwrap(), line);
    }

    #[test]
    fn unpacked_data_cannot_exceed_its_bound_or_be_truncated() {
        let body = vec![b'x'; 1024 * 1024];
        let packed = pack_snapshot(&body).unwrap();
        assert!(packed.len() < 4096);
        assert_eq!(unpack_snapshot(&packed, body.len()).unwrap(), body);
        assert!(unpack_snapshot(&packed, body.len() - 1).is_err());
        assert!(unpack_snapshot(&packed[..packed.len() - 1], body.len()).is_err());

        let frame = OutputPacker::new().pack(&body).unwrap();
        assert!(
            OutputUnpacker::default()
                .unpack(&frame, body.len() - 1)
                .is_err()
        );
        assert_eq!(
            OutputUnpacker::default()
                .unpack(&frame, body.len())
                .unwrap(),
            body
        );
    }
}
