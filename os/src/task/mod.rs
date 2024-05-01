//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::config::{MAX_SYSCALL_NUM, PAGE_SIZE};
use crate::loader::{get_app_data, get_num_app};
use crate::mm::{frame_alloc, frame_dealloc, MapPermission, PTEFlags, VirtAddr, VirtPageNum};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        next_task.start_time = get_time_ms();
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.tasks[next].start_time = get_time_ms();
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    ///
    fn current_task_mmap(&self, start: usize, num_pages: usize, pteflags: PTEFlags) -> isize {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        for i in 0..num_pages {
            let page_start = start + i * PAGE_SIZE;
            if (inner.tasks[current].memory_set.page_table).translate(VirtAddr(page_start).ceil()).is_some() {
                debug!("Mapped, {:?}", page_start);
                return -1;
            }

            let frame = match frame_alloc() {
                Some(f) => f,
                None => return -1
            };

            (inner.tasks[current].memory_set.page_table).map(VirtPageNum::from(page_start / PAGE_SIZE), frame.ppn,  pteflags);
        }
        0
    }

    ///
    fn current_task_unmap(&self, start: usize, num_pages: usize) -> isize {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        for i in 0..num_pages {
            let page_start = start + i * PAGE_SIZE;
            if (inner.tasks[current].memory_set.page_table).translate(VirtAddr(page_start).into()).is_none() {
                return -1;
            }
            
            frame_dealloc(inner.tasks[current].memory_set.page_table.root_ppn);

            (inner.tasks[current].memory_set.page_table).unmap(VirtPageNum::from(page_start / PAGE_SIZE));
        }
        0
    }
    /// Check Task is Mapped
    pub fn is_mapped(&self, start_va: VirtAddr, end_va: VirtAddr, mapped: bool) -> bool {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].memory_set.is_mapped(start_va, end_va, mapped)
    }
    /// Current task mmap
    pub fn current_mmap(&self, start_va: VirtAddr, end_va: VirtAddr, permissions: MapPermission) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].memory_set.insert_framed_area(start_va, end_va, permissions);

    }
    /// Current task unmmap
    pub fn current_unmap(&self, start_va: VirtAddr, end_va: VirtAddr) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].memory_set.remove_framed_area(start_va, end_va);
    }
    /// Current task status
    pub fn current_task_status(&self) -> TaskStatus {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let status = inner.tasks[current].task_status;
        drop(inner);
        status
    }
    /// Current task sysccall
    pub fn current_task_syscalls(&self) -> [u32; MAX_SYSCALL_NUM] {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let syscalls = inner.tasks[current].syscalls;
        drop(inner);
        syscalls
    }
    /// Current task syscalls increase
    pub fn current_task_syscalls_increase(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscalls[syscall_id] += 1;
        drop(inner);
    }
    /// Current task cost time
    pub fn current_task_cost_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let time = get_time_ms() - inner.tasks[current].start_time;
        drop(inner);
        time
    }
}
/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// test
pub fn current_task_mmap(start: usize, num_pages: usize, pteflags: PTEFlags) -> isize {
    TASK_MANAGER.current_task_mmap(start, num_pages, pteflags)
}

/// test
pub fn current_task_unmap(start: usize, num_pages: usize) -> isize {
    TASK_MANAGER.current_task_unmap(start, num_pages)
}

/// Check if mapped
pub fn current_is_mapped(start_va: VirtAddr, end_va:VirtAddr, mapped: bool) -> bool {
    TASK_MANAGER.is_mapped(start_va, end_va, mapped)
}

/// Current mmap
pub fn current_mmp(start_va: VirtAddr, end_va: VirtAddr, permissions: MapPermission) {
    TASK_MANAGER.current_mmap(start_va, end_va, permissions);
}

/// Current unmap
pub fn current_unmap(start_va: VirtAddr, end_va: VirtAddr) {
    TASK_MANAGER.current_unmap(start_va, end_va);
}

/// Current task status
pub fn current_task_status() -> TaskStatus {
    TASK_MANAGER.current_task_status()
}

/// Current task syscalls
pub fn current_task_syscalls() -> [u32; MAX_SYSCALL_NUM] {
    TASK_MANAGER.current_task_syscalls()
}

/// Current task sysccalls increase
pub fn current_task_syscalls_increase(syscall_id: usize) {
    TASK_MANAGER.current_task_syscalls_increase(syscall_id);
}

/// Current task cost time
pub fn current_task_cost_time() -> usize {
    TASK_MANAGER.current_task_cost_time()
}