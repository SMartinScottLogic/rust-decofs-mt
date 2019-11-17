use libc::{ENOENT};
use std::{fs};
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use fuse_mt::{FilesystemMT, FileType, DirectoryEntry, RequestInfo, ResultEmpty, ResultOpen, ResultReaddir};
use std::path::{Path, PathBuf};
use std::mem::MaybeUninit;

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

    fn stat(&self, path: &OsStr) -> libc::stat {
        let mut stat = MaybeUninit::<libc::stat>::uninit();

        let cstr = CString::new(path.as_bytes()).unwrap();
        unsafe {
            libc::stat(cstr.as_ptr(), stat.as_mut_ptr()); 
            stat.assume_init()
        }
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

    fn stat_to_filetype(stat: &libc::stat) -> FileType {
        DecoFS::mode_to_filetype(stat.st_mode)
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
                    let path = entry.path();
                    let real_path = self.real_path(path.as_path());
                    let stat = self.stat(real_path.as_os_str());
                    let filetype = DecoFS::stat_to_filetype(&stat);

                    entries.push(DirectoryEntry {
                        name: path.as_os_str().to_os_string(),
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
