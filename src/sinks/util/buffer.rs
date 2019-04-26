use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    None,
    Gzip,
}

#[derive(Debug)]
pub struct Buffer {
    inner: InnerBuffer,
    num_items: usize,
}

#[derive(Debug)]
pub enum InnerBuffer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl Buffer {
    pub fn new(gzip: bool) -> Self {
        let inner = if gzip {
            InnerBuffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
        } else {
            InnerBuffer::Plain(Vec::new())
        };
        Self {
            inner,
            num_items: 0,
        }
    }

    pub fn push(&mut self, input: &[u8]) {
        self.num_items += 1;
        match &mut self.inner {
            InnerBuffer::Plain(inner) => {
                inner.extend_from_slice(input);
            }
            InnerBuffer::Gzip(inner) => {
                inner.write_all(input).unwrap();
            }
        }
    }

    // This is not guaranteed to be completely accurate as the gzip library does
    // some internal buffering.
    pub fn size(&self) -> usize {
        match &self.inner {
            InnerBuffer::Plain(inner) => inner.len(),
            InnerBuffer::Gzip(inner) => inner.get_ref().len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match &self.inner {
            InnerBuffer::Plain(inner) => inner.is_empty(),
            InnerBuffer::Gzip(inner) => inner.get_ref().is_empty(),
        }
    }
}

impl super::batch::Batch for Buffer {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn len(&self) -> usize {
        self.size()
    }

    fn push(&mut self, item: Self::Input) {
        self.push(&item)
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn fresh(&self) -> Self {
        let inner = match &self.inner {
            InnerBuffer::Plain(_) => InnerBuffer::Plain(Vec::new()),
            InnerBuffer::Gzip(_) => {
                InnerBuffer::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default()))
            }
        };
        Self {
            inner,
            num_items: 0,
        }
    }

    fn finish(self) -> Self::Output {
        match self.inner {
            InnerBuffer::Plain(inner) => inner,
            InnerBuffer::Gzip(inner) => inner
                .finish()
                .expect("This can't fail because the inner writer is a Vec"),
        }
    }

    fn num_items(&self) -> usize {
        self.num_items
    }
}

#[cfg(test)]
mod test {
    use super::Buffer;
    use crate::sinks::util::batch::{Batch, BatchSink};
    use futures::{Future, Sink};
    use std::io::Read;

    #[test]
    fn gzip() {
        use flate2::read::GzDecoder;

        let buffered = BatchSink::new(vec![], Buffer::new(true), 1000);

        let input = std::iter::repeat(
            b"It's going down, I'm yelling timber, You better move, you better dance".to_vec(),
        )
        .take(100_000);

        let (buffered, _) = buffered
            .send_all(futures::stream::iter_ok(input))
            .wait()
            .unwrap();

        let output = buffered
            .into_inner()
            .into_iter()
            .map(|buf| buf.finish())
            .collect::<Vec<Vec<u8>>>();

        assert!(output.len() > 1);
        assert!(output.iter().map(|o| o.len()).sum::<usize>() < 50_000);

        let decompressed = output.into_iter().flat_map(|batch| {
            let mut decompressed = vec![];
            GzDecoder::new(batch.as_slice())
                .read_to_end(&mut decompressed)
                .unwrap();
            decompressed
        });

        assert!(decompressed.eq(std::iter::repeat(
            b"It's going down, I'm yelling timber, You better move, you better dance".to_vec()
        )
        .take(100_000)
        .flatten()));
    }
}