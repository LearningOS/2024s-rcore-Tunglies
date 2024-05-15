//! File and filesystem-related syscalls

use alloc::sync::Arc;
use alloc::vec;

use crate::fs::{open_file, OpenFlags, Stat, StatMode};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(&path, OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);

        if inner.st_table[fd].is_none() {
            let st = Stat::from_args(0, StatMode::FILE, 1, vec![Some(path.clone())]);
            inner.st_table[fd] = Some(Arc::new(st));
            return fd as isize;            
        } else {
            let st = inner.st_table[fd].clone().unwrap();
            if !st.exist_link(path.clone()) {
                return -1;
            }
            return fd as isize;
        }
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}


pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let ptr = _st as *const u8;
    let len = core::mem::size_of::<Stat>();
    let buffers = translated_byte_buffer(token, ptr, len);

    let binding = current_task().unwrap();
    let inner = binding.inner_exclusive_access();
    if _fd > inner.st_table.len() {
        return -1;
    }
    let _st = inner.st_table[_fd].clone().unwrap();
    let st = Stat::from_args(_st.ino, _st.mode, _st.nlink, _st.links.clone());

    let mut st_ptr = &st as *const _ as *const u8;
    unsafe {
        for buffer in buffers {
            st_ptr.copy_to(buffer.as_mut_ptr(), buffer.len());
            st_ptr = st_ptr.add(buffer.len());
        }
    }
    0
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let token = current_user_token();
    // let binding = current_task().unwrap();
    // let mut inner = binding.inner_exclusive_access();

    // let old_path = translated_str(token, _old_name);
    // let new_path = translated_str(token, _new_name);

    // if let Some(index) = inner.find_linked_index(old_path.clone()) {
    //     let _stat = inner.st_table[index].clone().unwrap();
    //     let mut stat = Stat::from_args(_stat.ino, _stat.mode, _stat.nlink, _stat.links.clone());
    //     stat.link(new_path);
    //     inner.st_table[index] = Some(Arc::new(stat));
    //     return 0;
    // } else {
    //     return -1;
    // }
    -1
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let token = current_user_token();
    // let binding = current_task().unwrap();
    // let mut inner = binding.inner_exclusive_access();
    
    // let path = translated_str(token, _name);
    // if let Some(index) = inner.find_linked_index(path.clone()) {
    //     let _stat = inner.st_table[index].clone().unwrap();
    //     let mut stat = Stat::from_args(_stat.ino, _stat.mode, _stat.nlink, _stat.links.clone());
    //     stat.unlikn(path);
    //     inner.st_table[index] = Some(Arc::new(stat));
    //     return 0;
    // } else {
    //     return -1;
    // }
    -1
}
