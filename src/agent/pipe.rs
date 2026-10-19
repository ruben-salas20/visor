//! The agent channel on Windows: a named pipe. Still to be written (W5).

use std::io::{self, Read, Write};
use std::path::Path;

#[derive(Debug)]
pub enum ClaimError {
    Running,
    Failed(String),
}

pub struct Claim;

impl Claim {
    pub fn release(self) {
        todo!("W5: named pipe")
    }
}

pub fn claim(_path: &Path) -> Result<(Listener, Claim), ClaimError> {
    todo!("W5: named pipe")
}

pub struct Listener;

impl Listener {
    pub fn incoming(&self) -> std::vec::IntoIter<io::Result<Stream>> {
        todo!("W5: named pipe")
    }
}

pub struct Stream;

impl Stream {
    pub fn connect(_path: &Path) -> io::Result<Stream> {
        todo!("W5: named pipe")
    }
}

impl Read for &Stream {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        todo!("W5: named pipe")
    }
}

impl Write for &Stream {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        todo!("W5: named pipe")
    }

    fn flush(&mut self) -> io::Result<()> {
        todo!("W5: named pipe")
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self).flush()
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}
