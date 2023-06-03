use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::io::{FromRawFd, IntoRawFd};

/// A file that is not closed upon leaving scope.
pub struct UnmanagedFile {
    inner: Option<File>,
}

impl UnmanagedFile {
    pub unsafe fn new(fd: u64) -> Self {
        Self {
            inner: Some(File::from_raw_fd(fd as i32)),
        }
    }
    // pub fn sync_all(&self) -> io::Result<()> {
    //     self.inner.as_ref().unwrap().sync_all()
    // }
    // pub fn sync_data(&self) -> io::Result<()> {
    //     self.inner.as_ref().unwrap().sync_data()
    // }
}

impl Drop for UnmanagedFile {
    fn drop(&mut self) {
        // Release control of the file descriptor so it is not closed.
        let file = self.inner.take().unwrap();
        file.into_raw_fd();
    }
}

impl Read for UnmanagedFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.as_ref().unwrap().read(buf)
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.inner.as_ref().unwrap().read_to_end(buf)
    }
}

impl Write for UnmanagedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.as_ref().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.as_ref().unwrap().flush()
    }
}

impl Seek for UnmanagedFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.as_ref().unwrap().seek(pos)
    }
}
