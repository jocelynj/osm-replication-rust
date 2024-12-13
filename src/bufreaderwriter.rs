// Original code from:
// - https://crates.io/crates/bufreaderwriter
// - https://github.com/alemigo/bufreaderwriter-rs

#![allow(dead_code)]
#![allow(clippy::match_wildcard_for_single_variants)]

use std::io::{self, BufReader, BufWriter, IntoInnerError, Read, Seek, SeekFrom, Write};

enum BufIO<RW: Read + Write + Seek> {
    Reader(BufReader<RW>),
    Writer(BufWriter<RW>),
}

impl<RW: Read + Write + Seek> BufIO<RW> {
    fn new_writer(rw: RW, capacity: Option<usize>) -> BufIO<RW> {
        BufIO::Writer(match capacity {
            Some(c) => BufWriter::with_capacity(c, rw),
            None => BufWriter::new(rw),
        })
    }

    fn new_reader(rw: RW, capacity: Option<usize>) -> BufIO<RW> {
        BufIO::Reader(match capacity {
            Some(c) => BufReader::with_capacity(c, rw),
            None => BufReader::new(rw),
        })
    }

    fn get_mut(&mut self) -> &mut RW {
        match self {
            BufIO::Reader(r) => r.get_mut(),
            BufIO::Writer(w) => w.get_mut(),
        }
    }

    fn get_ref(&self) -> &RW {
        match self {
            BufIO::Reader(r) => r.get_ref(),
            BufIO::Writer(w) => w.get_ref(),
        }
    }

    fn into_inner(self) -> Result<RW, IntoInnerError<BufWriter<RW>>> {
        match self {
            BufIO::Reader(r) => Ok(r.into_inner()),
            BufIO::Writer(w) => Ok(w.into_inner()?),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            BufIO::Reader(r) => r.capacity(),
            BufIO::Writer(w) => w.capacity(),
        }
    }

    pub fn seek_relative(&mut self, offset: i64) -> io::Result<()> {
        match self {
            BufIO::Reader(r) => r.seek_relative(offset),
            BufIO::Writer(w) => w.seek_relative(offset),
        }
    }
}

pub struct BufReaderWriterRand<RW: Read + Write + Seek> {
    inner: Option<BufIO<RW>>,
    capacity: Option<usize>,
}

impl<RW: Read + Write + Seek> BufReaderWriterRand<RW> {
    /// Returns a new BufReaderWriterRand instance, expecting a write as the first operation.
    pub fn new_writer(rw: RW) -> BufReaderWriterRand<RW> {
        BufReaderWriterRand {
            inner: Some(BufIO::new_writer(rw, None)),
            capacity: None,
        }
    }

    /// Returns a new BufReaderWriterRand instance, expecting a write as the first operation, with specified buffer capacity.
    pub fn writer_with_capacity(capacity: usize, rw: RW) -> BufReaderWriterRand<RW> {
        BufReaderWriterRand {
            inner: Some(BufIO::new_writer(rw, Some(capacity))),
            capacity: Some(capacity),
        }
    }

    /// Returns a new BufReaderWriter instance, expecting a read as the first operation.
    pub fn new_reader(rw: RW) -> BufReaderWriterRand<RW> {
        BufReaderWriterRand {
            inner: Some(BufIO::new_reader(rw, None)),
            capacity: None,
        }
    }

    /// Returns a new BufReaderWriter instance, expecting a read as the first operation, with specified buffer capacity.
    pub fn reader_with_capacity(capacity: usize, rw: RW) -> BufReaderWriterRand<RW> {
        BufReaderWriterRand {
            inner: Some(BufIO::new_reader(rw, Some(capacity))),
            capacity: Some(capacity),
        }
    }

    /// Gets a mutable reference to the underlying reader/writer.
    pub fn get_mut(&mut self) -> &mut RW {
        self.inner.as_mut().unwrap().get_mut()
    }

    /// Gets a reference to the underlying reader/writer.
    pub fn get_ref(&self) -> &RW {
        self.inner.as_ref().unwrap().get_ref()
    }

    /// Unwraps this `BufReaderWriter`, returning the underlying reader/writer.  Note: the `BufReaderWriter` should be dropped after using this.
    pub fn into_inner(self) -> Result<RW, IntoInnerError<BufWriter<RW>>> {
        self.inner.unwrap().into_inner()
    }

