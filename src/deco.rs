use fuse_mt::{
    CallbackResult, DirectoryEntry, FileAttr, FileType, FilesystemMT, RequestInfo, ResultEmpty,
    ResultEntry, ResultOpen, ResultReaddir, ResultSlice, ResultStatfs, Statfs,
};
use libc::ENOENT;
use std::convert::TryInto;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::libc_wrapper;
use crate::unmanaged_file;

static TTL: Duration = Duration::from_secs(1);

pub struct DecoFS {
    sourceroot: PathBuf,
}

impl DecoFS {
    pub fn new(sourceroot: PathBuf) -> Self {
        Self { sourceroot }
    }

    fn real_path(&self, partial: &Path) -> PathBuf {
        PathBuf::from(&self.sourceroot).join(partial.strip_prefix("/").unwrap())
    }

    fn statfs_real(&self, path: &Path) -> io::Result<libc::statfs> {
        let real = self.real_path(path);
        libc_wrapper::statfs(&real)
    }

    fn stat_real(&self, path: &Path) -> io::Result<FileAttr> {
        let real = self.real_path(path);
        let stat = libc_wrapper::lstat(&real)?;
        Ok(Self::stat_to_fuse(stat))
    }

    fn mode_to_filetype(mode: libc::mode_t) -> FileType {
        match mode & libc::S_IFMT {
            libc::S_IFDIR => FileType::Directory,
            libc::S_IFREG => FileType::RegularFile,
            libc::S_IFLNK => FileType::Symlink,
            libc::S_IFBLK => FileType::BlockDevice,
            libc::S_IFCHR => FileType::CharDevice,
            libc::S_IFIFO => FileType::NamedPipe,
            libc::S_IFSOCK => FileType::Socket,
            _ => {
                panic!("unknown file type");
            }
        }
    }

    fn statfs_to_fuse(statfs: libc::statfs) -> Statfs {
        Statfs {
            blocks: statfs.f_blocks,
            bfree: statfs.f_bfree,
            bavail: statfs.f_bavail,
            files: statfs.f_files,
            ffree: statfs.f_ffree,
            bsize: statfs.f_bsize as u32,
            namelen: statfs.f_namelen as u32,
            frsize: statfs.f_frsize as u32,
        }
    }

    fn stat_to_fuse(stat: libc::stat) -> FileAttr {
        // st_mode encodes both the kind and the permissions
        let kind = DecoFS::mode_to_filetype(stat.st_mode);
        let perm = (stat.st_mode & 0o7777) as u16;

        FileAttr {
            size: stat.st_size as u64,
            blocks: stat.st_blocks as u64,
            atime: SystemTime::UNIX_EPOCH
                + Duration::from_secs(stat.st_atime.try_into().unwrap())
                + Duration::from_nanos(stat.st_atime_nsec.try_into().unwrap()),
            mtime: SystemTime::UNIX_EPOCH
                + Duration::from_secs(stat.st_mtime.try_into().unwrap())
                + Duration::from_nanos(stat.st_mtime_nsec.try_into().unwrap()),
            ctime: SystemTime::UNIX_EPOCH
                + Duration::from_secs(stat.st_ctime.try_into().unwrap())
                + Duration::from_nanos(stat.st_ctime_nsec.try_into().unwrap()),
            crtime: SystemTime::UNIX_EPOCH,
            kind,
            perm,
            nlink: stat.st_nlink as u32,
            uid: stat.st_uid,
            gid: stat.st_gid,
            rdev: stat.st_rdev as u32,
            flags: 0,
        }
    }

    fn stat_to_filetype(stat: &libc::stat) -> FileType {
        Self::mode_to_filetype(stat.st_mode)
    }
}

