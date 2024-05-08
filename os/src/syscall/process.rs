//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, TIMEVAL},
    loader::get_app_data_by_name,
    mm::{translated_byte_buffer, translated_refmut, translated_str, MapPermission, VirtAddr},
    task::{
        add_task, current_task, current_task_is_mapped, current_task_mmap, current_task_unmap, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus
    }, timer::{get_time_ms, get_time_us},
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
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    debug!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    debug!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    debug!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    debug!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);

    if let Some(data) = get_app_data_by_name(path.as_str()) {
        debug!("[sys_exec] - path: {}", path);
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        debug!("found pair");
        debug!("[is zombie]: {}", p.inner_exclusive_access().is_zombie());
        debug!("[pid]: {}", pid);
        debug!("[pid getpid]: {}", p.getpid());
        debug!("[pid == -1 || pid == p.getpid()]: {}", (pid == -1 || pid as usize == p.getpid()));
        debug!("[{}]", p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid()));
        debug!("");
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        debug!("children length: {}", inner.children.len());
        let child = inner.children.remove(idx);
        debug!("child strong count: {}", Arc::strong_count(&child));
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        debug!("child found pid: {}", found_pid);
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        debug!("child pid: [{}], exit code: [{}]", found_pid, exit_code);
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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

/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let ptr = _ti as *const u8;
    let len = core::mem::size_of::<TaskInfo>();
    let buffers = translated_byte_buffer(token, ptr, len);

    let binding = current_task().unwrap();
    let inner = binding.inner_exclusive_access();
    let task_info = TaskInfo {
        status: inner.task_status,
        syscall_times: inner.syscall_times,
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

pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    if !current_task_is_mapped(start_va, end_va, false) {
        return -1;
    }
    current_task_mmap(start_va, end_va, permissions);
    0
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let start_va = VirtAddr(_start);
    if !start_va.aligned() {
        return -1;
    }

    let end_va = VirtAddr(_start + _len);
    if !end_va.aligned() {
        return -1;
    }

    if !current_task_is_mapped(start_va, end_va, true) {
        return -1;
    }
    current_task_unmap(start_va, end_va);
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    debug!("sys_spawn: {:?}", _path);
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    
    let token = current_user_token();
    let path = translated_str(token, _path);

    let tcb_parent = current_task().unwrap();
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let (tcb_child, tcb_child_pid) = tcb_parent.fork_without_copy(data);
        add_task(tcb_child.clone());
        tcb_child_pid as isize
    } else {
        -1
    }

}

// TODO YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio < 2 {
        -1
    } else {
        _prio
    }
}
