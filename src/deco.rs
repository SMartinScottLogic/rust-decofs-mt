use libc::{ENOENT};
use std::{fs, io};
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use fuse_mt::{FilesystemMT, FileAttr, FileType, DirectoryEntry, RequestInfo, ResultEmpty, ResultEntry, ResultOpen, ResultReaddir, ResultStatfs, Statfs};
use std::path::{Path, PathBuf};
use std::mem::MaybeUninit;
use time::Timespec;

const TTL: Timespec = Timespec { sec: 1, nsec: 0 };

pub struct DecoFS {
    sourceroot: OsString
}

impl DecoFS {
    pub fn new(sourceroot: OsString) -> DecoFS {
        DecoFS { sourceroot }
    }

    fn real_path(&self, partial: &Path) -> OsString {
        PathBuf::from(&self.sourceroot)
                .join(partial.strip_prefix("/").unwrap())
                .into_os_string()
    }

    fn stat(&self, path: &OsStr) -> io::Result<libc::stat> {
        let mut stat = MaybeUninit::<libc::stat>::uninit();

        let cstr = CString::new(path.as_bytes())?;
        let result = unsafe {
            libc::lstat(cstr.as_ptr(), stat.as_mut_ptr())
        };
        if -1 == result {
            let e = io::Error::last_os_error();
            error!("lstat({:?}): {}", path, e);
            Err(e)
        } else {
            let stat = unsafe {
                stat.assume_init()
            };
            Ok(stat)
        }
    }

    fn statfs_real(&self, path: &Path) -> io::Result<libc::statfs> {
        let real = self.real_path(path);
        let mut stat = MaybeUninit::<libc::statfs>::zeroed();

        let cstr = CString::new(real.as_bytes())?;
        let result = unsafe {
            libc::statfs(cstr.as_ptr(), stat.as_mut_ptr())
        };

        if -1 == result {
            let e = io::Error::last_os_error();
            error!("statfs({:?}): {}", path, e);
            Err(e)
        } else {
            let stat = unsafe {
                stat.assume_init()
            };
            Ok(stat)
        }
    }

    fn stat_real(&self, path: &Path) -> io::Result<FileAttr> {
        let real = self.real_path(path);
        let stat = self.stat(real.as_os_str())?;
        Ok(DecoFS::stat_to_fuse(stat))
    }

    fn mode_to_filetype(mode: libc::mode_t) -> FileType {
        match mode & libc::S_IFMT {
            libc::S_IFDIR => FileType::Directory,
            libc::S_IFREG => FileType::RegularFile,
            libc::S_IFLNK => FileType::Symlink,
            libc::S_IFBLK => FileType::BlockDevice,
            libc::S_IFCHR => FileType::CharDevice,
            libc::S_IFIFO  => FileType::NamedPipe,
            libc::S_IFSOCK => FileType::Socket,
            _ => { panic!("unknown file type"); }
        }
    }

    fn statfs_to_fuse(statfs: libc::statfs) -> Statfs {
        Statfs {
            blocks: statfs.f_blocks as u64,
            bfree: statfs.f_bfree as u64,
            bavail: statfs.f_bavail as u64,
            files: statfs.f_files as u64,
            ffree: statfs.f_ffree as u64,
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
            atime: Timespec { sec: stat.st_atime as i64, nsec: stat.st_atime_nsec as i32 },
            mtime: Timespec { sec: stat.st_mtime as i64, nsec: stat.st_mtime_nsec as i32 },
            ctime: Timespec { sec: stat.st_ctime as i64, nsec: stat.st_ctime_nsec as i32 },
            crtime: Timespec { sec: 0, nsec: 0 },
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
        DecoFS::mode_to_filetype(stat.st_mode)
    }

    fn stat_fh(fh: u64) -> io::Result<libc::stat> {
        let mut stat = MaybeUninit::<libc::stat>::uninit();

        let result = unsafe {
            libc::fstat(fh as libc::c_int, stat.as_mut_ptr())
        };
        if -1 == result {
            let e = io::Error::last_os_error();
            error!("fstat({:?}): {}", fh, e);
            Err(e)
        } else {
            let stat = unsafe {
                stat.assume_init()
            };
            Ok(stat)
        }

    }
}

impl FilesystemMT for DecoFS {
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        info!("init");
        Ok(())
    }

    fn destroy(&self, _req: RequestInfo) {
        info!("destroy");
    }

    fn getattr(&self, _req: RequestInfo, path: &Path, fh: Option<u64>) -> ResultEntry {
        debug!("getattr: {:?}", path);
        if let Some(fh) = fh {
            match DecoFS::stat_fh(fh) {
                Ok(stat) => Ok((TTL, DecoFS::stat_to_fuse(stat))),
                Err(e) => Err(e.raw_os_error().unwrap_or(ENOENT))
            }
        } else {
            match self.stat_real(path) {
                Ok(attr) => Ok((TTL, attr)),
                Err(e) => Err(e.raw_os_error().unwrap_or(ENOENT))
            }
        }
    }

    fn statfs(&self, _req: RequestInfo, path: &Path) -> ResultStatfs {
        debug!("statfs: {:?}", path);

        match self.statfs_real(path) {
            Ok(stat) => Ok(DecoFS::statfs_to_fuse(stat)),
            Err(e) => Err(e.raw_os_error().unwrap_or(ENOENT))
        }
    }

    fn opendir(&self, _req: RequestInfo, path: &Path, _flags: u32) -> ResultOpen {
        let real = self.real_path(path);
        debug!("opendir: {:?} {:?} (flags = {:#o})", path, real, _flags);
        Ok((0,0))
    }

    fn readdir(&self, _req: RequestInfo, path: &Path, _fh: u64) -> ResultReaddir {
        let real = self.real_path(path);
        debug!("readdir: {:?} {:?}", path, real);
        let mut entries: Vec<DirectoryEntry> = vec![];
        // Consider using libc::readdir to prevent need for always stat-ing entries
        let iter = match fs::read_dir(&real) {
            Ok(iter) => iter,
            Err(e) => return Err(e.raw_os_error().unwrap_or(ENOENT))
        };
        for entry in iter {
            match entry {
                Ok(entry) => {
                    let real_path = entry.path();
                    debug!("readdir: {:?} {:?}", real, real_path);
                    let stat = match self.stat(real_path.as_os_str()) {
                        Ok(stat) => stat,
                        Err(e) => return Err(e.raw_os_error().unwrap_or(ENOENT))
                    };
                    let filetype = DecoFS::stat_to_filetype(&stat);

                    entries.push(DirectoryEntry {
                        name: real_path.file_name().unwrap().to_os_string(),
                        kind: filetype,
                    });
                },
                Err(e) => {
                    error!("readdir: {:?}: {}", path, e);
                    return Err(e.raw_os_error().unwrap_or(ENOENT));
                }
            }
        }
        info!("entries: {:?}", entries);
        Ok(entries)
    }

    fn releasedir(&self, _req: RequestInfo, path: &Path, _fh: u64, _flags: u32) -> ResultEmpty {
        let real = self.real_path(path);
        debug!("opendir: {:?} {:?} (flags = {:#o})", path, real, _flags);
        Ok(())
    }
}
