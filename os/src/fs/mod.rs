//! File trait & inode(dir, file, pipe, stdin, stdout)

mod inode;
mod stdio;

use crate::mm::UserBuffer;

/// trait File for all file types
pub trait File: Send + Sync {
    /// the file readable?
    fn readable(&self) -> bool;
    /// the file writable?
    fn writable(&self) -> bool;
    /// read from the file to buf, return the number of bytes read
    fn read(&self, buf: UserBuffer) -> usize;
    /// write to the file from buf, return the number of bytes written
    fn write(&self, buf: UserBuffer) -> usize;
}

/// The stat of a inode
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    pub dev: u64,
    /// inode number
    pub ino: u64,
    /// file type and mode
    pub mode: StatMode,
    /// number of hard links
    pub nlink: u32,
    /// path links
    pub links: Vec<Option<String>>,
    /// unused pad
    pad: [u64; 7],
}

/// Stat
impl Stat {
    /// Create new stat
    pub fn new() -> Self {
        Stat {
            dev: 0,
            ino: 0,
            mode: StatMode::NULL,
            nlink: 0,
            links: Vec::new(),
            pad: [0; 7]
        }
    }
    /// From args
    pub fn from_args(ino: u64, mode: StatMode, nlink: u32, links: Vec<Option<String>>) -> Self {
        debug!("Stat from args: {} -> {:?}", ino, mode);
        Stat {
            dev: 0,
            ino: ino,
            mode: mode,
            nlink: nlink,
            links: links,
            pad: [0; 7],
        }
    }
    /// link
    pub fn link(&mut self, path: String) {
        self.nlink += 1;
        self.links.push(Some(path));
    }
    /// unlink
    pub fn unlikn(&mut self, path: String) {
        let index: Option<usize> = self.links.iter().enumerate().find_map(|(i, l)| {
            if let Some(_path) = l {
                if _path.eq(&path) {
                    return Some(i);
                }
            }
            None
        });

        if index.is_none() {
            return;
        }

        self.nlink -= 1;
        self.links.remove(index.unwrap());
    }
    /// exist link
    pub fn exist_link(&self, path: String) -> bool {
        if self.nlink == 0 {
            return false;
        };
        for link in self.links.clone() {
            if let Some(_path) = link {
                if _path == path {
                    return true;
                };
            };
        }
        return false;
    }
}

bitflags! {
    /// The mode of a inode
    /// whether a directory or a file
    pub struct StatMode: u32 {
        /// null
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

use alloc::{string::String, vec::Vec};
pub use inode::{list_apps, open_file, OSInode, OpenFlags};
pub use stdio::{Stdin, Stdout};
