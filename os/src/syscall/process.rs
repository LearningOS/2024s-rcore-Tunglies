//! Process management syscalls


use crate::{
    config::{MAX_SYSCALL_NUM, TIMEVAL}, mm::{translated_byte_buffer, MapPermission, VirtAddr}, task::{
        change_program_brk, current_is_mapped, current_mmp, current_task_status, current_task_syscalls, current_unmap, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus
    }, timer::{get_time_ms, get_time_us}
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    debug!("sys get time");
    trace!("kernel: sys_get_time");
    let token = current_user_token();
    let ptr = _ts as *const u8;
    let len = core::mem::size_of::<TimeVal>();
    let buffers = translated_byte_buffer(token, ptr, len);

    let us = get_time_us();
    let time_val = TimeVal {
        sec: us / TIMEVAL,
        usec: us % TIMEVAL
    };

    let mut time_ptr = &time_val as *const _ as *const u8;
    unsafe {
        for buffer in buffers {
            time_ptr.copy_to(buffer.as_mut_ptr(), buffer.len());
            time_ptr = time_ptr.add(buffer.len());
        }
    }

    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    debug!("sys task info");
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    let token = current_user_token();
    let ptr = _ti as *const u8;
    let len = core::mem::size_of::<TaskInfo>();
    let buffers = translated_byte_buffer(token, ptr, len);

    let task_info = TaskInfo {
        status: current_task_status(),
        syscall_times: current_task_syscalls(),
        time: get_time_ms()
    };

    let mut task_ptr = &task_info as *const _ as *const u8;
    unsafe {
        for buffer in buffers {
            task_ptr.copy_to(buffer.as_mut_ptr(), buffer.len());
            task_ptr = task_ptr.add(buffer.len());
        }
    }
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys mmap");
    debug!("start: {:?}, len: {:?}, port: {:?}", _start, _len, _port);
    let start_va = VirtAddr(_start);
    if !start_va.aligned() {
        return -1;
    }

    if _port & !0x7 !=0 || _port & 0x7 == 0 {
        return -1
    }

    let mut permissions = MapPermission::U;
    if _port & 0x1 != 0 {
        permissions.insert(MapPermission::R);
    }
    if _port & 0x2 != 0 {
        permissions.insert(MapPermission::W);
    }
    if _port & 0x4 != 0 {
        permissions.insert(MapPermission::X);
    }
    
    let end_va = VirtAddr(_start + _len); 
    if !current_is_mapped(start_va, end_va, false) {
        return -1;
    }
    current_mmp(start_va, end_va, permissions);
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys umap");
    let start_va = VirtAddr(_start);
    if !start_va.aligned() {
        return -1;
    }

    let end_va = VirtAddr(_start + _len);
    if !end_va.aligned() {
        return -1;
    }

    if !current_is_mapped(start_va, end_va, true) {
        return -1;
    }
    current_unmap(start_va, end_va);
    0
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