    /// Returns true if the `BufReaderWriter` in read mode, otherwise false for write mode.
    pub fn is_reader(&self) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match self.inner.as_ref().unwrap() {
            BufIO::Reader(_) => true,
            _ => false,
        }
    }

    /// Gets a reference to the underlying buffered reader, available if in read mode.
    pub fn get_bufreader_ref(&self) -> Option<&BufReader<RW>> {
        match self.inner.as_ref().unwrap() {
            BufIO::Reader(r) => Some(r),
            _ => None,
        }
    }

    /// Gets a mutable reference to the underlying buffered reader, available if in read mode.
    pub fn get_bufreader_mut(&mut self) -> Option<&mut BufReader<RW>> {
        match self.inner.as_mut().unwrap() {
            BufIO::Reader(r) => Some(r),
            _ => None,
        }
    }

    /// Unwraps this `BufReaderWriter` returning the BufReader, available if in read mode.  Note: the `BufReaderWriter` should be dropped after using this.
    pub fn into_bufreader(self) -> Option<BufReader<RW>> {
        match self.inner.unwrap() {
            BufIO::Reader(r) => Some(r),
            _ => None,
        }
    }

    /// Gets a reference to the underlying buffered writer, available if in write mode.
    pub fn get_bufwriter_ref(&self) -> Option<&BufWriter<RW>> {
        match self.inner.as_ref().unwrap() {
            BufIO::Writer(w) => Some(w),
            _ => None,
        }
    }

    /// Gets a mutable reference to the underlying buffered writer, available if in write mode.
    pub fn get_bufwriter_mut(&mut self) -> Option<&mut BufWriter<RW>> {
        match self.inner.as_mut().unwrap() {
            BufIO::Writer(w) => Some(w),
            _ => None,
        }
    }

    /// Unwraps this `BufReaderWriter` returning the `BufWriter`, available if in read mode.  Note: the `BufReaderWriter` should be dropped after using this.
    pub fn into_bufwriter(self) -> Option<BufWriter<RW>> {
        match self.inner.unwrap() {
            BufIO::Writer(w) => Some(w),
            _ => None,
        }
    }

    /// Returns the buffer capacity of the underlying reader or writer.
    pub fn capacity(&self) -> usize {
        #[allow(clippy::redundant_closure_for_method_calls)]
        self.inner.as_ref().map_or(0, |b| b.capacity())
    }
}

impl<RW: Read + Write + Seek> Read for BufReaderWriterRand<RW> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.as_mut().unwrap() {
            BufIO::Reader(r) => r.read(buf),
            BufIO::Writer(w) => {
                w.flush()?;
                let rw = self.inner.take().unwrap().into_inner()?;
                self.inner = match self.capacity {
                    Some(c) => Some(BufIO::Reader(BufReader::with_capacity(c, rw))),
                    None => Some(BufIO::Reader(BufReader::new(rw))),
                };
                self.read(buf)
            }
        }
    }
}

impl<RW: Read + Write + Seek> Write for BufReaderWriterRand<RW> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        #[allow(clippy::seek_from_current)]
        match self.inner.as_mut().unwrap() {
            BufIO::Writer(w) => w.write(buf),
            BufIO::Reader(r) => {
                r.seek(SeekFrom::Current(0))?;
                let rw = self.inner.take().unwrap().into_inner()?;
                self.inner = match self.capacity {
                    Some(c) => Some(BufIO::Writer(BufWriter::with_capacity(c, rw))),
                    None => Some(BufIO::Writer(BufWriter::new(rw))),
                };
                self.write(buf)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.inner.as_mut() {
            Some(BufIO::Writer(w)) => Ok(w.flush()?),
            _ => Ok(()),
        }
    }
}

impl<RW: Read + Write + Seek> Seek for BufReaderWriterRand<RW> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self.inner.as_mut().unwrap() {
            BufIO::Writer(w) => w.seek(pos),
            BufIO::Reader(r) => r.seek(pos),
        }
    }
    fn stream_position(&mut self) -> io::Result<u64> {
        match self.inner.as_mut().unwrap() {
            BufIO::Writer(w) => {
                let buffer_len = w.buffer().len();
                w.get_mut().stream_position().map(|pos| {
                    pos.checked_add(buffer_len as u64)
                        .expect("overflow when adding buffer size to inner stream position")
                })
            }
            BufIO::Reader(r) => r.stream_position(),
        }
    }
}