impl FilesystemMT for DecoFS {
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        info!("init");
        Ok(())
    }

    fn destroy(&self) {
        info!("destroy");
    }

    fn getattr(&self, _req: RequestInfo, path: &Path, fh: Option<u64>) -> ResultEntry {
        debug!("getattr: {:?}", path);
        if let Some(fh) = fh {
            match libc_wrapper::fstat(fh) {
                Ok(stat) => Ok((TTL, Self::stat_to_fuse(stat))),
                Err(e) => Err(e.raw_os_error().unwrap_or(ENOENT)),
            }
        } else {
            match self.stat_real(path) {
                Ok(attr) => Ok((TTL, attr)),
                Err(e) => Err(e.raw_os_error().unwrap_or(ENOENT)),
            }
        }
    }

    fn statfs(&self, _req: RequestInfo, path: &Path) -> ResultStatfs {
        debug!("statfs: {:?}", path);

        match self.statfs_real(path) {
            Ok(stat) => Ok(Self::statfs_to_fuse(stat)),
            Err(e) => Err(e.raw_os_error().unwrap_or(ENOENT)),
        }
    }

    fn opendir(&self, _req: RequestInfo, path: &Path, flags: u32) -> ResultOpen {
        let real = self.real_path(path);
        debug!("opendir: {:?} {:?} (flags = {:#o})", path, real, flags);
        Ok((0, 0))
    }

    fn readdir(&self, _req: RequestInfo, path: &Path, _fh: u64) -> ResultReaddir {
        let real = self.real_path(path);
        debug!("readdir: {:?} {:?}", path, real);
        let mut entries: Vec<DirectoryEntry> = vec![];
        // Consider using libc::readdir to prevent need for always stat-ing entries
        let iter = match fs::read_dir(&real) {
            Ok(iter) => iter,
            Err(e) => return Err(e.raw_os_error().unwrap_or(ENOENT)),
        };
        for entry in iter {
            match entry {
                Ok(entry) => {
                    let real_path = entry.path();
                    debug!("readdir: {:?} {:?}", real, real_path);
                    let stat = match libc_wrapper::lstat(&real_path) {
                        Ok(stat) => stat,
                        Err(e) => return Err(e.raw_os_error().unwrap_or(ENOENT)),
                    };
                    let filetype = DecoFS::stat_to_filetype(&stat);

                    entries.push(DirectoryEntry {
                        name: real_path.file_name().unwrap().to_os_string(),
                        kind: filetype,
                    });
                }
                Err(e) => {
                    error!("readdir: {:?}: {}", path, e);
                    return Err(e.raw_os_error().unwrap_or(ENOENT));
                }
            }
        }
        info!("entries: {:?}", entries);
        Ok(entries)
    }

    fn releasedir(&self, _req: RequestInfo, path: &Path, _fh: u64, flags: u32) -> ResultEmpty {
        let real = self.real_path(path);
        debug!("opendir: {:?} {:?} (flags = {:#o})", path, real, flags);
        Ok(())
    }

    fn open(&self, _req: RequestInfo, path: &Path, flags: u32) -> ResultOpen {
        let real = self.real_path(path);
        debug!("open: {:?} {:?} flags={:#x}", path, real, flags);

        match libc_wrapper::open(&real, flags as libc::c_int) {
            Ok(fh) => Ok((fh, flags)),
            Err(e) => {
                error!("readdir: {:?}: {}", path, e);
                Err(e.raw_os_error().unwrap_or(ENOENT))
            }
        }
    }

    fn release(
        &self,
        _req: RequestInfo,
        path: &Path,
        fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> ResultEmpty {
        debug!("release: {:?}", path);
        match libc_wrapper::close(fh) {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("open({:?}): {}", path, e);
                Err(e.raw_os_error().unwrap_or(ENOENT))
            }
        }
    }

    fn read(
        &self,
        _req: RequestInfo,
        path: &Path,
        fh: u64,
        offset: u64,
        size: u32,
        callback: impl FnOnce(ResultSlice<'_>) -> CallbackResult,
    ) -> CallbackResult {
        debug!("read: {:?} {:#x} @ {:#x}", path, size, offset);
        let mut file = unsafe { unmanaged_file::UnmanagedFile::new(fh) };

        let mut data = Vec::<u8>::new();
        data.resize(size as usize, 0);

        if let Err(e) = file.seek(SeekFrom::Start(offset)) {
            error!("seek({:?}, {}): {}", path, offset, e);
            callback(Err(e.raw_os_error().unwrap_or(ENOENT)))
        } else {
            match file.read(&mut data) {
                Ok(n) => {
                    data.truncate(n);
                    callback(Ok(&data))
                }
                Err(e) => {
                    error!("read {:?}, {:#x} @ {:#x}: {}", path, size, offset, e);
                    callback(Err(e.raw_os_error().unwrap_or(ENOENT)))
                }
            }
        }
    }
}
